use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::bottom_pane::BottomPaneState;
use super::CodeApp;
use crate::shared::styles::{ERROR, SUCCESS, WARNING};

#[allow(dead_code)]
pub(crate) fn render_footer(area: Rect, buf: &mut Buffer, app: &CodeApp) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let dim = Style::default().add_modifier(Modifier::DIM);
    let left = Line::from(vec![
        Span::styled("agent: ", dim),
        Span::raw(app.agent_id.clone()),
    ]);
    buf.set_line(area.x, area.y, &left, area.width);

    let center_hint = if app.is_running {
        Some("esc interrupt")
    } else if matches!(app.bottom_pane, BottomPaneState::Approval(_)) {
        Some("↑↓ select   enter confirm   esc deny")
    } else {
        None
    };

    if let Some(text) = center_hint {
        let center = Line::from(vec![Span::styled(text, dim)]);
        let center_width = center.width() as u16;
        if center_width <= area.width {
            let x = area.x + (area.width - center_width) / 2;
            buf.set_line(x, area.y, &center, center_width);
        }
    }

    let dot_color = match app.context_used_pct {
        0..=79 => SUCCESS,
        80..=95 => WARNING,
        _ => ERROR,
    };
    let right = Line::from(vec![
        Span::styled(format!("{}% context ", app.context_used_pct), dim),
        Span::styled("◉", Style::default().fg(dot_color)),
    ]);
    let right_width = right.width() as u16;
    if right_width <= area.width {
        let x = area.x + area.width - right_width;
        buf.set_line(x, area.y, &right, right_width);
    }
}

#[cfg(test)]
mod tests {
    use super::super::bottom_pane::ApprovalRequest;
    use super::*;
    use ratatui::style::Color;
    use uuid::Uuid;

    fn test_buffer(width: u16, height: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, width, height))
    }

    fn row_content(buf: &Buffer, width: u16) -> String {
        (0..width)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect::<String>()
    }

    fn find_symbol_x(buf: &Buffer, width: u16, symbol: &str) -> Option<u16> {
        (0..width).find(|x| buf[(*x, 0)].symbol() == symbol)
    }

    fn dot_color(buf: &Buffer, width: u16) -> Option<Color> {
        find_symbol_x(buf, width, "◉").and_then(|x| buf[(x, 0)].style().fg)
    }

    #[test]
    fn footer_shows_interrupt_when_running() {
        let mut app = CodeApp::new("clawhive-main".into(), "claude-4-opus".into());
        app.is_running = true;
        let area = Rect::new(0, 0, 100, 1);
        let mut buf = test_buffer(100, 1);

        render_footer(area, &mut buf, &app);

        let content = row_content(&buf, 100);
        assert!(content.contains("esc interrupt"));
    }

    #[test]
    fn footer_shows_approval_hints_in_approval_mode() {
        let mut app = CodeApp::new("clawhive-main".into(), "claude-4-opus".into());
        app.bottom_pane = BottomPaneState::Approval(ApprovalRequest {
            trace_id: Uuid::nil(),
            command: "ls".into(),
            agent_id: app.agent_id.clone(),
            diff: None,
            selected_option: 0,
        });
        let area = Rect::new(0, 0, 100, 1);
        let mut buf = test_buffer(100, 1);

        render_footer(area, &mut buf, &app);

        let content = row_content(&buf, 100);
        assert!(content.contains("↑↓ select   enter confirm   esc deny"));
    }

    #[test]
    fn footer_context_dot_color_thresholds() {
        let mut app = CodeApp::new("clawhive-main".into(), "claude-4-opus".into());
        let area = Rect::new(0, 0, 100, 1);

        app.context_used_pct = 79;
        let mut buf = test_buffer(100, 1);
        render_footer(area, &mut buf, &app);
        assert_eq!(dot_color(&buf, 100), Some(Color::Green));

        app.context_used_pct = 80;
        let mut buf = test_buffer(100, 1);
        render_footer(area, &mut buf, &app);
        assert_eq!(dot_color(&buf, 100), Some(Color::Yellow));

        app.context_used_pct = 96;
        let mut buf = test_buffer(100, 1);
        render_footer(area, &mut buf, &app);
        assert_eq!(dot_color(&buf, 100), Some(Color::Red));
    }

    #[test]
    fn footer_always_shows_agent_label() {
        let app = CodeApp::new("clawhive-main".into(), "claude-4-opus".into());
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = test_buffer(80, 1);

        render_footer(area, &mut buf, &app);

        let content = row_content(&buf, 80);
        assert!(content.contains("agent:"));
        assert!(content.contains("clawhive-main"));
    }

    #[test]
    fn footer_context_text_is_rendered() {
        let mut app = CodeApp::new("clawhive-main".into(), "claude-4-opus".into());
        app.context_used_pct = 55;
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = test_buffer(80, 1);

        render_footer(area, &mut buf, &app);

        let content = row_content(&buf, 80);
        assert!(content.contains("55% context"));
    }
}
