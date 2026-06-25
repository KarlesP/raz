//! `raz monitor ...` — Azure Monitor metric values and activity-log events.

use clap::Subcommand;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use raz_core::arm::monitor;
use raz_core::error::{RazError, Result};
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum MonitorCommand {
    /// Metric values for a resource.
    Metrics {
        #[command(subcommand)]
        command: MetricsCommand,
    },
    /// Activity-log (management) events.
    ActivityLog {
        #[command(subcommand)]
        command: ActivityLogCommand,
    },
}

#[derive(Subcommand)]
pub enum MetricsCommand {
    /// List the latest value of one or more metrics for a resource.
    List {
        /// Full resource id.
        #[arg(long)]
        resource: String,
        /// Comma-separated metric names (e.g. "Percentage CPU").
        #[arg(long)]
        metrics: String,
        /// Average | Total | Minimum | Maximum | Count.
        #[arg(long, default_value = "Average")]
        aggregation: String,
        /// ISO 8601 interval, e.g. PT1M, PT1H.
        #[arg(long)]
        interval: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ActivityLogCommand {
    /// List recent activity-log events for the subscription.
    List {
        /// How many hours back to start from.
        #[arg(long, default_value_t = 1)]
        offset: i64,
        /// Maximum events to return.
        #[arg(long, default_value_t = 20)]
        max: usize,
    },
}

pub async fn run(command: MonitorCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        MonitorCommand::Metrics {
            command:
                MetricsCommand::List {
                    resource,
                    metrics,
                    aggregation,
                    interval,
                },
        } => {
            let (ctx, client, _sub) = arm_context(globals).await?;
            let value = monitor::metrics(
                &client,
                &resource,
                &metrics,
                &aggregation,
                interval.as_deref(),
            )
            .await?;
            emit(&ctx, value, Some(&monitor::metrics_table_spec()))
        }
        MonitorCommand::ActivityLog {
            command: ActivityLogCommand::List { offset, max },
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let end = OffsetDateTime::now_utc();
            let start = end - Duration::hours(offset);
            let to_iso = |t: OffsetDateTime| {
                t.format(&Rfc3339)
                    .map_err(|e| RazError::Other(format!("time format: {e}")))
            };
            let value =
                monitor::activity_log(&client, &sub, &to_iso(start)?, &to_iso(end)?, max).await?;
            emit(&ctx, value, Some(&monitor::activity_table_spec()))
        }
    }
}
