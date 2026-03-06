use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::CodeApp;

#[allow(dead_code)]
pub(crate) fn render_header(area: Rect, buf: &mut Buffer, app: &CodeApp) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let dim = Style::default().add_modifier(Modifier::DIM);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let left = Line::from(vec![
        Span::raw("  🐝 "),
        Span::styled(app.agent_id.clone(), bold),
        Span::styled(" · ", dim),
        Span::styled(app.model_name.clone(), dim),
    ]);
    buf.set_line(area.x, area.y, &left, area.width);

    if app.token_count == 0 && app.cost_usd == 0.0 {
        return;
    }

    let right_text = format!(
        "{} tokens · ${:.2}",
        format_number_with_commas(app.token_count),
        app.cost_usd
    );
    let right = Line::from(vec![Span::styled(right_text, dim)]);
    let right_width = right.width() as u16;
    if right_width <= area.width {
        let x = area.x + area.width - right_width;
        buf.set_line(x, area.y, &right, right_width);
    }
}

#[allow(dead_code)]
fn format_number_with_commas(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().rev().enumerate() {
        if i != 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_buffer(width: u16, height: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, width, height))
    }

    fn row_content(buf: &Buffer, width: u16) -> String {
        (0..width)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect::<String>()
    }

    #[test]
    fn header_renders_agent_and_model() {
        let app = CodeApp::new("clawhive-main".into(), "claude-4-opus".into());
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = test_buffer(80, 1);

        render_header(area, &mut buf, &app);

        let content = row_content(&buf, 80);
        assert!(content.contains("clawhive-main"));
        assert!(content.contains("claude-4-opus"));
    }

    #[test]
    fn header_renders_token_count_and_cost() {
        let mut app = CodeApp::new("clawhive-main".into(), "claude-4-opus".into());
        app.token_count = 1234;
        app.cost_usd = 0.05;
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = test_buffer(80, 1);

        render_header(area, &mut buf, &app);

        let content = row_content(&buf, 80);
        assert!(content.contains("1,234 tokens · $0.05"));
    }
}
