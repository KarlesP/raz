//! `raz rest` — make an arbitrary authenticated REST call against ARM or Microsoft Graph.
//! Mirrors `az rest`: the bearer token is chosen by the URL host, and an ARM-relative path
//! (starting with `/`) is resolved against the management endpoint.

use clap::Args;
use serde_json::Value;

use raz_core::context::Context;
use raz_core::error::{usage, RazError, Result};
use raz_core::GlobalArgs;

use super::emit;

#[derive(Args)]
pub struct RestArgs {
    /// HTTP method (GET, POST, PUT, PATCH, DELETE).
    #[arg(long, short = 'm', default_value = "GET")]
    method: String,
    /// Full URL (https://…), or an ARM-relative path beginning with `/`
    /// (resolved against https://management.azure.com). Include `?api-version=…` for ARM.
    #[arg(long, short = 'u')]
    url: String,
    /// Request body as JSON, for POST/PUT/PATCH.
    #[arg(long, short = 'b')]
    body: Option<String>,
}

pub async fn run(args: RestArgs, globals: GlobalArgs) -> Result<()> {
    let ctx = Context::load(globals)?;

    // ARM-relative paths resolve against the management endpoint.
    let url = if args.url.starts_with('/') {
        format!("https://management.azure.com{}", args.url)
    } else {
        args.url.clone()
    };

    // Pick the token audience from the host: Graph vs ARM (default).
    let token = if url.contains("graph.microsoft.com") {
        ctx.graph_token().await?
    } else {
        ctx.subscription_and_token().await?.1
    };

    let method = reqwest::Method::from_bytes(args.method.to_uppercase().as_bytes())
        .map_err(|_| usage(format!("invalid --method '{}'", args.method)))?;

    let mut req = ctx.http.request(method, &url).bearer_auth(token);
    if let Some(body) = &args.body {
        req = req
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.clone());
    }

    let resp = req.send().await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(RazError::Http(format!("{} {url}: {text}", status.as_u16())));
    }

    // Render JSON through the output formatter; print non-JSON bodies verbatim.
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => emit(&ctx, value, None),
        Err(_) => {
            if !text.trim().is_empty() {
                println!("{text}");
            }
            Ok(())
        }
    }
}
