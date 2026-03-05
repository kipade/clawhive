//! Conversation history primitives.

use std::time::Duration;

use chrono::{DateTime, Local};
use uuid::Uuid;

/// A single entry rendered in the history pane.
#[allow(dead_code)]
pub(crate) enum HistoryCell {
    UserMessage {
        text: String,
        timestamp: DateTime<Local>,
    },
    AssistantText {
        /// Accumulated streaming text (markdown source).
        text: String,
        is_streaming: bool,
    },
    Thinking {
        text: String,
        collapsed: bool,
    },
    ToolCall {
        tool_name: String,
        arguments: String,
        output: Option<ToolOutput>,
        duration: Option<Duration>,
        is_running: bool,
    },
    Error {
        trace_id: Uuid,
        message: String,
    },
}

/// Output payload shown for a tool call in history.
#[allow(dead_code)]
pub(crate) enum ToolOutput {
    /// Plain text lines (already truncated upstream).
    Text(Vec<String>),
    /// Unified diff for file edits.
    Diff {
        file_path: String,
        hunks: Vec<DiffHunk>,
    },
}

/// One unified-diff hunk section.
#[allow(dead_code)]
pub(crate) struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

/// A line within a diff hunk.
#[allow(dead_code)]
pub(crate) enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}
