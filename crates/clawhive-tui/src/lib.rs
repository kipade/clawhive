use std::io;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clawhive_bus::{EventBus, Topic};
use clawhive_core::approval::ApprovalRegistry;
use clawhive_gateway::Gateway;
use clawhive_schema::{ApprovalDecision, BusMessage, InboundMessage};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use tokio::sync::mpsc;
use uuid::Uuid;

pub mod code;
pub mod dashboard;
pub mod shared;

pub use dashboard::BusReceivers;

const MAX_ITEMS: usize = 200;

pub async fn run_tui(
    bus: &EventBus,
    approval_registry: Option<Arc<ApprovalRegistry>>,
) -> Result<()> {
    dashboard::run_tui(bus, approval_registry).await
}

pub async fn subscribe_all(bus: &EventBus) -> BusReceivers {
    dashboard::subscribe_all(bus).await
}

pub async fn run_tui_from_receivers(
    receivers: BusReceivers,
    approval_registry: Option<Arc<ApprovalRegistry>>,
) -> Result<()> {
    dashboard::run_tui_from_receivers(receivers, approval_registry).await
}

#[derive(Clone, Copy, PartialEq)]
enum CodePane {
    Conversation,
    Input,
    Logs,
}

impl CodePane {
    fn next(self) -> Self {
        match self {
            Self::Conversation => Self::Input,
            Self::Input => Self::Logs,
            Self::Logs => Self::Conversation,
        }
    }
}

struct CodeApp {
    conversation: Vec<String>,
    input: String,
    logs: Vec<String>,
    conv_scroll: usize,
    log_scroll: usize,
    focus: CodePane,
    should_quit: bool,
    pending_approvals: Vec<(Uuid, String, String)>,
    approval_overlay: bool,
    approval_selected: usize,
}

impl CodeApp {
    fn new() -> Self {
        Self {
            conversation: vec!["Ready. Type in Input pane and press Enter.".into()],
            input: String::new(),
            logs: vec!["Waiting for events...".into()],
            conv_scroll: 0,
            log_scroll: 0,
            focus: CodePane::Input,
            should_quit: false,
            pending_approvals: vec![],
            approval_overlay: false,
            approval_selected: 0,
        }
    }

    fn push_conversation(&mut self, line: String) {
        self.conversation.push(line);
        if self.conversation.len() > MAX_ITEMS {
            self.conversation.remove(0);
        }
    }

    fn push_log(&mut self, line: String) {
        if self.logs.first().map(|s| s.as_str()) == Some("Waiting for events...") {
            self.logs.clear();
        }
        self.logs.push(line);
        if self.logs.len() > MAX_ITEMS {
            self.logs.remove(0);
        }
    }

    async fn handle_approval_key(&mut self, key: KeyCode, registry: &Arc<ApprovalRegistry>) {
        if self.pending_approvals.is_empty() {
            self.approval_overlay = false;
            self.approval_selected = 0;
            return;
        }

        match key {
            KeyCode::Up => {
                self.approval_selected = self.approval_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                self.approval_selected = (self.approval_selected + 1)
                    .min(self.pending_approvals.len().saturating_sub(1));
            }
            KeyCode::Esc => {
                self.approval_overlay = false;
            }
            KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Char('d') => {
                let idx = self
                    .approval_selected
                    .min(self.pending_approvals.len().saturating_sub(1));
                let (trace_id, command, _) = self.pending_approvals.remove(idx);
                let decision = match key {
                    KeyCode::Char('a') => ApprovalDecision::AllowOnce,
                    KeyCode::Char('A') => ApprovalDecision::AlwaysAllow,
                    _ => ApprovalDecision::Deny,
                };
                let _ = registry.resolve(trace_id, decision).await;
                self.push_log(format!(
                    "[{}] Approval decision for {}: {}",
                    chrono::Local::now().format("%H:%M:%S"),
                    &trace_id.to_string()[..8],
                    command
                ));
                if self.pending_approvals.is_empty() {
                    self.approval_overlay = false;
                    self.approval_selected = 0;
                } else {
                    self.approval_selected =
                        idx.min(self.pending_approvals.len().saturating_sub(1));
                }
            }
            _ => {}
        }
    }
}

pub async fn run_code_tui(
    bus: &EventBus,
    gateway: Arc<Gateway>,
    approval_registry: Option<Arc<ApprovalRegistry>>,
) -> Result<()> {
    let mut rx_reply = bus.subscribe(Topic::ReplyReady).await;
    let mut rx_accept = bus.subscribe(Topic::MessageAccepted).await;
    let mut rx_fail = bus.subscribe(Topic::TaskFailed).await;
    let mut rx_stream = bus.subscribe(Topic::StreamDelta).await;

    let connector_id = format!(
        "code-{}-{}-{}",
        std::env::var("HOSTNAME").unwrap_or_else(|_| "local".to_string()),
        std::process::id(),
        &uuid::Uuid::new_v4().to_string()[..4]
    );
    let conversation_scope = format!("code:{connector_id}:main");
    let user_scope = format!(
        "user:code:{}",
        std::env::var("USER").unwrap_or_else(|_| "developer".to_string())
    );

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let gateway_bg = gateway.clone();
    let connector_bg = connector_id.clone();
    let scope_bg = conversation_scope.clone();
    let user_bg = user_scope.clone();
    tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            let inbound = InboundMessage {
                trace_id: uuid::Uuid::new_v4(),
                channel_type: "code".into(),
                connector_id: connector_bg.clone(),
                conversation_scope: scope_bg.clone(),
                user_scope: user_bg.clone(),
                text,
                at: chrono::Utc::now(),
                thread_id: None,
                is_mention: false,
                mention_target: None,
                message_id: None,
                attachments: vec![],
                group_context: None,
            };
            if let Err(err) = gateway_bg.handle_inbound(inbound).await {
                tracing::error!("code inbound failed: {err}");
            }
        }
    });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = CodeApp::new();
    app.push_log(format!("connector: {connector_id}"));
    app.push_log(format!("scope: {conversation_scope}"));

    let run_result = (|| -> Result<()> {
        loop {
            let ts = chrono::Local::now().format("%H:%M:%S");

            while let Ok(msg) = rx_reply.try_recv() {
                if let BusMessage::ReplyReady { outbound } = msg {
                    if outbound.channel_type == "code" && outbound.connector_id == connector_id {
                        app.push_conversation(format!("[{ts}] Agent: {}", outbound.text));
                    }
                }
            }

            while let Ok(msg) = rx_accept.try_recv() {
                if let BusMessage::MessageAccepted { trace_id } = msg {
                    app.push_log(format!(
                        "[{ts}] accepted trace={}",
                        &trace_id.to_string()[..8]
                    ));
                }
            }

            while let Ok(msg) = rx_fail.try_recv() {
                if let BusMessage::TaskFailed { trace_id, error } = msg {
                    app.push_log(format!(
                        "[{ts}] failed trace={} error={}",
                        &trace_id.to_string()[..8],
                        error
                    ));
                }
            }

            while let Ok(msg) = rx_stream.try_recv() {
                if let BusMessage::StreamDelta {
                    trace_id,
                    delta,
                    is_final,
                } = msg
                {
                    if is_final {
                        app.push_log(format!("[{ts}] stream done {}", &trace_id.to_string()[..8]));
                    } else if !delta.is_empty() {
                        app.push_log(format!(
                            "[{ts}] stream {}: {}",
                            &trace_id.to_string()[..8],
                            delta.chars().take(60).collect::<String>()
                        ));
                    }
                }
            }

            if let Some(ref reg) = approval_registry {
                let pending = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(reg.pending_list())
                });
                if pending != app.pending_approvals {
                    app.pending_approvals = pending;
                    if app.pending_approvals.is_empty() {
                        app.approval_overlay = false;
                        app.approval_selected = 0;
                    } else {
                        app.approval_selected = app
                            .approval_selected
                            .min(app.pending_approvals.len().saturating_sub(1));
                    }
                }
                if !app.pending_approvals.is_empty() && !app.approval_overlay {
                    app.approval_overlay = true;
                    app.approval_selected =
                        app.approval_selected.min(app.pending_approvals.len() - 1);
                }
            }

            terminal.draw(|f| code_ui(f, &app))?;

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        if app.approval_overlay {
                            if let Some(ref reg) = approval_registry {
                                tokio::task::block_in_place(|| {
                                    tokio::runtime::Handle::current()
                                        .block_on(app.handle_approval_key(key.code, reg))
                                });
                            } else if key.code == KeyCode::Esc {
                                app.approval_overlay = false;
                            }
                            continue;
                        }
                        match key.code {
                            KeyCode::Char('q') => app.should_quit = true,
                            KeyCode::Tab => app.focus = app.focus.next(),
                            KeyCode::Up => match app.focus {
                                CodePane::Conversation => {
                                    app.conv_scroll = app.conv_scroll.saturating_sub(1)
                                }
                                CodePane::Logs => app.log_scroll = app.log_scroll.saturating_sub(1),
                                CodePane::Input => {}
                            },
                            KeyCode::Down => match app.focus {
                                CodePane::Conversation => {
                                    app.conv_scroll = (app.conv_scroll + 1)
                                        .min(app.conversation.len().saturating_sub(1))
                                }
                                CodePane::Logs => {
                                    app.log_scroll =
                                        (app.log_scroll + 1).min(app.logs.len().saturating_sub(1))
                                }
                                CodePane::Input => {}
                            },
                            KeyCode::Backspace => {
                                if app.focus == CodePane::Input {
                                    app.input.pop();
                                }
                            }
                            KeyCode::Esc => {
                                if app.focus == CodePane::Input {
                                    app.input.clear();
                                }
                            }
                            KeyCode::Enter => {
                                if app.focus == CodePane::Input {
                                    let text = app.input.trim().to_string();
                                    if !text.is_empty() {
                                        let _ = tx.send(text.clone());
                                        app.push_conversation(format!("[{ts}] You: {text}"));
                                        app.push_log(format!("[{ts}] sent"));
                                    }
                                    app.input.clear();
                                }
                            }
                            KeyCode::Char(c) => {
                                if app.focus == CodePane::Input {
                                    app.input.push(c);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            if app.should_quit {
                break;
            }
        }
        Ok(())
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    run_result
}

fn code_ui(frame: &mut Frame, app: &CodeApp) {
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Length(4),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(frame.area());

    dashboard::render_list_panel(
        frame,
        main[0],
        " Conversation ",
        &app.conversation,
        app.conv_scroll,
        app.focus == CodePane::Conversation,
        Color::Cyan,
    );

    let input_border = if app.focus == CodePane::Input {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let input = Paragraph::new(app.input.as_str()).block(
        Block::default()
            .title(" Input ")
            .borders(Borders::ALL)
            .border_style(input_border),
    );
    frame.render_widget(input, main[1]);

    dashboard::render_list_panel(
        frame,
        main[2],
        " Task Logs ",
        &app.logs,
        app.log_scroll,
        app.focus == CodePane::Logs,
        Color::Magenta,
    );

    let status = Paragraph::new(Line::from(vec![
        Span::styled(
            "[q]",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" quit ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[Tab]",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" switch pane ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" send ", Style::default().fg(Color::DarkGray)),
    ]));
    frame.render_widget(status, main[3]);

    if app.approval_overlay && !app.pending_approvals.is_empty() {
        dashboard::render_approval_overlay(
            frame,
            &app.pending_approvals,
            app.approval_selected,
            " ⚠ Approval Required ",
        );
    }
}
