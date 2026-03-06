# Code TUI Redesign — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current minimal `clawhive code` TUI with a production-quality coding agent interface modeled after Codex CLI's architecture, incorporating the best UX patterns from Claude Code and OpenCode.

**Architecture:** Full-screen Ratatui TUI with two-zone layout (scrollable History pane + fixed Bottom Pane). The Bottom Pane is a state machine that swaps between input, approval, and overlay views. All rendering uses a `Renderable` trait for composability. The existing EventBus integration is preserved.

**Tech Stack:** Ratatui 0.29, Crossterm 0.28, `tui-textarea` (multi-line input), `pulldown-cmark` (markdown rendering), `syntect` (syntax highlighting), `diffy` (diff generation)

---

## Context: Current State

The current `code` TUI lives entirely in `crates/clawhive-tui/src/lib.rs` (1385 lines, single file). It has:
- A `CodeApp` struct with `Vec<String>` for conversation and logs
- Three fixed vertical regions: Conversation (60%), Input (4 lines), Task Logs (remaining)
- A centered popup overlay for approvals
- No streaming support (deltas append to logs as truncated lines)
- No diff display, no markdown rendering, no file completion
- Single-line input with no editing capabilities

The public API is `run_code_tui(bus: &EventBus, gateway: Arc<Gateway>, approval_registry: Option<Arc<ApprovalRegistry>>)`.

---

## Design Specification

### 1. Overall Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│  Header (1 line)                                                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  History Pane (scrollable, flex-grow)                                │
│  - Welcome screen when empty                                        │
│  - HistoryCell list when conversation is active                     │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│  Bottom Pane (fixed height, state-machine driven)                   │
│  - InputView (default)                                              │
│  - ApprovalView (when permission needed)                            │
│  - ShortcutOverlay (when ? pressed)                                 │
│  - SlashCommandPicker (when / typed)                                │
│  - FileSearchPicker (when @ typed)                                  │
├─────────────────────────────────────────────────────────────────────┤
│  Footer (1 line)                                                    │
└─────────────────────────────────────────────────────────────────────┘
```

**Constraints:**
- Header: `Constraint::Length(1)`
- History: `Constraint::Min(4)` (flex-grow, takes remaining space)
- Bottom Pane: `Constraint::Length(N)` where N is dynamic per view (input: 4-8, approval: 10-14, overlay: varies)
- Footer: `Constraint::Length(1)`

### 2. Header

```
  🐝 clawhive-main · claude-4-opus                  1,234 tokens · $0.05
```

- Left: agent emoji + agent ID + model name
- Right: cumulative token count + cumulative cost
- Style: agent name in Bold, model in Dim, token/cost in Dim
- Data source: track tokens/cost from `StreamDelta` and `ReplyReady` messages

### 3. History Pane

The history pane is a scrollable container holding a list of `HistoryCell` entries. Each cell is one of:

#### 3.1 Welcome Screen (empty session)

Shown when `history_cells` is empty. Vertically centered:

```
                     🐝
                   clawhive

          Your AI agent, ready to help.

       / commands   @ files   ! shell   ? shortcuts
```

#### 3.2 User Message Cell

```
  ┃ > Fix the authentication bug in session.rs
```

- Left border: `┃` in Cyan
- Content: user's message text, word-wrapped
- Prefix: `>` in Bold

#### 3.3 Assistant Text Cell

AI response text rendered as markdown with syntax highlighting:
- Inline code: `code` in a distinct style
- Code blocks: syntax-highlighted with `syntect`, language label displayed
- Bold, italic, headings: rendered with appropriate Ratatui modifiers
- Word-wrapped to available width

During streaming: text accumulates line-by-line (newline-gated). The last incomplete line is not rendered until a newline arrives. The pane auto-scrolls to bottom (sticky scroll).

#### 3.4 Thinking Cell (collapsible)

```
  ╭─ Thinking ────────────────────────────────────────────────
  │ The session expiry check compares timestamps incorrectly...
  ╰───────────────────────────────────────────────────────────
```

- Default: collapsed, shows just `╭─ Thinking ─╮` as a single line
- Expanded: shows full reasoning text in Dim style with `│` left border
- Toggle: not interactively toggleable in v1 (always show collapsed; expanded in verbose mode via `Ctrl+O`)

#### 3.5 Tool Call Cell

```
  ⏺ Read src/auth/session.rs                                   0.3s
    ⎿ 45 lines
```

- Icon: `⏺` in Cyan
- Tool name + arguments on first line
- Duration on the right (Dim, right-aligned)
- Output: indented under `⎿`, truncated to 5 lines max (with "N more lines" indicator)
- While running: show shimmer animation on placeholder text

**Shimmer animation:** A cosine-based brightness wave sweeps across placeholder text ("thinking...") every 2 seconds. Uses time-based animation, no extra threads needed — computed on each render tick (50ms poll interval).

#### 3.6 Tool Call Cell with Diff

When the tool is `Edit` or `Write`, the output shows a unified diff:

```
  ⏺ Edit src/auth/session.rs                                   0.3s
    ╭──────────────────────────────────────────────────────────
    │ @@ -42,3 +42,3 @@
    │ 42 │  fn is_expired(&self) -> bool {
    │ 43 │- ····self.expires_at > Utc::now()                     ← red bg
    │    │+ ····self.expires_at < Utc::now()                     ← green bg
    │ 44 │  }
    ╰──────────────────────────────────────────────────────────
```

- Diff background colors: theme-aware (dark terminal: muted tints; light terminal: GitHub-style pastels)
- Line numbers in Dim
- Added lines: green background
- Removed lines: red background
- Syntax highlighting on diff content via `syntect`

#### 3.7 Error Cell

```
  ✗ Task failed: compilation error in session.rs                 Red
    ⎿ error[E0308]: mismatched types...
```

- Icon: `✗` in Red
- Error message in Red Bold
- Details indented in Dim

#### 3.8 Scroll Behavior

- **Sticky scroll:** When user is at the bottom, new content auto-scrolls to keep bottom visible
- **Manual scroll:** `PageUp`/`PageDown` or mouse wheel to scroll history
- **Scroll break:** When user scrolls up manually, sticky scroll disengages. It re-engages when user scrolls back to bottom.
- **Scroll indicator:** Show `↑ N more` at top of pane when scrolled down from top

### 4. Bottom Pane — State Machine

The bottom pane renders exactly one view at a time, determined by `BottomPaneState`:

```rust
enum BottomPaneState {
    Input,           // Default: text input
    Approval,        // Permission request (replaces input)
    ShortcutOverlay, // ? key pressed (replaces input)
    SlashCommand,    // / typed (overlay above input)
    FileSearch,      // @ typed (overlay above input)
}
```

Transitions:
- `Input` → `Approval`: when `NeedHumanApproval` bus message arrives
- `Approval` → `Input`: after user decides (or Esc)
- `Input` → `ShortcutOverlay`: when `?` pressed on empty input
- `ShortcutOverlay` → `Input`: any key
- `Input` → `SlashCommand`: when `/` typed as first character
- `SlashCommand` → `Input`: Esc or selection
- `Input` → `FileSearch`: when `@` typed
- `FileSearch` → `Input`: Esc or selection

#### 4.1 InputView

```
  ┃ › Ask clawhive anything...                              (placeholder)
  ┃ › Fix the auth bug in @src/auth/session.rs              (with text)
   agent: clawhive-main                        100% context ◉
```

- Left border: `┃` in agent accent color (Cyan)
- Prompt symbol: `›` in Bold
- Multi-line input using `tui-textarea` crate
  - `Shift+Enter` or `Ctrl+J`: insert newline
  - `Enter`: submit (when input is non-empty)
  - `Backspace`, `Ctrl+W` (delete word), `Ctrl+U` (delete line): standard editing
  - `↑` on empty input: recall last sent message
  - Height: min 2 lines, max 8 lines, auto-grows with content
- Placeholder text (Dim) when empty: "Ask clawhive anything..."

**When agent is running:**
```
  ┃ ░░░░░░░░░░░░░░░░░░░░░░░░░░░                              (shimmer)
  ┃                                                   thinking
   agent: clawhive-main          esc interrupt   72% context ◉
```

- Input is replaced by shimmer animation
- `esc` to interrupt the running task
- `Tab` to queue a follow-up message (shown after current task completes)

**Shell mode (! prefix):**
```
  ┃ ! cargo test --lib auth                                   (blue ┃)
```

- When input starts with `!`, left border color changes to Blue
- On submit, the command is executed directly via the agent's shell tool
- Output appears as a Tool Call Cell in history

#### 4.2 ApprovalView (Exec)

Replaces the input area when a `NeedHumanApproval` message arrives:

```
  ┃ △ Permission required
  ┃
  ┃   $ rm -rf target/ && cargo build --release
  ┃
  ┃   › [y] Allow once
  ┃     [a] Allow for session
  ┃     [A] Always allow
  ┃     [n] Deny
  ┃     [?] Explain this command
  ┃
   ↑↓ select   enter confirm   esc deny         72% context ◉
```

- Left border: `┃` in Yellow (warning color)
- Triangle icon: `△` in Yellow Bold
- Command displayed in Bold White, truncated with `...` if > available width
- Options: vertical list, current selection highlighted with `›` and Bold
- Keyboard: `↑`/`↓` navigate, `Enter` confirms, `Esc` denies, or press shortcut letter directly (`y`/`a`/`A`/`n`/`?`)
- `?` option: sends explanation request to agent, returns to Input after response
- Height: dynamic, based on number of options + command display

**Approval queue:** Multiple pending approvals are processed sequentially. After resolving one, the next automatically appears. A counter `(1/3)` shows position in queue.

#### 4.3 ApprovalView (File Edit with Diff)

When the approval is for a file edit, the diff is shown inline:

```
  ┃ △ Edit src/auth/session.rs
  ┃
  ┃   ╭──────────────────────────────────────────────────
  ┃   │ @@ -42,3 +42,3 @@
  ┃   │ 42 │  fn is_expired(&self) -> bool {
  ┃   │ 43 │- ····self.expires_at > Utc::now()
  ┃   │    │+ ····self.expires_at < Utc::now()
  ┃   │ 44 │  }
  ┃   ╰──────────────────────────────────────────────────
  ┃
  ┃   › [y] Accept   [n] Reject   [d] Full diff
  ┃
   ↑↓ scroll diff   enter confirm   esc reject   72% context ◉
```

- Diff box: rounded corners `╭╰` with `│` border
- Diff is scrollable within the approval view if it exceeds available height
- `d` opens full-screen diff view (temporarily replaces entire screen)
- Theme-aware background colors for added/removed lines

#### 4.4 ShortcutOverlay

Shown when `?` is pressed on empty input:

```
   Keyboard Shortcuts
   ─────────────────────────────────────────────────────────
   /           slash commands       @          file paths
   !           shell command        ?          this help
   shift+enter newline              tab        queue message
   ctrl+g      external editor      ctrl+v     paste image
   esc         interrupt / back     ctrl+c     quit
   esc esc     rewind checkpoint    ctrl+l     clear screen
   ctrl+o      toggle verbose

   press any key to dismiss                    72% context ◉
```

- Any keypress dismisses and returns to InputView
- Height: fixed based on content

#### 4.5 SlashCommandPicker

Overlay floating above the input area when `/` is typed:

```
  ┃ › /co█
  ┃   ┌─────────────────────────────────────────────────┐
  ┃   │ › /compact     Compress conversation history    │
  ┃   │   /context     Show context window usage        │
  ┃   │   /cost        Show token usage and cost        │
  ┃   │   /config      Show current configuration       │
  ┃   └─────────────────────────────────────────────────┘
```

- Fuzzy matching on command name as user types
- `↑`/`↓` to navigate, `Enter` to select, `Esc` to dismiss
- Shows command description next to each entry (Dim)
- Max 8 visible items, scrollable if more

**Slash commands (v1):**

| Command | Description |
|---------|-------------|
| `/compact` | Compress conversation history to free context |
| `/context` | Show context window usage breakdown |
| `/cost` | Show token usage and cost summary |
| `/diff` | Show files changed this session |
| `/clear` | Clear screen (keep history) |
| `/model` | Show current model info |
| `/help` | Show help and available commands |
| `/exit` | Exit the TUI |

#### 4.6 FileSearchPicker

Overlay floating above input when `@` is typed:

```
  ┃ › Refactor @src/auth/█
  ┃   ┌─────────────────────────────────┐
  ┃   │ › src/auth/session.rs           │
  ┃   │   src/auth/token.rs             │
  ┃   │   src/auth/middleware.rs         │
  ┃   │   src/auth/mod.rs               │
  ┃   └─────────────────────────────────┘
```

- Searches workspace files by fuzzy match on the text after `@`
- `↑`/`↓` to navigate, `Enter`/`Tab` to insert selected path, `Esc` to dismiss
- Max 8 visible items
- File search uses `glob` or walks the workspace directory (respecting .gitignore)

### 5. Footer

Single line at the very bottom, content driven by context:

**When idle (input focused):**
```
  agent: clawhive-main                                  100% context ◉
```

**When agent is running:**
```
  agent: clawhive-main            esc interrupt    72% context ◉
```

**When in approval:**
```
  ↑↓ select   enter confirm   esc deny             72% context ◉
```

- Left: agent name (always shown)
- Center: contextual hints (only when relevant)
- Right: context window percentage + indicator dot (◉ green = <80%, ◉ yellow = 80-95%, ◉ red = >95%)

### 6. Keyboard Shortcuts (Complete)

#### Global (always active)
| Key | Action |
|-----|--------|
| `Ctrl+C` | Quit (requires double-press within 1s) |
| `Ctrl+L` | Clear screen (keep conversation history) |
| `Ctrl+O` | Toggle verbose mode (expand tool outputs and thinking) |

#### Input Mode
| Key | Action |
|-----|--------|
| `Enter` | Submit message |
| `Shift+Enter` / `Ctrl+J` | Insert newline |
| `Esc` | Interrupt running agent / clear input |
| `Esc Esc` | Open rewind checkpoint menu (future) |
| `↑` | Recall previous message (on empty input) |
| `/` | Trigger slash command picker (as first char) |
| `@` | Trigger file search picker |
| `!` | Shell mode prefix |
| `?` | Show shortcuts overlay (on empty input) |
| `Tab` | Queue message (while agent is running) |
| `Ctrl+G` | Open input in external `$EDITOR` |
| `Ctrl+W` | Delete word backwards |
| `Ctrl+U` | Delete to beginning of line |
| `PageUp` / `PageDown` | Scroll history pane |

#### Approval Mode
| Key | Action |
|-----|--------|
| `y` | Allow once |
| `a` | Allow for session |
| `A` | Always allow |
| `n` | Deny |
| `?` | Explain command |
| `↑` / `↓` | Navigate options |
| `Enter` | Confirm selected option |
| `Esc` | Deny and return to input |

### 7. Color Specification

Strict ANSI color palette for maximum terminal compatibility:

| Element | Color | Notes |
|---------|-------|-------|
| Agent accent / left border | Cyan | User messages, input border |
| User message prefix `>` | Bold White | |
| AI response text | Default fg | |
| Tool call icon `⏺` | Cyan | |
| Tool output `⎿` | Dim | |
| Duration / secondary text | Dim (DarkGray) | |
| Thinking block | Dim | |
| Diff added line bg | Green (muted) | Dark: `#213A2B`, Light: `#dafbe1` |
| Diff removed line bg | Red (muted) | Dark: `#4A221D`, Light: `#ffebe9` |
| Warning / approval `△` | Yellow | |
| Error `✗` | Red | |
| Success | Green | |
| Shell mode `!` border | Blue | |
| Shimmer | Animated Dim→Default sweep | |
| Footer hints | DarkGray | |
| Selected option `›` | Bold White | |

**Avoid:** Custom RGB colors except for diff backgrounds and shimmer animation. Never use ANSI blue or yellow as foreground text.

### 8. Data Model

```rust
/// A single entry in the history pane.
enum HistoryCell {
    UserMessage {
        text: String,
        timestamp: DateTime<Local>,
    },
    AssistantText {
        parts: Vec<TextPart>,      // accumulated streaming parts
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

enum ToolOutput {
    Text(Vec<String>),            // truncated lines
    Diff {
        file_path: String,
        hunks: Vec<DiffHunk>,     // parsed unified diff
    },
}

/// Bottom pane state machine
enum BottomPaneState {
    Input,
    Approval(ApprovalRequest),
    ShortcutOverlay,
    SlashCommand(FilterState),
    FileSearch(FilterState),
}

struct ApprovalRequest {
    trace_id: Uuid,
    command: String,
    agent_id: String,
    diff: Option<Vec<DiffHunk>>,  // for file edit approvals
    selected_option: usize,
}

/// Main app state
struct CodeApp {
    history: Vec<HistoryCell>,
    bottom_pane: BottomPaneState,
    approval_queue: VecDeque<ApprovalRequest>,

    // Scroll state
    history_scroll: ScrollState,   // tracks position + sticky mode

    // Input state (delegated to tui-textarea)
    input: TextArea<'static>,
    input_history: Vec<String>,    // previously sent messages
    queued_message: Option<String>, // message queued while agent is running

    // Agent state
    is_running: bool,
    agent_id: String,
    model_name: String,
    token_count: u64,
    cost_usd: f64,
    context_used_pct: u8,

    // Display settings
    verbose: bool,                 // Ctrl+O toggle

    should_quit: bool,
    quit_pressed_at: Option<Instant>, // for double-press Ctrl+C
}
```

### 9. EventBus Integration

The existing bus subscription model is preserved. Messages are mapped to `HistoryCell` entries:

| BusMessage | Action |
|------------|--------|
| `HandleIncomingMessage` | (ignored in code mode — we originate messages) |
| `MessageAccepted { trace_id }` | Mark agent as running, show shimmer |
| `StreamDelta { trace_id, delta, is_final }` | Append to current `AssistantText` cell; if `is_final`, mark streaming done |
| `ReplyReady { outbound }` | Finalize assistant message cell, mark agent as not running |
| `TaskFailed { trace_id, error }` | Add `Error` cell, mark agent as not running |
| `NeedHumanApproval { trace_id, command, agent_id, .. }` | Push to `approval_queue`, switch bottom pane to `Approval` if not already |
| `MemoryWriteRequested` | Show as Tool Call cell ("Memory Write") in verbose mode |
| `ConsolidationCompleted` | Ignore in code mode |

**Stream assembly:** `StreamDelta` messages with `is_final=false` accumulate into the current `AssistantText` cell. The markdown renderer processes the accumulated text on each render, but only commits complete lines (newline-gated). When `is_final=true` or `ReplyReady` arrives, the cell is finalized.

**Tool call detection:** Tool calls are detected from structured delta content. The format depends on the LLM provider, but the orchestrator should emit tool call events as distinct bus messages (or structured within StreamDelta). If the current architecture doesn't support this, tool calls are shown inline in the assistant text.

> **Note for implementer:** Check if `BusMessage` has a variant for tool call events. If not, tool calls will initially render as part of the assistant text. A future enhancement can add `BusMessage::ToolCallStarted` and `BusMessage::ToolCallCompleted` variants.

### 10. File Structure

The current single-file `lib.rs` (1385 lines) should be split into a module tree:

```
crates/clawhive-tui/src/
├── lib.rs                    # Public API: run_code_tui(), run_tui()
├── code/
│   ├── mod.rs                # CodeApp state + main loop
│   ├── history.rs            # HistoryCell enum + rendering
│   ├── bottom_pane/
│   │   ├── mod.rs            # BottomPaneState machine
│   │   ├── input.rs          # InputView (tui-textarea wrapper)
│   │   ├── approval.rs       # ApprovalView (exec + diff)
│   │   ├── shortcuts.rs      # ShortcutOverlay
│   │   ├── slash_command.rs  # SlashCommandPicker
│   │   └── file_search.rs    # FileSearchPicker
│   ├── header.rs             # Header rendering
│   ├── footer.rs             # Footer rendering (state-machine driven)
│   ├── markdown.rs           # Markdown → Ratatui Lines (pulldown-cmark)
│   ├── diff.rs               # Diff rendering (theme-aware, syntax-highlighted)
│   ├── shimmer.rs            # Shimmer animation
│   └── scroll.rs             # Scroll state (sticky scroll logic)
├── dashboard/
│   ├── mod.rs                # Existing dashboard TUI (App, BusReceivers, etc.)
│   └── ...                   # Extract from current lib.rs
└── shared/
    ├── mod.rs
    ├── approval_overlay.rs   # Shared approval rendering (if needed)
    └── styles.rs             # Color constants, shared styles
```

### 11. New Dependencies

Add to `crates/clawhive-tui/Cargo.toml`:

```toml
tui-textarea = "0.7"          # Multi-line text input widget
pulldown-cmark = "0.12"       # Markdown parsing
syntect = "5"                 # Syntax highlighting
two-face = "0.4"              # Embedded syntect theme data
diffy = "0.4"                 # Diff generation
```

### 12. Rendering Architecture

Each visual component implements a common pattern:

```rust
trait Renderable {
    /// How many rows this component needs given the available width.
    fn desired_height(&self, width: u16) -> u16;

    /// Render into the given area.
    fn render(&self, area: Rect, buf: &mut Buffer);
}
```

The main render loop:
1. Compute Header height (1)
2. Compute Footer height (1)
3. Compute Bottom Pane height via `desired_height(width)`
4. History Pane gets remaining space
5. Render each in order: Header → History → Bottom Pane → Footer

The 50ms poll interval is preserved (`event::poll(Duration::from_millis(50))`), which also drives shimmer animation updates (no additional timers needed).

---

## Implementation Tasks

### Task 1: Scaffold Module Structure

**Files:**
- Create: `crates/clawhive-tui/src/code/mod.rs`
- Create: `crates/clawhive-tui/src/code/history.rs`
- Create: `crates/clawhive-tui/src/code/header.rs`
- Create: `crates/clawhive-tui/src/code/footer.rs`
- Create: `crates/clawhive-tui/src/code/scroll.rs`
- Create: `crates/clawhive-tui/src/code/shimmer.rs`
- Create: `crates/clawhive-tui/src/code/markdown.rs`
- Create: `crates/clawhive-tui/src/code/diff.rs`
- Create: `crates/clawhive-tui/src/code/bottom_pane/mod.rs`
- Create: `crates/clawhive-tui/src/code/bottom_pane/input.rs`
- Create: `crates/clawhive-tui/src/code/bottom_pane/approval.rs`
- Create: `crates/clawhive-tui/src/code/bottom_pane/shortcuts.rs`
- Create: `crates/clawhive-tui/src/code/bottom_pane/slash_command.rs`
- Create: `crates/clawhive-tui/src/code/bottom_pane/file_search.rs`
- Create: `crates/clawhive-tui/src/dashboard/mod.rs`
- Create: `crates/clawhive-tui/src/shared/mod.rs`
- Create: `crates/clawhive-tui/src/shared/styles.rs`
- Modify: `crates/clawhive-tui/src/lib.rs`
- Modify: `crates/clawhive-tui/Cargo.toml`

**Step 1:** Add new dependencies to Cargo.toml (`tui-textarea`, `pulldown-cmark`, `syntect`, `two-face`, `diffy`).

**Step 2:** Create all module files with minimal placeholder content (empty structs, stub functions). Each file should compile.

**Step 3:** Extract existing dashboard code from `lib.rs` into `dashboard/mod.rs` — move `App`, `Panel`, `BusReceivers`, `subscribe_all`, `run_tui`, `run_tui_from_receivers`, `run_app`, `ui`, `render_list_panel` and related functions. Keep `lib.rs` as the re-export surface.

**Step 4:** Create `shared/styles.rs` with color constants and the `Renderable` trait.

**Step 5:** Verify `cargo build -p clawhive-tui` compiles and existing tests pass.

**Step 6:** Commit: `refactor(tui): scaffold module structure for code TUI redesign`

### Task 2: Core Data Model + CodeApp State

**Files:**
- Modify: `crates/clawhive-tui/src/code/mod.rs`
- Modify: `crates/clawhive-tui/src/code/history.rs`
- Modify: `crates/clawhive-tui/src/code/scroll.rs`

**Step 1:** Define `HistoryCell` enum in `history.rs` with all variants (UserMessage, AssistantText, Thinking, ToolCall, Error).

**Step 2:** Define `ToolOutput` enum (Text, Diff) and `DiffHunk` struct.

**Step 3:** Define `BottomPaneState` enum in `bottom_pane/mod.rs`.

**Step 4:** Define `ScrollState` in `scroll.rs` with sticky scroll logic:
- `scroll_to_bottom()`, `scroll_up(n)`, `scroll_down(n)`, `is_at_bottom()`, `visible_offset(total_items, viewport_height)`

**Step 5:** Define new `CodeApp` struct in `code/mod.rs` with all fields from the data model section.

**Step 6:** Write unit tests for `ScrollState` (sticky behavior, manual scroll break/re-engage).

**Step 7:** Verify: `cargo test -p clawhive-tui`

**Step 8:** Commit: `feat(tui): define code TUI data model and scroll state`

### Task 3: Header + Footer Rendering

**Files:**
- Modify: `crates/clawhive-tui/src/code/header.rs`
- Modify: `crates/clawhive-tui/src/code/footer.rs`
- Modify: `crates/clawhive-tui/src/shared/styles.rs`

**Step 1:** Implement `render_header(frame, area, app)` — left-aligned agent info, right-aligned token/cost.

**Step 2:** Implement `render_footer(frame, area, app)` with state-machine logic:
- Idle: agent name + context %
- Running: agent name + "esc interrupt" + context %
- Approval: navigation hints + context %
- Context indicator dot: ◉ green/yellow/red based on `context_used_pct`

**Step 3:** Write snapshot tests (use `ratatui::buffer::Buffer` to assert rendered output matches expected).

**Step 4:** Commit: `feat(tui): implement header and footer rendering`

### Task 4: Shimmer Animation

**Files:**
- Modify: `crates/clawhive-tui/src/code/shimmer.rs`

**Step 1:** Implement `shimmer_spans(text: &str, width: u16) -> Vec<Span>`:
- Time-based cosine wave (2-second period)
- Sweep brightness from dim → default → dim across characters
- Use `Instant::now()` since process start for phase calculation

**Step 2:** Write a test that verifies shimmer produces varying styles across character positions.

**Step 3:** Commit: `feat(tui): implement shimmer animation for loading state`

### Task 5: Markdown Renderer

**Files:**
- Modify: `crates/clawhive-tui/src/code/markdown.rs`

**Step 1:** Implement `render_markdown(source: &str, width: u16) -> Vec<Line>`:
- Parse with `pulldown-cmark`
- Map events to Ratatui `Line`/`Span`:
  - `Heading` → Bold, preceded by blank line
  - `Paragraph` → wrapped text
  - `Code` (inline) → distinct style (e.g., Magenta or reverse video)
  - `CodeBlock` → syntax-highlighted with `syntect`, bordered
  - `Strong` → Bold modifier
  - `Emphasis` → Italic modifier
  - `List` → `•` prefix with indentation
- Word-wrap to available width

**Step 2:** Implement `commit_complete_lines(buffer: &str) -> Vec<Line>` for streaming:
- Only render up to the last `\n` in buffer
- Return newly completed lines since last call

**Step 3:** Write tests: markdown with headings, code blocks, lists, inline code.

**Step 4:** Commit: `feat(tui): implement markdown-to-ratatui renderer`

### Task 6: Diff Renderer

**Files:**
- Modify: `crates/clawhive-tui/src/code/diff.rs`

**Step 1:** Implement `render_diff(hunks: &[DiffHunk], width: u16, theme: DiffTheme) -> Vec<Line>`:
- Line numbers on left (Dim)
- `+` lines: green background
- `-` lines: red background
- Context lines: default background
- Syntax highlighting on content via `syntect`
- Hard-wrap long lines at available width

**Step 2:** Implement `DiffTheme` with dark/light variants:
- Dark: `#213A2B` (added), `#4A221D` (removed)
- Light: `#dafbe1` (added), `#ffebe9` (removed)
- Auto-detect: check `$COLORFGBG` or default to dark

**Step 3:** Write tests with known diff inputs, verify line count and style application.

**Step 4:** Commit: `feat(tui): implement theme-aware diff renderer`

### Task 7: History Pane Rendering

**Files:**
- Modify: `crates/clawhive-tui/src/code/history.rs`
- Test: `crates/clawhive-tui/src/code/history.rs` (inline tests)

**Step 1:** Implement `render_history_cell(cell: &HistoryCell, width: u16, verbose: bool) -> Vec<Line>` for each cell variant:
- `UserMessage`: `┃ > {text}` with Cyan border, word-wrapped
- `AssistantText`: markdown-rendered content
- `Thinking`: collapsed single-line or expanded block based on `verbose`
- `ToolCall`: `⏺ {tool} {args}` + output (truncated to 5 lines) + duration
- `Error`: `✗ {message}` in Red

**Step 2:** Implement welcome screen rendering (centered logo + tips).

**Step 3:** Implement the scrollable history container:
- Compute total rendered height of all cells
- Apply `ScrollState` offset to determine visible range
- Render only visible cells into the area
- Show `↑ N more` indicator when scrolled

**Step 4:** Write tests for cell rendering (user message wrapping, tool output truncation).

**Step 5:** Commit: `feat(tui): implement history pane with all cell types`

### Task 8: InputView (Bottom Pane)

**Files:**
- Modify: `crates/clawhive-tui/src/code/bottom_pane/input.rs`

**Step 1:** Wrap `tui-textarea::TextArea` with clawhive-specific configuration:
- Set placeholder text
- Configure keybindings (Shift+Enter → newline, Enter → submit)
- Set max height (8 lines), min height (2 lines)
- Set left border style (agent accent color `┃`)

**Step 2:** Implement shell mode detection (input starts with `!`, change border to Blue).

**Step 3:** Implement input history (↑ on empty input recalls last message).

**Step 4:** Implement shimmer state (when agent is running, replace input with shimmer + "thinking").

**Step 5:** Write tests for height calculation, shell mode toggle.

**Step 6:** Commit: `feat(tui): implement multi-line input with tui-textarea`

### Task 9: ApprovalView (Bottom Pane)

**Files:**
- Modify: `crates/clawhive-tui/src/code/bottom_pane/approval.rs`

**Step 1:** Implement exec approval rendering:
- `△ Permission required` header
- Command display (truncated to width)
- Option list with selection highlight
- Options: Allow once (y), Allow for session (a), Always allow (A), Deny (n), Explain (?)

**Step 2:** Implement diff approval rendering:
- `△ Edit {file_path}` header
- Inline diff view (using diff renderer from Task 6)
- Scrollable diff (↑/↓ scroll within diff area)
- Options: Accept (y), Reject (n), Full diff (d)

**Step 3:** Implement keyboard handling for approval (option navigation, shortcut keys).

**Step 4:** Implement approval queue processing (auto-show next after resolve, counter display).

**Step 5:** Write tests for option selection, queue progression.

**Step 6:** Commit: `feat(tui): implement approval view with diff display`

### Task 10: ShortcutOverlay + SlashCommandPicker + FileSearchPicker

**Files:**
- Modify: `crates/clawhive-tui/src/code/bottom_pane/shortcuts.rs`
- Modify: `crates/clawhive-tui/src/code/bottom_pane/slash_command.rs`
- Modify: `crates/clawhive-tui/src/code/bottom_pane/file_search.rs`

**Step 1:** Implement shortcut overlay — static grid of key→action pairs, dismiss on any key.

**Step 2:** Implement slash command picker:
- Define command list with name + description
- Fuzzy filter as user types after `/`
- Render as floating list above input
- ↑/↓ navigate, Enter select, Esc dismiss

**Step 3:** Implement file search picker:
- Walk workspace directory (respect `.gitignore` via `ignore` crate or manual filtering)
- Fuzzy match on path after `@`
- Render as floating list above input
- Tab/Enter insert selected path, Esc dismiss

**Step 4:** Commit: `feat(tui): implement shortcut overlay, slash commands, and file search`

### Task 11: Main Loop + EventBus Wiring

**Files:**
- Modify: `crates/clawhive-tui/src/code/mod.rs`
- Modify: `crates/clawhive-tui/src/lib.rs`

**Step 1:** Implement the main event loop in `code/mod.rs`:
- Subscribe to bus topics (reuse `subscribe_all` or create code-specific subscription)
- Poll loop: drain bus messages → update state → handle input → render
- 50ms poll interval

**Step 2:** Implement bus message → state mapping:
- `MessageAccepted` → set `is_running = true`
- `StreamDelta` → append to current `AssistantText` cell
- `ReplyReady` → finalize assistant cell, set `is_running = false`
- `TaskFailed` → add Error cell
- `NeedHumanApproval` → push to approval queue, switch bottom pane

**Step 3:** Implement user input submission:
- On Enter: create `InboundMessage`, send to gateway (via mpsc channel + tokio::spawn, same pattern as current code)
- Add `UserMessage` cell to history

**Step 4:** Implement approval resolution:
- On approval decision: call `registry.resolve()` via `block_in_place`
- Pop from queue, show next or return to Input

**Step 5:** Wire the main render function:
- Compute layout constraints
- Call Header, History, BottomPane, Footer renderers

**Step 6:** Update `lib.rs` to expose `run_code_tui()` from the new `code` module.

**Step 7:** Verify: `cargo build -p clawhive-tui && cargo test -p clawhive-tui`

**Step 8:** Commit: `feat(tui): wire main event loop and EventBus integration`

### Task 12: Slash Command Execution

**Files:**
- Modify: `crates/clawhive-tui/src/code/mod.rs`
- Modify: `crates/clawhive-tui/src/code/bottom_pane/slash_command.rs`

**Step 1:** Implement handlers for each slash command:
- `/compact` → send a system message to agent requesting compaction
- `/context` → render context breakdown into history as an info cell
- `/cost` → render token/cost summary into history
- `/diff` → collect changed files (if tracked) and render into history
- `/clear` → clear visible history cells
- `/model` → render model info into history
- `/help` → render command list into history
- `/exit` → set `should_quit = true`

**Step 2:** Commit: `feat(tui): implement slash command handlers`

### Task 13: Integration Testing + Polish

**Files:**
- Modify: various files in `crates/clawhive-tui/src/code/`
- Test: `crates/clawhive-tui/src/code/mod.rs`

**Step 1:** Write integration test: create a `CodeApp`, push mock bus messages, verify state transitions.

**Step 2:** Write integration test: simulate approval flow — push `NeedHumanApproval`, verify bottom pane state, simulate key press, verify resolution.

**Step 3:** Run `cargo clippy -p clawhive-tui -- -D warnings` and fix all warnings.

**Step 4:** Run `cargo test -p clawhive-tui` and verify all tests pass.

**Step 5:** Manual testing: `cargo run -- code` and verify:
- Welcome screen appears on start
- User can type and send messages
- Agent responses stream into history
- Approval overlay appears and works
- Slash commands and file search work
- Ctrl+C double-press quits

**Step 6:** Commit: `feat(tui): code TUI redesign complete`

---

## Out of Scope (Future Enhancements)

These are explicitly **not** part of this implementation:

- **Rewind/Checkpoint system** (`Esc Esc`) — requires session persistence infrastructure
- **Vim mode** for input — add later via `tui-textarea` vim keybinding config
- **Image paste** (`Ctrl+V`) — requires terminal image protocol support
- **External editor** (`Ctrl+G`) — requires temp file + editor spawn
- **Mouse support** — clickable cells, mouse scroll
- **Sidebar** (OpenCode-style) — token stats, file list, agent status
- **Theme switching** — light/dark auto-detection, user-configurable themes
- **`BusMessage::ToolCallStarted/Completed`** — requires orchestrator changes
- **Split diff view** — side-by-side diff for wide terminals
- **Inline file content preview** — show file content when hovering over @paths
