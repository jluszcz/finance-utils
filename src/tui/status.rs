//! The status line: one message in the footer's place, an error or not, and
//! the rule for how long it stays.
//!
//! With no modal open, a message lasts until the next key or [`TTL`]. With
//! one open, it lasts until the modal closes, so an error stays in view
//! while the form is being fixed. The application says whether a modal is
//! open, since only it knows.

use std::time::{Duration, Instant};

/// How long a message holds the footer when no modal is open.
pub const TTL: Duration = Duration::from_secs(4);

/// The message on the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// What the line says.
    pub text: String,
    /// Whether it is drawn as an error.
    pub error: bool,
    /// `None` under a modal, which holds the message until it closes.
    expires: Option<Instant>,
}

/// The status line's message, if any, and whether the key being handled set
/// it.
#[derive(Debug, Default)]
pub struct StatusLine {
    message: Option<Message>,
    set_by_key: bool,
}

impl StatusLine {
    /// The message to draw in place of the footer, if there is one.
    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// Say `text`. `modal_open` decides whether the message expires.
    pub fn info(&mut self, text: impl Into<String>, modal_open: bool) {
        self.set(text.into(), false, modal_open);
    }

    /// Say `text` as an error. `modal_open` decides whether it expires.
    pub fn error(&mut self, text: impl Into<String>, modal_open: bool) {
        self.set(text.into(), true, modal_open);
    }

    fn set(&mut self, text: String, error: bool, modal_open: bool) {
        let expires = (!modal_open).then(|| Instant::now() + TTL);
        self.message = Some(Message {
            text,
            error,
            expires,
        });
        self.set_by_key = true;
    }

    /// Drop the message.
    pub fn clear(&mut self) {
        self.message = None;
    }

    /// Call before dispatching a key, with whether a modal is open: with
    /// none, the key takes the message away.
    pub fn begin_key(&mut self, modal_open: bool) {
        if !modal_open {
            self.message = None;
        }
        self.set_by_key = false;
    }

    /// Call after dispatching it, with whether a modal was open before the
    /// key and is open now: closing one clears a message the closing key did
    /// not set.
    pub fn end_key(&mut self, modal_was_open: bool, modal_open: bool) {
        if modal_was_open && !modal_open && !self.set_by_key {
            self.message = None;
        }
    }

    /// Drop a message whose time is up, and say whether one went: the event
    /// loop redraws on the answer.
    pub fn expire(&mut self) -> bool {
        self.expire_at(Instant::now())
    }

    /// [`StatusLine::expire`] at `now`, for a test that cannot wait.
    pub fn expire_at(&mut self, now: Instant) -> bool {
        let expired = self
            .message
            .as_ref()
            .and_then(|m| m.expires)
            .is_some_and(|at| now >= at);
        if expired {
            self.message = None;
        }
        expired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &StatusLine) -> Option<&str> {
        line.message().map(|m| m.text.as_str())
    }

    #[test]
    fn a_message_with_no_modal_open_expires_after_its_time() {
        let mut line = StatusLine::default();
        line.info("saved", false);
        assert!(!line.expire_at(Instant::now()));
        assert!(line.expire_at(Instant::now() + TTL));
        assert_eq!(text(&line), None);
    }

    #[test]
    fn a_message_under_a_modal_never_expires() {
        let mut line = StatusLine::default();
        line.error("bad date", true);
        assert!(!line.expire_at(Instant::now() + TTL * 10));
        assert!(line.message().unwrap().error);
    }

    #[test]
    fn with_no_modal_open_the_next_key_takes_the_message_away() {
        let mut line = StatusLine::default();
        line.info("saved", false);
        line.begin_key(false);
        line.end_key(false, false);
        assert_eq!(text(&line), None);
    }

    #[test]
    fn under_a_modal_a_key_leaves_the_message_in_place() {
        let mut line = StatusLine::default();
        line.error("bad date", true);
        line.begin_key(true);
        line.end_key(true, true);
        assert_eq!(text(&line), Some("bad date"));
    }

    #[test]
    fn closing_a_modal_clears_a_message_the_closing_key_did_not_set() {
        let mut line = StatusLine::default();
        line.error("bad date", true);
        line.begin_key(true);
        line.end_key(true, false);
        assert_eq!(text(&line), None);
    }

    #[test]
    fn closing_a_modal_keeps_the_message_the_closing_key_set() {
        let mut line = StatusLine::default();
        line.error("bad date", true);
        line.begin_key(true);
        line.info("saved", false);
        line.end_key(true, false);
        assert_eq!(text(&line), Some("saved"));
    }
}
