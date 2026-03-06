//! Shared style constants and rendering contracts.

use ratatui::style::Color;

pub const AGENT_ACCENT: Color = Color::Cyan;
pub const WARNING: Color = Color::Yellow;
pub const ERROR: Color = Color::Red;
pub const SUCCESS: Color = Color::Green;
pub const INFO: Color = Color::Blue;
pub const MUTED: Color = Color::DarkGray;

pub trait Renderable {
    fn desired_height(&self, width: u16) -> u16;
    fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer);
}
