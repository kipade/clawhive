//! Bottom pane module skeleton.

use uuid::Uuid;

use super::history::DiffHunk;

pub mod approval;
pub mod file_search;
pub mod input;
pub mod shortcuts;
pub mod slash_command;

/// Active mode for the bottom interaction pane.
#[allow(dead_code)]
pub(crate) enum BottomPaneState {
    Input,
    Approval(ApprovalRequest),
    ShortcutOverlay,
    SlashCommand(FilterState),
    FileSearch(FilterState),
}

/// Pending command approval state.
#[allow(dead_code)]
pub(crate) struct ApprovalRequest {
    pub trace_id: Uuid,
    pub command: String,
    pub agent_id: String,
    pub diff: Option<Vec<DiffHunk>>,
    pub selected_option: usize,
}

/// Shared list filtering state for pickers.
#[allow(dead_code)]
pub(crate) struct FilterState {
    pub query: String,
    pub selected: usize,
}

#[allow(clippy::derivable_impls)]
impl Default for BottomPaneState {
    fn default() -> Self {
        Self::Input
    }
}
