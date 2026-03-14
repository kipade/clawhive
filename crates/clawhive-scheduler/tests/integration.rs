use std::fs;
use std::sync::Arc;

use chrono::Utc;
use clawhive_bus::{EventBus, Topic};
use clawhive_scheduler::{
    apply_job_result, error_backoff_ms, migrate_yaml_to_sqlite, CompletedResult, DeliveryConfig,
    DeliveryMode, FailureDestination, RunStatus, ScheduleConfig, ScheduleEntry, ScheduleManager,
    ScheduleState, ScheduleType, SessionMode, SqliteStore, TaskPayload,
};
use clawhive_schema::BusMessage;
use tokio::time::{timeout, Duration};

fn sqlite_db_path(temp: &tempfile::TempDir) -> std::path::PathBuf {
    temp.path().join("data/scheduler.db")
}

fn lifecycle_schedule_config() -> ScheduleConfig {
    ScheduleConfig {
        schedule_id: "sqlite-lifecycle".to_string(),
        enabled: true,
        name: "SQLite Lifecycle".to_string(),
        description: Some("exercise manager persistence".to_string()),
        schedule: ScheduleType::Every {
            interval_ms: 60_000,
            anchor_ms: Some(0),
        },
        agent_id: "test-agent".to_string(),
        session_mode: SessionMode::Main,
        payload: Some(TaskPayload::AgentTurn {
            message: "run lifecycle check".to_string(),
            model: Some("openai/gpt-5".to_string()),
            thinking: Some("medium".to_string()),
            timeout_seconds: 123,
            light_context: true,
        }),
        timeout_seconds: 456,
        delete_after_run: false,
        delivery: DeliveryConfig {
            mode: DeliveryMode::Announce,
            channel: Some("discord".to_string()),
            connector_id: Some("discord-main".to_string()),
            source_channel_type: Some("discord".to_string()),
            source_connector_id: Some("discord-main".to_string()),
            source_conversation_scope: Some("guild:1:channel:2".to_string()),
            source_user_scope: Some("user:99".to_string()),
            webhook_url: None,
            failure_destination: Some(FailureDestination {
                channel: Some("telegram".to_string()),
                connector_id: Some("telegram-main".to_string()),
                conversation_scope: Some("chat:123".to_string()),
            }),
            best_effort: true,
        },
    }
}

#[tokio::test]
async fn schedule_full_lifecycle_sqlite() {
    let temp = tempfile::TempDir::new().unwrap();
    let bus = Arc::new(EventBus::new(32));
    let store = SqliteStore::open(&sqlite_db_path(&temp)).unwrap();
    let manager = ScheduleManager::new(store, Arc::clone(&bus)).await.unwrap();

    let config = lifecycle_schedule_config();
    let schedule_id = config.schedule_id.clone();

    manager.add_schedule(config).await.unwrap();

    let list = manager.list().await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].config.schedule_id, schedule_id);
    assert_eq!(list[0].config.name, "SQLite Lifecycle");

    manager
        .update_schedule(
            &schedule_id,
            &serde_json::json!({
                "name": "SQLite Lifecycle Updated",
                "description": "updated through manager"
            }),
        )
        .await
        .unwrap();

    let reloaded_store = SqliteStore::open(&sqlite_db_path(&temp)).unwrap();
    let reloaded_manager = ScheduleManager::new(reloaded_store, bus).await.unwrap();
    let persisted = reloaded_manager.get_schedule(&schedule_id).await.unwrap();
    assert_eq!(persisted.config.name, "SQLite Lifecycle Updated");
    assert_eq!(
        persisted.config.description.as_deref(),
        Some("updated through manager")
    );

    reloaded_manager
        .remove_schedule(&schedule_id)
        .await
        .unwrap();
    assert!(reloaded_manager.list().await.is_empty());
}

#[tokio::test]
async fn yaml_migration_then_manager_works() {
    let temp = tempfile::TempDir::new().unwrap();
    let yaml_dir = temp.path().join("config/schedules.d");
    let state_json_path = temp.path().join("data/schedules/state.json");
    let history_dir = temp.path().join("data/schedules/runs");
    let db_path = sqlite_db_path(&temp);

    fs::create_dir_all(&yaml_dir).unwrap();
    fs::create_dir_all(state_json_path.parent().unwrap()).unwrap();
    fs::create_dir_all(&history_dir).unwrap();

    let config = lifecycle_schedule_config();
    let schedule_id = config.schedule_id.clone();
    let persisted_state = ScheduleState {
        schedule_id: schedule_id.clone(),
        next_run_at_ms: Some(Utc::now().timestamp_millis() + 60_000),
        running_at_ms: None,
        last_run_at_ms: Some(Utc::now().timestamp_millis() - 60_000),
        last_run_status: Some(RunStatus::Ok),
        last_error: None,
        last_duration_ms: Some(321),
        consecutive_errors: 0,
        last_delivery_status: None,
        last_delivery_error: None,
    };

    fs::write(
        yaml_dir.join("sqlite-lifecycle.yaml"),
        serde_yaml::to_string(&config).unwrap(),
    )
    .unwrap();
    fs::write(
        &state_json_path,
        serde_json::to_string(&std::collections::HashMap::from([(
            schedule_id.clone(),
            persisted_state.clone(),
        )]))
        .unwrap(),
    )
    .unwrap();

    let store = SqliteStore::open(&db_path).unwrap();
    let migrated = migrate_yaml_to_sqlite(&yaml_dir, &state_json_path, &history_dir, &store)
        .await
        .unwrap();

    assert_eq!(migrated, 1);

    let manager = ScheduleManager::new(store, Arc::new(EventBus::new(32)))
        .await
        .unwrap();
    let loaded = manager.get_schedule(&schedule_id).await.unwrap();
    assert_eq!(loaded.config, config);
    assert_eq!(loaded.state.last_run_at_ms, persisted_state.last_run_at_ms);
    assert_eq!(
        loaded.state.last_duration_ms,
        persisted_state.last_duration_ms
    );

    manager
        .update_schedule(
            &schedule_id,
            &serde_json::json!({ "name": "Migrated And Updated" }),
        )
        .await
        .unwrap();
    assert_eq!(
        manager
            .get_schedule(&schedule_id)
            .await
            .unwrap()
            .config
            .name,
        "Migrated And Updated"
    );

    assert!(!yaml_dir.exists());
    assert!(temp.path().join("config/schedules.d.migrated").exists());
}

#[tokio::test]
async fn complex_config_survives_sqlite_roundtrip() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = SqliteStore::open(&sqlite_db_path(&temp)).unwrap();

    let config = ScheduleConfig {
        schedule_id: "complex-roundtrip".to_string(),
        enabled: true,
        name: "Complex Roundtrip".to_string(),
        description: Some("all optional fields populated".to_string()),
        schedule: ScheduleType::Cron {
            expr: "15 8 * * 1-5".to_string(),
            tz: "Asia/Tokyo".to_string(),
        },
        agent_id: "clawhive-main".to_string(),
        session_mode: SessionMode::Main,
        payload: Some(TaskPayload::AgentTurn {
            message: "summarize every weekday morning".to_string(),
            model: Some("anthropic/claude-sonnet-4".to_string()),
            thinking: Some("high".to_string()),
            timeout_seconds: 987,
            light_context: true,
        }),
        timeout_seconds: 654,
        delete_after_run: true,
        delivery: DeliveryConfig {
            mode: DeliveryMode::Announce,
            channel: Some("telegram".to_string()),
            connector_id: Some("telegram-main".to_string()),
            source_channel_type: Some("discord".to_string()),
            source_connector_id: Some("discord-main".to_string()),
            source_conversation_scope: Some("guild:9:channel:88".to_string()),
            source_user_scope: Some("user:42".to_string()),
            webhook_url: Some("https://example.com/scheduler-hook".to_string()),
            failure_destination: Some(FailureDestination {
                channel: Some("slack".to_string()),
                connector_id: Some("slack-primary".to_string()),
                conversation_scope: Some("workspace:1:channel:alerts".to_string()),
            }),
            best_effort: true,
        },
    };

    store.save_schedule_config(&config).await.unwrap();

    let loaded = store.load_schedule_configs().await.unwrap();
    assert_eq!(loaded, vec![config]);
}

#[test]
fn cron_schedule_config_loads_with_defaults() {
    let yaml = r#"
schedule_id: test-daily
enabled: true
name: "Test Daily"
schedule:
  kind: cron
  expr: "0 9 * * *"
  tz: "Asia/Shanghai"
agent_id: clawhive-main
session_mode: isolated
payload:
  kind: agent_turn
  message: "Test task"
  timeout_seconds: 300
"#;

    let config: ScheduleConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.schedule_id, "test-daily");
    assert!(config.enabled);
    assert!(matches!(
        config.delivery.mode,
        clawhive_scheduler::DeliveryMode::None
    ));
}

#[tokio::test]
async fn schedule_manager_triggers_bus_event() {
    let temp = tempfile::TempDir::new().unwrap();
    let db_path = temp.path().join("data/scheduler.db");
    let store = SqliteStore::open(&db_path).unwrap();

    let yaml = r#"
schedule_id: test-immediate
enabled: true
name: "Immediate"
schedule:
  kind: every
  interval_ms: 100
agent_id: test-agent
session_mode: isolated
payload:
  kind: agent_turn
  message: "Hello from test"
  timeout_seconds: 300
"#;
    let config: ScheduleConfig = serde_yaml::from_str(yaml).unwrap();
    store.save_schedule_config(&config).await.unwrap();

    let bus = Arc::new(EventBus::new(32));
    let mut rx = bus.subscribe(Topic::ScheduledTaskTriggered).await;
    let manager = ScheduleManager::new(store, Arc::clone(&bus)).await.unwrap();
    let handle = tokio::spawn(async move {
        manager.run().await;
    });

    let msg = timeout(Duration::from_secs(2), rx.recv()).await;
    handle.abort();

    assert!(msg.is_ok());
    assert!(matches!(
        msg.unwrap().unwrap(),
        BusMessage::ScheduledTaskTriggered { schedule_id, .. } if schedule_id == "test-immediate"
    ));
}

#[tokio::test]
async fn schedule_manager_propagates_session_mode_to_bus_event() {
    let temp = tempfile::TempDir::new().unwrap();
    let db_path = temp.path().join("data/scheduler.db");
    let store = SqliteStore::open(&db_path).unwrap();

    let yaml = r#"
schedule_id: test-main-mode
enabled: true
name: "Main Mode"
schedule:
  kind: every
  interval_ms: 60000
agent_id: test-agent
session_mode: main
payload:
  kind: agent_turn
  message: "Hello from main mode"
  timeout_seconds: 300
"#;
    let config: ScheduleConfig = serde_yaml::from_str(yaml).unwrap();
    store.save_schedule_config(&config).await.unwrap();

    let bus = Arc::new(EventBus::new(32));
    let manager = ScheduleManager::new(store, Arc::clone(&bus)).await.unwrap();
    let mut rx = bus.subscribe(Topic::ScheduledTaskTriggered).await;

    manager.trigger_now("test-main-mode").await.unwrap();

    let msg = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("timed out waiting for trigger")
        .expect("channel closed");

    assert!(matches!(
        msg,
        BusMessage::ScheduledTaskTriggered {
            schedule_id,
            session_mode: clawhive_schema::ScheduledSessionMode::Main,
            ..
        } if schedule_id == "test-main-mode"
    ));
}

#[test]
fn error_backoff_and_state_transition_work() {
    let mut entry = ScheduleEntry {
        config: ScheduleConfig {
            schedule_id: "retry-test".to_string(),
            enabled: true,
            name: "Retry test".to_string(),
            description: None,
            schedule: ScheduleType::Every {
                interval_ms: 1_000,
                anchor_ms: Some(0),
            },
            agent_id: "clawhive-main".to_string(),
            session_mode: SessionMode::Isolated,
            payload: Some(TaskPayload::AgentTurn {
                message: "run".to_string(),
                model: None,
                thinking: None,
                timeout_seconds: 300,
                light_context: false,
            }),
            timeout_seconds: 300,
            delete_after_run: false,
            delivery: DeliveryConfig::default(),
        },
        state: ScheduleState {
            schedule_id: "retry-test".to_string(),
            next_run_at_ms: Some(Utc::now().timestamp_millis() + 1_000),
            running_at_ms: Some(Utc::now().timestamp_millis() - 500),
            last_run_at_ms: None,
            last_run_status: None,
            last_error: None,
            last_duration_ms: None,
            consecutive_errors: 0,
            last_delivery_status: None,
            last_delivery_error: None,
        },
    };

    let result = CompletedResult {
        status: RunStatus::Error,
        error: Some("timeout".to_string()),
        started_at_ms: 1_000,
        ended_at_ms: 2_000,
        duration_ms: 1_000,
    };

    assert_eq!(error_backoff_ms(1), 30_000);
    assert_eq!(error_backoff_ms(5), 3_600_000);

    let should_delete = apply_job_result(&mut entry, &result);
    assert!(!should_delete);
    assert_eq!(entry.state.consecutive_errors, 1);
    assert!(entry.state.next_run_at_ms.unwrap() >= result.ended_at_ms + 30_000);
}
