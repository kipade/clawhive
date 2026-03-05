//! Code TUI module skeleton.

use std::collections::VecDeque;
use std::time::Instant;

pub mod bottom_pane;
pub mod diff;
pub mod footer;
pub mod header;
pub mod history;
pub mod markdown;
pub mod scroll;
pub mod shimmer;

use self::bottom_pane::{ApprovalRequest, BottomPaneState};
use self::history::HistoryCell;
use self::scroll::ScrollState;

/// Core application state for the new code-mode TUI.
#[allow(dead_code)]
pub(crate) struct CodeApp {
    pub history: Vec<HistoryCell>,
    pub bottom_pane: BottomPaneState,
    pub approval_queue: VecDeque<ApprovalRequest>,

    pub history_scroll: ScrollState,

    pub input: String,
    pub input_history: Vec<String>,
    pub queued_message: Option<String>,

    pub is_running: bool,
    pub agent_id: String,
    pub model_name: String,
    pub token_count: u64,
    pub cost_usd: f64,
    pub context_used_pct: u8,

    pub verbose: bool,

    pub should_quit: bool,
    pub quit_pressed_at: Option<Instant>,
}

#[allow(dead_code)]
impl CodeApp {
    pub(crate) fn new(agent_id: String, model_name: String) -> Self {
        Self {
            history: Vec::new(),
            bottom_pane: BottomPaneState::default(),
            approval_queue: VecDeque::new(),
            history_scroll: ScrollState::new(),
            input: String::new(),
            input_history: Vec::new(),
            queued_message: None,
            is_running: false,
            agent_id,
            model_name,
            token_count: 0,
            cost_usd: 0.0,
            context_used_pct: 0,
            verbose: false,
            should_quit: false,
            quit_pressed_at: None,
        }
    }
}
