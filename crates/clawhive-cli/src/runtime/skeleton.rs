use std::path::Path;

use anyhow::Result;

/// If `config/main.yaml` does not exist, create a minimal skeleton so the
/// server can start and present the Web Setup Wizard.
pub(crate) fn ensure_skeleton_config(root: &Path, port: u16) -> Result<()> {
    let config_dir = root.join("config");
    let main_yaml = config_dir.join("main.yaml");

    if main_yaml.exists() {
        return Ok(());
    }

    // Create directory structure
    std::fs::create_dir_all(config_dir.join("agents.d"))?;
    std::fs::create_dir_all(config_dir.join("providers.d"))?;
    std::fs::create_dir_all(config_dir.join("schedules.d"))?;

    // config/main.yaml — channels disabled
    std::fs::write(
        &main_yaml,
        "app:\n  name: clawhive\n\nruntime:\n  max_concurrent: 4\n\nfeatures:\n  multi_agent: true\n  sub_agent: true\n  tui: true\n  cli: true\n\nchannels:\n  telegram:\n    enabled: false\n    connectors: []\n  discord:\n    enabled: false\n    connectors: []\n  feishu:\n    enabled: false\n    connectors: []\n  dingtalk:\n    enabled: false\n    connectors: []\n  wecom:\n    enabled: false\n    connectors: []\n\nembedding:\n  enabled: true\n  provider: auto\n  api_key: \"\"\n  model: text-embedding-3-small\n  dimensions: 1536\n  base_url: https://api.openai.com/v1\n\ntools: {}\n",
    )?;

    // config/routing.yaml
    std::fs::write(
        config_dir.join("routing.yaml"),
        "default_agent_id: clawhive-main\nbindings: []\n",
    )?;

    // config/agents.d/clawhive-main.yaml — placeholder, disabled
    std::fs::write(
        config_dir.join("agents.d/clawhive-main.yaml"),
        "agent_id: clawhive-main\nenabled: false\nidentity:\n  name: \"Clawhive\"\n  emoji: \"\\U0001F41D\"\nmodel_policy:\n  primary: \"\"\n  fallbacks: []\nmemory_policy:\n  mode: \"standard\"\n  write_scope: \"all\"\n",
    )?;

    // Workspace prompt templates (AGENTS.md, SOUL.md, etc.) are created
    // automatically by workspace.init_with_defaults() during agent startup.

    eprintln!();
    eprintln!("  \u{1F41D} First run detected — setup required.");
    eprintln!();
    eprintln!("     Open the Web Setup Wizard to get started:");
    eprintln!();
    eprintln!("       → http://localhost:{port}/setup");
    eprintln!();
    eprintln!("     Or use the CLI wizard: clawhive setup");
    eprintln!();

    Ok(())
}
