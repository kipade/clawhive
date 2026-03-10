use std::path::Path;

use anyhow::Result;
use chrono::TimeZone;
use clap::Subcommand;

use crate::runtime::bootstrap::{bootstrap, format_schedule_type};

#[derive(Subcommand)]
pub(crate) enum ScheduleCommands {
    #[command(about = "List all scheduled tasks with status")]
    List,
    #[command(about = "Trigger a scheduled task immediately")]
    Run {
        #[arg(help = "Schedule ID")]
        schedule_id: String,
    },
    #[command(about = "Enable a disabled schedule")]
    Enable {
        #[arg(help = "Schedule ID")]
        schedule_id: String,
    },
    #[command(about = "Disable a schedule")]
    Disable {
        #[arg(help = "Schedule ID")]
        schedule_id: String,
    },
    #[command(about = "Show recent run history for a schedule")]
    History {
        #[arg(help = "Schedule ID")]
        schedule_id: String,
        #[arg(long, default_value = "10")]
        limit: usize,
    },
}

pub(crate) async fn run(cmd: ScheduleCommands, root: &Path) -> Result<()> {
    let (_bus, _memory, _gateway, _config, schedule_manager, _wait_manager, _approval_registry) =
        bootstrap(root, None).await?;
    match cmd {
        ScheduleCommands::List => {
            let entries = schedule_manager.list().await;
            println!(
                "{:<24} {:<8} {:<24} {:<26} {:<8}",
                "ID", "ENABLED", "SCHEDULE", "NEXT RUN", "ERRORS"
            );
            println!("{}", "-".repeat(96));
            for entry in entries {
                let next_run = entry
                    .state
                    .next_run_at_ms
                    .and_then(|ms| chrono::Utc.timestamp_millis_opt(ms).single())
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "{:<24} {:<8} {:<24} {:<26} {:<8}",
                    entry.config.schedule_id,
                    if entry.config.enabled { "yes" } else { "no" },
                    format_schedule_type(&entry.config.schedule),
                    next_run,
                    entry.state.consecutive_errors,
                );
            }
        }
        ScheduleCommands::Run { schedule_id } => {
            schedule_manager.trigger_now(&schedule_id).await?;
            println!("Triggered schedule '{schedule_id}'.");
        }
        ScheduleCommands::Enable { schedule_id } => {
            schedule_manager.set_enabled(&schedule_id, true).await?;
            println!("Enabled schedule '{schedule_id}'.");
        }
        ScheduleCommands::Disable { schedule_id } => {
            schedule_manager.set_enabled(&schedule_id, false).await?;
            println!("Disabled schedule '{schedule_id}'.");
        }
        ScheduleCommands::History { schedule_id, limit } => {
            let records = schedule_manager.recent_history(&schedule_id, limit).await?;
            if records.is_empty() {
                println!("No history for schedule '{schedule_id}'.");
            } else {
                for record in records {
                    println!(
                        "{} | {:>6}ms | {:?} | {}",
                        record.started_at.to_rfc3339(),
                        record.duration_ms,
                        record.status,
                        record.error.as_deref().unwrap_or("-"),
                    );
                }
            }
        }
    }
    Ok(())
}
