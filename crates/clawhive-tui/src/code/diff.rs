#![allow(dead_code)]

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::history::{DiffHunk, DiffLine};

const NUMBER_WIDTH: usize = 4;

pub(crate) fn render_diff(hunks: &[DiffHunk], width: u16) -> Vec<Line<'static>> {
    let mut rendered = Vec::new();
    let dim = Style::default().add_modifier(Modifier::DIM);
    rendered.push(Line::from(Span::styled("╭─", dim)));

    let max_width = width.max(1) as usize;
    for hunk in hunks {
        rendered.push(Line::from(vec![
            Span::styled("│ ", dim),
            Span::styled(
                format!("@@ -{} +{} @@", hunk.old_start, hunk.new_start),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        let mut old_line = hunk.old_start;
        for line in &hunk.lines {
            let (line_number, marker, text, style) = match line {
                DiffLine::Context(text) => {
                    let n = old_line;
                    old_line += 1;
                    (Some(n), ' ', text.as_str(), Style::default())
                }
                DiffLine::Removed(text) => {
                    let n = old_line;
                    old_line += 1;
                    (Some(n), '-', text.as_str(), Style::default().fg(Color::Red))
                }
                DiffLine::Added(text) => {
                    (None, '+', text.as_str(), Style::default().fg(Color::Green))
                }
            };

            let number_text = line_number
                .map(|n| format!("{:>width$}", n, width = NUMBER_WIDTH))
                .unwrap_or_else(|| " ".repeat(NUMBER_WIDTH));
            let first_prefix = format!("│ {} │{} ", number_text, marker);
            let continuation_prefix = format!("│ {} │  ", " ".repeat(NUMBER_WIDTH));

            let chunks = wrap_diff_text(
                text,
                max_width,
                first_prefix.chars().count(),
                continuation_prefix.chars().count(),
            );

            for (idx, chunk) in chunks.iter().enumerate() {
                let is_first = idx == 0;
                let prefix = if is_first {
                    &first_prefix
                } else {
                    &continuation_prefix
                };

                let mut spans = vec![
                    Span::styled("│ ", dim),
                    Span::styled(number_text.clone(), dim),
                    Span::styled(" │", dim),
                ];
                if is_first {
                    spans.push(Span::styled(marker.to_string(), style));
                    spans.push(Span::raw(" "));
                } else {
                    spans.push(Span::raw("  "));
                }

                let prefix_len = prefix.chars().count();
                if prefix_len > max_width {
                    rendered.push(Line::from(spans));
                    continue;
                }

                spans.push(Span::styled(chunk.clone(), style));
                rendered.push(Line::from(spans));
            }
        }
    }

    rendered.push(Line::from(Span::styled("╰─", dim)));
    rendered
}

fn wrap_diff_text(
    text: &str,
    width: usize,
    first_prefix_width: usize,
    continuation_prefix_width: usize,
) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_limit = width.saturating_sub(first_prefix_width).max(1);

    for ch in text.chars() {
        if current.chars().count() >= current_limit {
            chunks.push(std::mem::take(&mut current));
            current_limit = width.saturating_sub(continuation_prefix_width).max(1);
        }
        current.push(ch);
    }

    if current.is_empty() {
        chunks.push(String::new());
    } else {
        chunks.push(current);
    }

    chunks
}

pub(crate) fn parse_unified_diff(diff_text: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current: Option<DiffHunk> = None;

    for raw_line in diff_text.lines() {
        if let Some((old_start, new_start)) = parse_hunk_header(raw_line) {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            current = Some(DiffHunk {
                old_start,
                new_start,
                lines: Vec::new(),
            });
            continue;
        }

        let Some(hunk) = current.as_mut() else {
            continue;
        };

        if let Some(line) = raw_line.strip_prefix(' ') {
            hunk.lines.push(DiffLine::Context(line.to_owned()));
        } else if let Some(line) = raw_line.strip_prefix('+') {
            hunk.lines.push(DiffLine::Added(line.to_owned()));
        } else if let Some(line) = raw_line.strip_prefix('-') {
            hunk.lines.push(DiffLine::Removed(line.to_owned()));
        }
    }

    if let Some(hunk) = current {
        hunks.push(hunk);
    }

    hunks
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let body = rest.split(" @@").next()?;
    let mut parts = body.split(" +");
    let old_part = parts.next()?;
    let new_part = parts.next()?;
    let old_start = old_part.split(',').next()?.parse::<u32>().ok()?;
    let new_start = new_part.split(',').next()?.parse::<u32>().ok()?;
    Some((old_start, new_start))
}

#[cfg(test)]
mod tests {
    use ratatui::style::Style;

    use super::*;

    fn line_text(line: &ratatui::text::Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn simple_one_line_change_renders_correctly() {
        let hunks = vec![DiffHunk {
            old_start: 42,
            new_start: 42,
            lines: vec![
                DiffLine::Removed("old".into()),
                DiffLine::Added("new".into()),
            ],
        }];

        let rendered = render_diff(&hunks, 80);
        let texts = rendered.iter().map(line_text).collect::<Vec<_>>();

        assert!(texts.iter().any(|t| t.contains("@@ -42 +42 @@")));
        assert!(texts.iter().any(|t| t.contains("- old")));
        assert!(texts.iter().any(|t| t.contains("+ new")));
    }

    #[test]
    fn added_line_has_green_foreground() {
        let hunks = vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![DiffLine::Added("added".into())],
        }];

        let rendered = render_diff(&hunks, 80);
        let has_green = rendered.iter().flat_map(|l| l.spans.iter()).any(|span| {
            span.content.as_ref().contains("added") && span.style.fg == Some(Color::Green)
        });
        assert!(has_green);
    }

    #[test]
    fn removed_line_has_red_foreground() {
        let hunks = vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![DiffLine::Removed("removed".into())],
        }];

        let rendered = render_diff(&hunks, 80);
        let has_red = rendered.iter().flat_map(|l| l.spans.iter()).any(|span| {
            span.content.as_ref().contains("removed") && span.style.fg == Some(Color::Red)
        });
        assert!(has_red);
    }

    #[test]
    fn context_lines_have_default_style() {
        let hunks = vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![DiffLine::Context("context".into())],
        }];

        let rendered = render_diff(&hunks, 80);
        let has_default = rendered.iter().flat_map(|l| l.spans.iter()).any(|span| {
            span.content.as_ref().contains("context") && span.style == Style::default()
        });
        assert!(has_default);
    }

    #[test]
    fn line_numbers_are_present_and_dim() {
        let hunks = vec![DiffHunk {
            old_start: 9,
            new_start: 9,
            lines: vec![DiffLine::Context("line".into())],
        }];

        let rendered = render_diff(&hunks, 80);
        let has_dim_number = rendered.iter().flat_map(|l| l.spans.iter()).any(|span| {
            span.content.as_ref().contains("  9") && span.style.add_modifier.contains(Modifier::DIM)
        });
        assert!(has_dim_number);
    }

    #[test]
    fn parse_unified_diff_parses_standard_diff() {
        let diff = "@@ -10,2 +10,2 @@\n line a\n-old\n+new\n";
        let hunks = parse_unified_diff(diff);

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start, 10);
        assert_eq!(hunks[0].new_start, 10);
        assert!(matches!(hunks[0].lines[0], DiffLine::Context(_)));
        assert!(matches!(hunks[0].lines[1], DiffLine::Removed(_)));
        assert!(matches!(hunks[0].lines[2], DiffLine::Added(_)));
    }

    #[test]
    fn multi_hunk_diff_renders_hunk_headers() {
        let hunks = vec![
            DiffHunk {
                old_start: 1,
                new_start: 1,
                lines: vec![DiffLine::Context("a".into())],
            },
            DiffHunk {
                old_start: 10,
                new_start: 11,
                lines: vec![DiffLine::Context("b".into())],
            },
        ];

        let rendered = render_diff(&hunks, 80);
        let texts = rendered.iter().map(line_text).collect::<Vec<_>>();
        assert!(texts.iter().any(|t| t.contains("@@ -1 +1 @@")));
        assert!(texts.iter().any(|t| t.contains("@@ -10 +11 @@")));
    }
}
