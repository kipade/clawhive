use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use std::f32::consts::TAU;
use std::time::{SystemTime, UNIX_EPOCH};

#[allow(dead_code)]
pub(crate) fn shimmer_spans(text: &str, width: u16) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let total = chars.len().min(width as usize);
    if total == 0 {
        return Vec::new();
    }

    let sweep_period = 2.0_f32;
    let elapsed_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f32())
        .unwrap_or(0.0);
    let t = elapsed_seconds % sweep_period;
    let time_phase = t / sweep_period;
    let band_half_width = 0.25_f32;

    chars
        .into_iter()
        .take(total)
        .enumerate()
        .map(|(idx, ch)| {
            let spatial_phase = idx as f32 / total as f32;
            let mut phase = spatial_phase - time_phase;
            phase -= phase.round();
            let brightness = if phase.abs() <= band_half_width {
                0.5 * (1.0 + (phase * TAU).cos())
            } else {
                0.0
            };

            let level = (100.0 + brightness * 100.0).round() as u8;
            Span::styled(
                ch.to_string(),
                Style::default().fg(Color::Rgb(level, level, level)),
            )
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn shimmer_line(width: u16) -> Line<'static> {
    if width == 0 {
        return Line::from(Vec::<Span<'static>>::new());
    }

    let chunk = "thinking...";
    let mut text = String::with_capacity(width as usize);
    while text.chars().count() < width as usize {
        text.push_str(chunk);
    }
    let visible: String = text.chars().take(width as usize).collect();
    Line::from(shimmer_spans(&visible, width))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shimmer_spans_returns_one_span_per_character() {
        let spans = shimmer_spans("thinking", 8);
        assert_eq!(spans.len(), 8);
    }

    #[test]
    fn shimmer_line_fills_requested_width() {
        let width = 32;
        let line = shimmer_line(width);
        assert_eq!(line.width(), width as usize);
        assert_eq!(line.spans.len(), width as usize);
    }
}
