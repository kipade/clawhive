use std::path::Path;

use anyhow::Result;
use clawhive_core::SecurityMode;

use crate::runtime::bootstrap::{bootstrap, resolve_security_override};

pub(crate) async fn run(
    root: &Path,
    port: u16,
    security: Option<SecurityMode>,
    no_security: bool,
) -> Result<()> {
    let security_override = resolve_security_override(security, no_security);
    let _ = port;
    let (bus, _memory, gateway, _config, _schedule_manager, _wait_manager, approval_registry) =
        bootstrap(root, security_override).await?;
    clawhive_tui::run_code_tui(bus.as_ref(), gateway, Some(approval_registry)).await
}
