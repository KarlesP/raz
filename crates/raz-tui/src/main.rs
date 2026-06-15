//! `raz-tui` — the interactive dashboard front-end. It reuses raz-core for all data and
//! auth logic (no duplication of the CLI's behavior) and layers a ratatui UI with
//! tachyonfx transitions on top: a login gate that renders the device code, then a
//! subscription browser and a VM/VNet resource explorer.

mod app;

use std::io;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyEventKind};

use app::App;

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
    let mut app = match App::new() {
        Ok(app) => app,
        Err(e) => {
            // Restore happens in main; surface the error after.
            return Err(io::Error::other(e.to_string()));
        }
    };

    let mut last = Instant::now();
    while !app.should_quit {
        let elapsed = last.elapsed();
        last = Instant::now();

        terminal.draw(|frame| app.draw(frame, elapsed))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key.code);
                }
            }
        }

        app.tick();
    }
    Ok(())
}
