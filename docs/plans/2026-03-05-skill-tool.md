# Skill Tool Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a dedicated `skill` tool so agents can discover and read skill instructions without needing filesystem access outside their workspace.

**Architecture:** New `SkillTool` struct implements `ToolExecutor`. Registered in the global `ToolRegistry`. Agent sees skill list in system prompt (existing `summary_prompt()`), calls `skill` tool with name to get full SKILL.md content. Orchestrator reads from `skills_root` directly — no workspace permission changes.

**Tech Stack:** Rust, clawhive-core crate, existing `ToolExecutor` trait + `SkillRegistry`

---

### Task 1: Create SkillTool

**Files:**
- Create: `crates/clawhive-core/src/skill_tool.rs`
- Modify: `crates/clawhive-core/src/lib.rs` (add `mod skill_tool;`)

**Step 1: Write the failing test**

```rust
// In skill_tool.rs at the bottom
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{ToolContext, ToolExecutor};
    use std::fs;

    fn create_test_skills_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let weather_dir = dir.path().join("weather");
        fs::create_dir_all(&weather_dir).unwrap();
        fs::write(
            weather_dir.join("SKILL.md"),
            "---\nname: weather\ndescription: Get weather forecasts\n---\n\n# Weather Skill\n\nUse `curl wttr.in` to get weather.",
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn execute_returns_skill_content() {
        let dir = create_test_skills_dir();
        let tool = SkillTool::new(dir.path().to_path_buf());
        let ctx = ToolContext::default();
        let input = serde_json::json!({"name": "weather"});
        let output = tool.execute(input, &ctx).await.unwrap();
        assert!(!output.is_error);
        assert!(output.content.contains("# Weather Skill"));
        assert!(output.content.contains("curl wttr.in"));
    }

    #[tokio::test]
    async fn execute_returns_error_for_unknown_skill() {
        let dir = create_test_skills_dir();
        let tool = SkillTool::new(dir.path().to_path_buf());
        let ctx = ToolContext::default();
        let input = serde_json::json!({"name": "nonexistent"});
        let output = tool.execute(input, &ctx).await.unwrap();
        assert!(output.is_error);
        assert!(output.content.contains("not found"));
        assert!(output.content.contains("weather")); // lists available skills
    }

    #[tokio::test]
    async fn definition_has_correct_schema() {
        let dir = create_test_skills_dir();
        let tool = SkillTool::new(dir.path().to_path_buf());
        let def = tool.definition();
        assert_eq!(def.name, "skill");
        assert!(def.description.contains("skill"));
        let required = def.input_schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "name"));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p clawhive-core skill_tool --lib`
Expected: FAIL — module `skill_tool` doesn't exist yet

**Step 3: Write the implementation**

```rust
// crates/clawhive-core/src/skill_tool.rs
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use clawhive_provider::ToolDef;

use crate::skill::SkillRegistry;
use crate::tool::{ToolContext, ToolExecutor, ToolOutput};

pub struct SkillTool {
    skills_root: PathBuf,
}

impl SkillTool {
    pub fn new(skills_root: PathBuf) -> Self {
        Self { skills_root }
    }
}

#[async_trait]
impl ToolExecutor for SkillTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "skill".into(),
            description: "Read the full instructions for an available skill. Pass the skill name from the Available Skills list in the system prompt to get its complete SKILL.md content.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the skill to read (from Available Skills list)"
                    }
                },
                "required": ["name"]
            }),
        }
    }

    async fn execute(&self, input: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let name = input["name"]
            .as_str()
            .unwrap_or("")
            .trim();

        if name.is_empty() {
            return Ok(ToolOutput {
                content: "Error: skill name is required.".into(),
                is_error: true,
            });
        }

        let registry = SkillRegistry::load_from_dir(&self.skills_root).unwrap_or_default();
        match registry.get(name) {
            Some(skill) => Ok(ToolOutput {
                content: skill.content.clone(),
                is_error: false,
            }),
            None => {
                let available: Vec<_> = registry
                    .available()
                    .iter()
                    .map(|s| s.name.clone())
                    .collect();
                let list = if available.is_empty() {
                    "No skills are currently available.".to_string()
                } else {
                    format!("Available skills: {}", available.join(", "))
                };
                Ok(ToolOutput {
                    content: format!("Skill '{name}' not found. {list}"),
                    is_error: true,
                })
            }
        }
    }
}
```

Add module declaration in `lib.rs`:
```rust
pub mod skill_tool;
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p clawhive-core skill_tool --lib`
Expected: PASS (3 tests)

**Step 5: Commit**

```bash
git add crates/clawhive-core/src/skill_tool.rs crates/clawhive-core/src/lib.rs
git commit -m "feat: add SkillTool for agents to read skill instructions"
```

---

### Task 2: Register SkillTool in Orchestrator

**Files:**
- Modify: `crates/clawhive-core/src/orchestrator.rs`

**Step 1: Add registration**

In `Orchestrator::new()`, after line 171 (`tool_registry.register(Box::new(ScheduleTool::new(schedule_manager)));`), add:

```rust
tool_registry.register(Box::new(crate::skill_tool::SkillTool::new(
    workspace_root.join("skills"),
)));
```

**Step 2: Verify compilation**

Run: `cargo build -p clawhive-core`
Expected: Compiles with no errors

**Step 3: Commit**

```bash
git add crates/clawhive-core/src/orchestrator.rs
git commit -m "feat: register SkillTool in orchestrator tool registry"
```

---

### Task 3: Update summary_prompt to reference the skill tool

**Files:**
- Modify: `crates/clawhive-core/src/skill.rs`

**Step 1: Update the summary_prompt method and test**

Change `summary_prompt()` (line 196-209 in `skill.rs`):

From:
```rust
lines.push(
    "\nTo use a skill, read the full SKILL.md for detailed instructions.".to_string(),
);
```

To:
```rust
lines.push(
    "\nTo use a skill, call the `skill` tool with the skill name to read its full instructions.".to_string(),
);
```

Update the existing test `summary_prompt_formats_correctly` to match.

**Step 2: Run tests**

Run: `cargo test -p clawhive-core summary_prompt --lib`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/clawhive-core/src/skill.rs
git commit -m "fix: update summary_prompt to reference skill tool instead of SKILL.md path"
```

---

### Task 4: Full build + test verification

**Step 1: Run full crate tests**

Run: `cargo test -p clawhive-core --lib`
Expected: All tests pass

**Step 2: Run full workspace build**

Run: `cargo build`
Expected: Compiles with no errors

**Step 3: Check for lint issues**

Run: `cargo clippy -p clawhive-core -- -D warnings`
Expected: No warnings
