use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use clawhive_core::*;

use crate::runtime::bootstrap::bootstrap;

#[derive(Subcommand)]
pub(crate) enum SessionCommands {
    #[command(about = "Reset a session by key")]
    Reset {
        #[arg(help = "Session key")]
        session_key: String,
    },
}

pub(crate) async fn run(cmd: SessionCommands, root: &Path) -> Result<()> {
    let (_bus, memory, _gateway, _config, _schedule_manager, _wait_manager, _approval_registry) =
        bootstrap(root, None).await?;
    let session_mgr = SessionManager::new(memory, 1800);
    match cmd {
        SessionCommands::Reset { session_key } => {
            let key = clawhive_schema::SessionKey(session_key.clone());
            match session_mgr.reset(&key).await? {
                true => println!("Session '{session_key}' reset successfully."),
                false => println!("Session '{session_key}' not found."),
            }
        }
    }
    Ok(())
}
