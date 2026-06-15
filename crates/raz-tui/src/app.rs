//! Dashboard state machine, rendering, and input handling.
//!
//! Async raz-core calls are driven from this synchronous UI loop via a small Tokio runtime
//! (`rt.block_on(...)`), which keeps the render/event loop simple while reusing the exact
//! same auth/ARM code the CLI uses. tachyonfx effects are added on every view transition.

use std::time::{Duration as StdDuration, Instant};

use ratatui::crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use serde_json::Value;
use tachyonfx::{fx, EffectManager, Interpolation};
use tokio::runtime::Runtime;

use raz_core::arm::client::{discover_all, ArmClient};
use raz_core::arm::{vm, vnet};
use raz_core::auth::device_code::{self, DeviceCodeResponse, PollOutcome};
use raz_core::auth::{credential, now_unix};
use raz_core::config::{Profile, Subscription};
use raz_core::context::new_http_client;

/// Top-level screens.
enum View {
    Login,
    Subscriptions,
    Resources,
}

/// Which resource list is showing in the Resources view.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ResTab {
    Vms,
    Vnets,
}

/// Login sub-state: the device code prompt plus polling bookkeeping.
struct LoginState {
    device: DeviceCodeResponse,
    next_poll: Instant,
    interval: u64,
    status: String,
}

pub struct App {
    pub should_quit: bool,
    rt: Runtime,
    http: reqwest::Client,
    profile: Profile,

    view: View,
    login: Option<LoginState>,

    subs_state: ListState,
    res_tab: ResTab,
    vms: Vec<Value>,
    vnets: Vec<Value>,
    res_state: ListState,
    message: String,

    effects: EffectManager<()>,
}

impl App {
    pub fn new() -> raz_core::Result<Self> {
        let rt = Runtime::new().map_err(|e| raz_core::RazError::Other(e.to_string()))?;
        let http = new_http_client();
        let profile = Profile::load()?;

        // Start at the resource browser if we already hold a valid token, else login.
        let logged_in = profile
            .token
            .as_ref()
            .map(|t| !t.is_expired(now_unix(), 60))
            .unwrap_or(false);

        let mut subs_state = ListState::default();
        if !profile.subscriptions.is_empty() {
            subs_state.select(Some(0));
        }

        let mut app = App {
            should_quit: false,
            rt,
            http,
            profile,
            view: View::Login,
            login: None,
            subs_state,
            res_tab: ResTab::Vms,
            vms: Vec::new(),
            vnets: Vec::new(),
            res_state: ListState::default(),
            message: String::new(),
            effects: EffectManager::default(),
        };

        if logged_in {
            app.goto_subscriptions();
        } else {
            app.begin_login();
        }
        Ok(app)
    }

    /// Short fade-in applied to the whole screen on each view transition.
    fn transition_effect(&mut self) {
        let fx = fx::fade_to(
            Color::Rgb(0, 120, 212), // Azure blue
            Color::Reset,
            (350, Interpolation::QuadOut),
        );
        self.effects.add_effect(fx);
    }

    fn begin_login(&mut self) {
        self.view = View::Login;
        let tenant = self
            .profile
            .tenant_id
            .clone()
            .unwrap_or_else(|| "organizations".to_string());
        match self
            .rt
            .block_on(device_code::request_device_code(&self.http, &tenant))
        {
            Ok(device) => {
                let interval = device.interval.max(1);
                self.login = Some(LoginState {
                    next_poll: Instant::now() + StdDuration::from_secs(interval),
                    interval,
                    status: "Waiting for you to complete sign-in in the browser...".to_string(),
                    device,
                });
            }
            Err(e) => {
                self.message = format!("Failed to start login: {e}");
            }
        }
        self.transition_effect();
    }

    fn goto_subscriptions(&mut self) {
        self.view = View::Subscriptions;
        if self.subs_state.selected().is_none() && !self.profile.subscriptions.is_empty() {
            self.subs_state.select(Some(0));
        }
        self.transition_effect();
    }

    fn goto_resources(&mut self) {
        self.view = View::Resources;
        self.res_tab = ResTab::Vms;
        self.load_resources();
        self.transition_effect();
    }

    /// Fetch VMs and VNets for the selected subscription using the same raz-core ops the
    /// CLI calls.
    fn load_resources(&mut self) {
        let (sub, tenant) = match self.selected_subscription() {
            Some(s) => (s.id.clone(), s.tenant_id.clone()),
            None => {
                self.message = "No subscription selected".to_string();
                return;
            }
        };
        // Mint a token for the subscription's tenant (subscriptions may span tenants).
        let token = match self.token_for_tenant(&tenant) {
            Some(t) => t,
            None => {
                self.message = "Token expired — restart and log in again".to_string();
                return;
            }
        };
        let client = ArmClient::with_token(self.http.clone(), token);

        self.vms = self
            .rt
            .block_on(vm::list(&client, &sub))
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        self.vnets = self
            .rt
            .block_on(vnet::list(&client, &sub))
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();

        self.res_state.select(if self.current_list().is_empty() {
            None
        } else {
            Some(0)
        });
        self.message = format!("{} VM(s), {} VNet(s)", self.vms.len(), self.vnets.len());
    }

    fn selected_subscription(&self) -> Option<&Subscription> {
        self.subs_state
            .selected()
            .and_then(|i| self.profile.subscriptions.get(i))
    }

    /// Mint an ARM token for `tenant` from the stored refresh token, falling back to the
    /// cached access token. Mirrors `Context::token_for_tenant` for the TUI's sync loop.
    fn token_for_tenant(&self, tenant: &str) -> Option<String> {
        let cached = self.profile.token.as_ref()?;
        if !tenant.is_empty() {
            if let Some(refresh) = &cached.refresh_token {
                if let Ok(tok) = self.rt.block_on(device_code::exchange_refresh_token(
                    &self.http, tenant, refresh,
                )) {
                    return Some(tok.access_token);
                }
            }
        }
        if cached.is_expired(now_unix(), 60) {
            None
        } else {
            Some(cached.access_token.clone())
        }
    }

    fn current_list(&self) -> &[Value] {
        match self.res_tab {
            ResTab::Vms => &self.vms,
            ResTab::Vnets => &self.vnets,
        }
    }

    // ------------------------------------------------------------------ input

    pub fn handle_key(&mut self, code: KeyCode) {
        // Global quit.
        if matches!(code, KeyCode::Char('q')) {
            self.should_quit = true;
            return;
        }
        match self.view {
            View::Login => {
                if let KeyCode::Esc = code {
                    self.should_quit = true;
                }
            }
            View::Subscriptions => self.handle_subscriptions_key(code),
            View::Resources => self.handle_resources_key(code),
        }
    }

    fn handle_subscriptions_key(&mut self, code: KeyCode) {
        let len = self.profile.subscriptions.len();
        match code {
            KeyCode::Down | KeyCode::Char('j') => move_selection(&mut self.subs_state, len, 1),
            KeyCode::Up | KeyCode::Char('k') => move_selection(&mut self.subs_state, len, -1),
            KeyCode::Enter if self.selected_subscription().is_some() => self.goto_resources(),
            _ => {}
        }
    }

    fn handle_resources_key(&mut self, code: KeyCode) {
        let len = self.current_list().len();
        match code {
            KeyCode::Down | KeyCode::Char('j') => move_selection(&mut self.res_state, len, 1),
            KeyCode::Up | KeyCode::Char('k') => move_selection(&mut self.res_state, len, -1),
            KeyCode::Tab | KeyCode::Char('t') => {
                self.res_tab = match self.res_tab {
                    ResTab::Vms => ResTab::Vnets,
                    ResTab::Vnets => ResTab::Vms,
                };
                self.res_state.select(if self.current_list().is_empty() {
                    None
                } else {
                    Some(0)
                });
                self.transition_effect();
            }
            KeyCode::Esc | KeyCode::Char('b') => self.goto_subscriptions(),
            KeyCode::Char('r') => self.load_resources(),
            _ => {}
        }
    }

    // ------------------------------------------------------------------- tick

    /// Per-frame background work: poll the device-code token endpoint while on the login
    /// screen.
    pub fn tick(&mut self) {
        if !matches!(self.view, View::Login) {
            return;
        }
        let Some(login) = self.login.as_mut() else {
            return;
        };
        if Instant::now() < login.next_poll {
            return;
        }

        let tenant = self
            .profile
            .tenant_id
            .clone()
            .unwrap_or_else(|| "organizations".to_string());
        let device_code = login.device.device_code.clone();
        let outcome = self.rt.block_on(device_code::poll_token_once(
            &self.http,
            &tenant,
            &device_code,
        ));

        match outcome {
            Ok(PollOutcome::Pending) => {
                login.next_poll = Instant::now() + StdDuration::from_secs(login.interval);
            }
            Ok(PollOutcome::SlowDown) => {
                login.interval += 5;
                login.next_poll = Instant::now() + StdDuration::from_secs(login.interval);
            }
            Ok(PollOutcome::Granted(token)) => {
                self.complete_login(*token, tenant);
            }
            Err(e) => {
                login.status = format!("Login failed: {e}. Press Esc to quit.");
            }
        }
    }

    fn complete_login(&mut self, token: device_code::TokenResponse, tenant: String) {
        self.profile.tenant_id = Some(tenant);
        self.profile.token = Some(credential::cache_from_response(&token));
        let _ = self.profile.save();

        // Cross-tenant discovery, same as the CLI's `raz login`.
        if let Ok((_tenants, subs)) = self.rt.block_on(discover_all(&self.http, &token)) {
            self.profile.subscriptions = subs;
            let _ = self.profile.save();
            if !self.profile.subscriptions.is_empty() {
                self.subs_state.select(Some(0));
            }
        }
        self.login = None;
        self.goto_subscriptions();
    }

    // ------------------------------------------------------------------- draw

    pub fn draw(&mut self, frame: &mut Frame, elapsed: StdDuration) {
        let area = frame.area();
        match self.view {
            View::Login => self.draw_login(frame, area),
            View::Subscriptions => self.draw_subscriptions(frame, area),
            View::Resources => self.draw_resources(frame, area),
        }
        // Apply queued tachyonfx effects over the rendered frame.
        self.effects
            .process_effects(elapsed.into(), frame.buffer_mut(), area);
    }

    fn draw_login(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);

        frame.render_widget(title_block(" raz — sign in "), chunks[0]);

        let body = match &self.login {
            Some(l) => format!(
                "{}\n\nUser code: {}\nVerification URL: {}\n\n{}",
                l.device.message, l.device.user_code, l.device.verification_uri, l.status
            ),
            None => format!("Could not start device-code login.\n\n{}", self.message),
        };
        let para = Paragraph::new(body).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Device code "),
        );
        frame.render_widget(para, chunks[1]);

        frame.render_widget(footer("Esc/q: quit"), chunks[2]);
    }

    fn draw_subscriptions(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

        frame.render_widget(title_block(" raz — subscriptions "), chunks[0]);

        let items: Vec<ListItem> = self
            .profile
            .subscriptions
            .iter()
            .map(|s| {
                let marker = if s.is_default { "★ " } else { "  " };
                ListItem::new(format!("{marker}{}  ({})", s.name, s.id))
            })
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Subscriptions "),
            )
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan))
            .highlight_symbol("➤ ");
        frame.render_stateful_widget(list, chunks[1], &mut self.subs_state);

        frame.render_widget(
            footer("↑/↓: navigate   Enter: open resources   q: quit"),
            chunks[2],
        );
    }

    fn draw_resources(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

        let sub_name = self
            .selected_subscription()
            .map(|s| s.name.clone())
            .unwrap_or_default();
        frame.render_widget(title_block(&format!(" raz — {sub_name} ")), chunks[0]);

        let tabs = Tabs::new(vec!["VMs", "VNets"])
            .select(match self.res_tab {
                ResTab::Vms => 0,
                ResTab::Vnets => 1,
            })
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(tabs, chunks[1]);

        // Split body into list + detail.
        let body = Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(chunks[2]);

        let items: Vec<ListItem> = self
            .current_list()
            .iter()
            .map(|item| ListItem::new(str_field(item, "name")))
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(list_title(self.res_tab)),
            )
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan))
            .highlight_symbol("➤ ");
        frame.render_stateful_widget(list, body[0], &mut self.res_state);

        let detail = self
            .res_state
            .selected()
            .and_then(|i| self.current_list().get(i))
            .map(detail_text)
            .unwrap_or_else(|| "Select an item to see details.".to_string());
        frame.render_widget(
            Paragraph::new(detail)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title(" Details ")),
            body[1],
        );

        frame.render_widget(
            footer(&format!(
                "Tab: switch   ↑/↓: navigate   r: refresh   b/Esc: back   q: quit   [{}]",
                self.message
            )),
            chunks[3],
        );
    }
}

// ---------------------------------------------------------------- ui helpers

fn move_selection(state: &mut ListState, len: usize, delta: i32) {
    if len == 0 {
        state.select(None);
        return;
    }
    let cur = state.selected().unwrap_or(0) as i32;
    let next = (cur + delta).rem_euclid(len as i32);
    state.select(Some(next as usize));
}

fn title_block(title: &str) -> Paragraph<'_> {
    Paragraph::new(title)
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Rgb(0, 120, 212))),
        )
}

fn footer(text: &str) -> Paragraph<'_> {
    Paragraph::new(text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL))
}

fn list_title(tab: ResTab) -> &'static str {
    match tab {
        ResTab::Vms => " Virtual machines ",
        ResTab::Vnets => " Virtual networks ",
    }
}

fn str_field(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or("<unnamed>")
        .to_string()
}

fn detail_text(item: &Value) -> String {
    let mut lines = vec![
        format!("Name:           {}", str_field(item, "name")),
        format!("Resource group: {}", str_field(item, "resourceGroup")),
        format!("Location:       {}", str_field(item, "location")),
        format!("Type:           {}", str_field(item, "type")),
    ];
    if let Some(state) = item
        .get("properties")
        .and_then(|p| p.get("provisioningState"))
        .and_then(Value::as_str)
    {
        lines.push(format!("Provisioning:   {state}"));
    }
    lines.push(String::new());
    lines.push("Id:".to_string());
    lines.push(str_field(item, "id"));
    lines.join("\n")
}
