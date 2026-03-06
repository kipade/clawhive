use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::ApprovalRequest;
use crate::shared::styles::{MUTED, WARNING};

#[allow(dead_code)]
pub(crate) fn render_approval_exec(
    area: Rect,
    buf: &mut Buffer,
    request: &ApprovalRequest,
    queue_position: (usize, usize),
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let mut y = area.y;
    let white_bold = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let triangle = Style::default().fg(WARNING).add_modifier(Modifier::BOLD);
    let selected_prefix = Style::default().add_modifier(Modifier::BOLD);

    write_line(
        area,
        buf,
        y,
        vec![
            Span::styled("△", triangle),
            Span::raw(" "),
            Span::styled("Permission required", white_bold),
        ],
    );
    if queue_position.1 > 1 {
        let queue_text = format!("({}/{})", queue_position.0, queue_position.1);
        write_right(area, buf, y, &queue_text, Style::default().fg(MUTED));
    }

    y += 2;
    let command_width = area.width.saturating_sub(8) as usize;
    let command = truncate_for_width(&request.command, command_width);
    write_line(
        area,
        buf,
        y,
        vec![
            Span::raw("  "),
            Span::styled(format!("$ {command}"), white_bold),
        ],
    );

    y += 2;
    let options = [
        ('y', "Allow once"),
        ('a', "Allow for session"),
        ('A', "Always allow"),
        ('n', "Deny"),
        ('?', "Explain this command"),
    ];
    for (idx, (key, label)) in options.into_iter().enumerate() {
        let selected = request.selected_option == idx;
        let mut spans = Vec::new();
        if selected {
            spans.push(Span::styled("› ", selected_prefix));
        } else {
            spans.push(Span::raw("  "));
        }
        spans.extend(option_spans(key, label, selected));
        write_line(area, buf, y, spans);
        y += 1;
    }
}

#[allow(dead_code)]
pub(crate) fn render_approval_diff(
    area: Rect,
    buf: &mut Buffer,
    request: &ApprovalRequest,
    _queue_position: (usize, usize),
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let mut y = area.y;
    let white_bold = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let triangle = Style::default().fg(WARNING).add_modifier(Modifier::BOLD);

    let file_path = request
        .command
        .strip_prefix("Edit ")
        .unwrap_or(&request.command);
    write_line(
        area,
        buf,
        y,
        vec![
            Span::styled("△", triangle),
            Span::raw(" "),
            Span::styled(format!("Edit {file_path}"), white_bold),
        ],
    );

    y += 2;
    if let Some(hunks) = &request.diff {
        for line in super::super::diff::render_diff(hunks, area.width.saturating_sub(6)) {
            if y >= area.y + area.height {
                break;
            }
            let mut spans = vec![Span::raw("  "), border_span(), Span::raw("   ")];
            spans.extend(line.spans);
            buf.set_line(area.x, y, &Line::from(spans), area.width);
            y += 1;
        }
    }

    y += 1;
    if y >= area.y + area.height {
        return;
    }
    let options = [
        (0, 'y', "Accept"),
        (1, 'n', "Reject"),
        (2, 'd', "Full diff"),
    ];
    let mut spans = Vec::new();
    for (idx, key, label) in options {
        let selected = request.selected_option == idx;
        if idx == 0 {
            if selected {
                spans.push(Span::styled(
                    "› ",
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::raw("  "));
            }
        } else {
            spans.push(Span::raw("   "));
            if selected {
                spans.push(Span::styled(
                    "› ",
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::raw("  "));
            }
        }
        spans.extend(option_spans(key, label, selected));
    }
    write_line(area, buf, y, spans);
}

#[allow(dead_code)]
pub(crate) fn render_approval_view(
    area: Rect,
    buf: &mut Buffer,
    request: &ApprovalRequest,
    queue_position: (usize, usize),
) {
    if request.diff.is_some() {
        render_approval_diff(area, buf, request, queue_position);
    } else {
        render_approval_exec(area, buf, request, queue_position);
    }
}

#[allow(dead_code)]
pub(crate) fn approval_option_count(request: &ApprovalRequest) -> usize {
    if request.diff.is_some() {
        3
    } else {
        5
    }
}

#[allow(dead_code)]
pub(crate) fn desired_approval_height(request: &ApprovalRequest, width: u16) -> u16 {
    if let Some(hunks) = &request.diff {
        let diff_lines =
            super::super::diff::render_diff(hunks, width.saturating_sub(6)).len() as u16;
        (2 + diff_lines + 2).max(8)
    } else {
        10
    }
}

fn option_spans(key: char, label: &str, selected: bool) -> Vec<Span<'static>> {
    let base = if selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let key_style = base.add_modifier(Modifier::BOLD);
    vec![
        Span::styled("[", base),
        Span::styled(key.to_string(), key_style),
        Span::styled("] ", base),
        Span::styled(label.to_string(), base),
    ]
}

fn write_line(area: Rect, buf: &mut Buffer, y: u16, content: Vec<Span<'static>>) {
    if y >= area.y + area.height {
        return;
    }
    let mut spans = vec![Span::raw("  "), border_span(), Span::raw(" ")];
    spans.extend(content);
    buf.set_line(area.x, y, &Line::from(spans), area.width);
}

fn write_right(area: Rect, buf: &mut Buffer, y: u16, text: &str, style: Style) {
    let line = Line::from(Span::styled(text.to_string(), style));
    let width = line.width() as u16;
    if width > area.width {
        return;
    }
    let x = area.x + area.width - width;
    buf.set_line(x, y, &line, width);
}

fn truncate_for_width(input: &str, width: usize) -> String {
    let count = input.chars().count();
    if count <= width {
        return input.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut out = input.chars().take(width - 1).collect::<String>();
    out.push('…');
    out
}

fn border_span() -> Span<'static> {
    Span::styled("┃", Style::default().fg(WARNING))
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use uuid::Uuid;

    use crate::code::history::{DiffHunk, DiffLine};

    use super::{
        approval_option_count, render_approval_diff, render_approval_exec, ApprovalRequest,
    };

    fn buffer_text(buf: &Buffer, area: Rect) -> String {
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(area.x + x, area.y + y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn sample_exec_request(selected_option: usize) -> ApprovalRequest {
        ApprovalRequest {
            trace_id: Uuid::nil(),
            command: "rm -rf target/ && cargo build --release".into(),
            agent_id: "agent-1".into(),
            diff: None,
            selected_option,
        }
    }

    fn sample_diff_request(selected_option: usize) -> ApprovalRequest {
        ApprovalRequest {
            trace_id: Uuid::nil(),
            command: "Edit src/auth/session.rs".into(),
            agent_id: "agent-1".into(),
            diff: Some(vec![DiffHunk {
                old_start: 42,
                new_start: 42,
                lines: vec![
                    DiffLine::Context(" fn is_expired(&self) -> bool {".into()),
                    DiffLine::Removed("    self.expires_at > Utc::now()".into()),
                    DiffLine::Added("    self.expires_at < Utc::now()".into()),
                    DiffLine::Context(" }".into()),
                ],
            }]),
            selected_option,
        }
    }

    #[test]
    fn render_approval_exec_renders_triangle_and_permission_required() {
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        let request = sample_exec_request(0);

        render_approval_exec(area, &mut buf, &request, (1, 1));

        let text = buffer_text(&buf, area);
        assert!(text.contains("△ Permission required"));
    }

    #[test]
    fn render_approval_exec_renders_command_with_dollar_prefix() {
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        let request = sample_exec_request(0);

        render_approval_exec(area, &mut buf, &request, (1, 1));

        let text = buffer_text(&buf, area);
        assert!(text.contains("$ rm -rf target/ && cargo build --release"));
    }

    #[test]
    fn render_approval_exec_shows_selected_option_with_marker() {
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        let request = sample_exec_request(0);

        render_approval_exec(area, &mut buf, &request, (1, 1));

        let text = buffer_text(&buf, area);
        assert!(text.contains("› [y] Allow once"));
    }

    #[test]
    fn render_approval_exec_renders_all_five_options() {
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        let request = sample_exec_request(0);

        render_approval_exec(area, &mut buf, &request, (1, 1));

        let text = buffer_text(&buf, area);
        assert!(text.contains("[y] Allow once"));
        assert!(text.contains("[a] Allow for session"));
        assert!(text.contains("[A] Always allow"));
        assert!(text.contains("[n] Deny"));
        assert!(text.contains("[?] Explain this command"));
    }

    #[test]
    fn render_approval_diff_renders_diff_content() {
        let area = Rect::new(0, 0, 90, 14);
        let mut buf = Buffer::empty(area);
        let request = sample_diff_request(0);

        render_approval_diff(area, &mut buf, &request, (1, 1));

        let text = buffer_text(&buf, area);
        assert!(text.contains("@@ -42 +42 @@"));
        assert!(text.contains("self.expires_at > Utc::now()"));
        assert!(text.contains("self.expires_at < Utc::now()"));
    }

    #[test]
    fn render_approval_diff_shows_three_options() {
        let area = Rect::new(0, 0, 90, 14);
        let mut buf = Buffer::empty(area);
        let request = sample_diff_request(0);

        render_approval_diff(area, &mut buf, &request, (1, 1));

        let text = buffer_text(&buf, area);
        assert!(text.contains("[y] Accept"));
        assert!(text.contains("[n] Reject"));
        assert!(text.contains("[d] Full diff"));
    }

    #[test]
    fn approval_option_count_matches_request_kind() {
        let exec = sample_exec_request(0);
        let diff = sample_diff_request(0);

        assert_eq!(approval_option_count(&exec), 5);
        assert_eq!(approval_option_count(&diff), 3);
    }

    #[test]
    fn queue_position_shows_fraction_format() {
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        let request = sample_exec_request(0);

        render_approval_exec(area, &mut buf, &request, (1, 3));

        let text = buffer_text(&buf, area);
        assert!(text.contains("(1/3)"));
    }
}
