use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::{RunRecord, ScheduleConfig, ScheduleState, SqliteStore};

pub async fn migrate_yaml_to_sqlite(
    yaml_dir: &Path,
    state_json_path: &Path,
    history_dir: &Path,
    store: &SqliteStore,
) -> Result<usize> {
    if !yaml_dir.exists() {
        return Ok(0);
    }

    let configs: Vec<ScheduleConfig> = read_yaml_dir(yaml_dir)?;
    if configs.is_empty() {
        return Ok(0);
    }

    let states = load_state_json(state_json_path)?;

    let mut migrated = 0;
    for config in configs {
        if store
            .get_schedule_config(&config.schedule_id)
            .await?
            .is_some()
        {
            continue;
        }

        store.save_schedule_config(&config).await?;

        if let Some(state) = states.get(&config.schedule_id) {
            store.save_schedule_state(state).await?;
        }

        let history_file = history_dir.join(format!("{}.jsonl", config.schedule_id));
        if history_file.exists() {
            let records = read_jsonl_history(&history_file)?;
            for record in records {
                store.append_run_record(&record).await?;
            }
        }

        migrated += 1;
    }

    if migrated > 0 {
        tracing::info!(count = migrated, "migrated schedules from YAML to SQLite");

        let backup = yaml_dir.with_extension("d.migrated");
        if !backup.exists() {
            std::fs::rename(yaml_dir, &backup)?;
        }
    }

    Ok(migrated)
}

fn load_state_json(path: &Path) -> Result<HashMap<String, ScheduleState>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn read_yaml_dir<T>(dir: &Path) -> Result<Vec<T>>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let mut paths = Vec::new();
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read {}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("yaml") {
            paths.push(path);
        }
    }
    paths.sort();

    let mut items = Vec::with_capacity(paths.len());
    for path in paths {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let item = serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        items.push(item);
    }
    Ok(items)
}

fn read_jsonl_history(path: &Path) -> Result<Vec<RunRecord>> {
    let content = std::fs::read_to_string(path)?;
    let mut records = Vec::new();
    let mut skipped = 0usize;
    for line in content.lines() {
        match serde_json::from_str(line) {
            Ok(record) => records.push(record),
            Err(_) => skipped += 1,
        }
    }
    if skipped > 0 {
        tracing::warn!(
            path = %path.display(),
            skipped,
            "skipped malformed lines during JSONL history migration"
        );
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::Utc;
    use tempfile::TempDir;

    use crate::{
        DeliveryConfig, RunRecord, RunStatus, ScheduleConfig, ScheduleState, ScheduleType,
        SessionMode, SqliteStore, TaskPayload,
    };

    fn sample_config(schedule_id: &str) -> ScheduleConfig {
        ScheduleConfig {
            schedule_id: schedule_id.to_string(),
            enabled: true,
            name: format!("Schedule {schedule_id}"),
            description: Some("Migrated from YAML".to_string()),
            schedule: ScheduleType::Every {
                interval_ms: 60_000,
                anchor_ms: Some(0),
            },
            agent_id: "clawhive-main".to_string(),
            session_mode: SessionMode::Isolated,
            payload: Some(TaskPayload::AgentTurn {
                message: format!("run {schedule_id}"),
                model: None,
                thinking: None,
                timeout_seconds: 300,
                light_context: false,
            }),
            timeout_seconds: 300,
            delete_after_run: false,
            delivery: DeliveryConfig::default(),
        }
    }

    fn sample_state(schedule_id: &str) -> ScheduleState {
        ScheduleState {
            schedule_id: schedule_id.to_string(),
            next_run_at_ms: Some(1_700_000_000_000),
            running_at_ms: None,
            last_run_at_ms: Some(1_699_999_940_000),
            last_run_status: Some(RunStatus::Ok),
            last_error: None,
            last_duration_ms: Some(250),
            consecutive_errors: 0,
            last_delivery_status: None,
            last_delivery_error: None,
        }
    }

    fn sample_record(schedule_id: &str) -> RunRecord {
        let started_at = Utc::now();
        RunRecord {
            schedule_id: schedule_id.to_string(),
            started_at,
            ended_at: started_at,
            status: RunStatus::Ok,
            error: None,
            duration_ms: 42,
            response: Some("done".to_string()),
            session_key: Some("session:test".to_string()),
        }
    }

    #[tokio::test]
    async fn migrate_yaml_schedules_to_sqlite() {
        let temp_dir = TempDir::new().unwrap();
        let yaml_dir = temp_dir.path().join("config/schedules.d");
        let state_json_path = temp_dir.path().join("data/schedules/state.json");
        let history_dir = temp_dir.path().join("data/schedules/runs");
        let db_path = temp_dir.path().join("data/scheduler.db");
        let store = SqliteStore::open(&db_path).unwrap();

        fs::create_dir_all(&yaml_dir).unwrap();
        fs::create_dir_all(state_json_path.parent().unwrap()).unwrap();
        fs::create_dir_all(&history_dir).unwrap();

        let migrated_config = sample_config("daily-report");
        let skipped_config = sample_config("already-sqlite");
        let migrated_state = sample_state("daily-report");
        let migrated_record = sample_record("daily-report");

        fs::write(
            yaml_dir.join("daily-report.yaml"),
            serde_yaml::to_string(&migrated_config).unwrap(),
        )
        .unwrap();
        fs::write(
            yaml_dir.join("already-sqlite.yaml"),
            serde_yaml::to_string(&skipped_config).unwrap(),
        )
        .unwrap();
        fs::write(
            &state_json_path,
            serde_json::to_string(&std::collections::HashMap::from([(
                migrated_state.schedule_id.clone(),
                migrated_state.clone(),
            )]))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            history_dir.join("daily-report.jsonl"),
            format!("{}\n", serde_json::to_string(&migrated_record).unwrap()),
        )
        .unwrap();

        store.save_schedule_config(&skipped_config).await.unwrap();

        let migrated =
            super::migrate_yaml_to_sqlite(&yaml_dir, &state_json_path, &history_dir, &store)
                .await
                .unwrap();

        assert_eq!(migrated, 1);
        assert_eq!(
            store
                .get_schedule_config("daily-report")
                .await
                .unwrap()
                .unwrap(),
            migrated_config
        );
        assert_eq!(
            store.load_schedule_states().await.unwrap(),
            vec![migrated_state]
        );
        assert_eq!(
            store.recent_runs("daily-report", 10).await.unwrap(),
            vec![migrated_record]
        );

        let backup_dir = temp_dir.path().join("config/schedules.d.migrated");
        assert!(!yaml_dir.exists());
        assert!(backup_dir.exists());

        let rerun =
            super::migrate_yaml_to_sqlite(&backup_dir, &state_json_path, &history_dir, &store)
                .await
                .unwrap();
        assert_eq!(rerun, 0);
    }
}
