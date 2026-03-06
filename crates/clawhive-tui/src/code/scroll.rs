//! Scroll model placeholders.

/// Manages scroll position with sticky-bottom behavior.
///
/// When the user is at the bottom, new content auto-scrolls.
/// When the user scrolls up manually, sticky mode disengages.
/// It re-engages when the user scrolls back to bottom.
#[allow(dead_code)]
pub(crate) struct ScrollState {
    offset: usize,
    sticky: bool,
}

#[allow(dead_code)]
impl ScrollState {
    pub(crate) fn new() -> Self {
        Self {
            offset: 0,
            sticky: true,
        }
    }

    pub(crate) fn scroll_up(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        self.offset = self.offset.saturating_sub(n);
        self.sticky = false;
    }

    pub(crate) fn scroll_down(&mut self, n: usize, total: usize, viewport: usize) {
        let bottom = bottom_offset(total, viewport);
        self.offset = self.offset.saturating_add(n).min(bottom);
        self.sticky = self.offset == bottom;
    }

    pub(crate) fn ensure_bottom(&mut self, total: usize, viewport: usize) {
        if self.sticky {
            self.offset = bottom_offset(total, viewport);
        }
    }

    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    pub(crate) fn is_sticky(&self) -> bool {
        self.sticky
    }

    pub(crate) fn page_up(&mut self, viewport: usize) {
        self.scroll_up(viewport);
    }

    pub(crate) fn page_down(&mut self, total: usize, viewport: usize) {
        self.scroll_down(viewport, total, viewport);
    }
}

#[allow(dead_code)]
fn bottom_offset(total: usize, viewport: usize) -> usize {
    total.saturating_sub(viewport)
}

#[cfg(test)]
mod tests {
    use super::ScrollState;

    #[test]
    fn starts_sticky_with_zero_offset() {
        let state = ScrollState::new();

        assert_eq!(state.offset(), 0);
        assert!(state.is_sticky());
    }

    #[test]
    fn ensure_bottom_when_sticky_moves_to_bottom_position() {
        let mut state = ScrollState::new();

        state.ensure_bottom(20, 5);

        assert_eq!(state.offset(), 15);
        assert!(state.is_sticky());
    }

    #[test]
    fn scroll_up_disengages_sticky() {
        let mut state = ScrollState::new();
        state.ensure_bottom(30, 10);

        state.scroll_up(3);

        assert_eq!(state.offset(), 17);
        assert!(!state.is_sticky());
    }

    #[test]
    fn scroll_down_to_bottom_reengages_sticky() {
        let mut state = ScrollState::new();
        state.ensure_bottom(30, 10);
        state.scroll_up(4);

        state.scroll_down(4, 30, 10);

        assert_eq!(state.offset(), 20);
        assert!(state.is_sticky());
    }

    #[test]
    fn page_up_and_page_down_use_viewport_height() {
        let mut state = ScrollState::new();
        state.ensure_bottom(50, 10);

        state.page_up(10);
        assert_eq!(state.offset(), 30);
        assert!(!state.is_sticky());

        state.page_down(50, 10);
        assert_eq!(state.offset(), 40);
        assert!(state.is_sticky());
    }

    #[test]
    fn scroll_up_zero_does_not_change_offset_or_sticky() {
        let mut state = ScrollState::new();
        state.ensure_bottom(30, 10);

        state.scroll_up(0);

        assert_eq!(state.offset(), 20);
        assert!(state.is_sticky());
    }

    #[test]
    fn cannot_scroll_below_bottom() {
        let mut state = ScrollState::new();
        state.ensure_bottom(18, 6);
        state.scroll_up(2);

        state.scroll_down(50, 18, 6);

        assert_eq!(state.offset(), 12);
        assert!(state.is_sticky());
    }
}
