//! Everything Ruffle says, routed to the Output panel.
//!
//! - [`ChannelLog`] is Ruffle's log backend: `trace()` and uncaught AS2
//!   exceptions.
//! - [`RuffleCapture`] collects Ruffle's WARN-or-worse `tracing` events
//!   (script timeouts, halts, invalid bytecode, stubs), which never reach the
//!   log backend, and notices when the AVM halts. It installs a scoped
//!   dispatcher around each Ruffle call, so it needs no global subscriber and
//!   attributes every message to the player that produced it. While the
//!   scope is active, Ruffle's events go only to the Output panel.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use ruffle_core::backend::log::LogBackend;
use tracing::field::{Field, Visit};
use tracing::level_filters::LevelFilter;
use tracing::subscriber::Interest;
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

use crate::{LogKind, LogLine};

/// The message Ruffle's AVM1 logs (at ERROR) when it halts for good, after a
/// script timeout or invalid bytecode (`core/src/avm1/runtime.rs`).
pub(crate) const AVM1_HALT_MESSAGE: &str = "No more actions will be executed in this movie.";

/// Ruffle's log backend: sends `trace()` output and AVM warnings to a channel.
pub(crate) struct ChannelLog {
    sink: Sender<LogLine>,
}

impl ChannelLog {
    /// A backend writing to `sink`. A closed channel is ignored.
    pub(crate) fn new(sink: Sender<LogLine>) -> Self {
        Self { sink }
    }
}

impl LogBackend for ChannelLog {
    fn avm_trace(&self, message: &str) {
        let _ = self.sink.send(LogLine::new(LogKind::Trace, message));
    }

    fn avm_warning(&self, message: &str) {
        let _ = self.sink.send(LogLine::new(LogKind::AvmWarning, message));
    }
}

/// State shared between a [`RuffleCapture`] and its layer.
#[derive(Debug)]
struct CaptureState {
    sink: Sender<LogLine>,
    halted: AtomicBool,
}

/// Captures Ruffle's `tracing` events for one player.
pub(crate) struct RuffleCapture {
    dispatch: tracing::Dispatch,
    state: Arc<CaptureState>,
}

impl RuffleCapture {
    /// A capture sending lines to `sink`.
    pub(crate) fn new(sink: Sender<LogLine>) -> Self {
        let state = Arc::new(CaptureState {
            sink,
            halted: AtomicBool::new(false),
        });
        let subscriber = tracing_subscriber::registry().with(CaptureLayer {
            state: Arc::clone(&state),
        });
        Self {
            dispatch: tracing::Dispatch::new(subscriber),
            state,
        }
    }

    /// Runs `f` with this capture as the thread's default dispatcher.
    pub(crate) fn scope<R>(&self, f: impl FnOnce() -> R) -> R {
        tracing::dispatcher::with_default(&self.dispatch, f)
    }

    /// Whether Ruffle reported that the AVM halted.
    pub(crate) fn halted(&self) -> bool {
        self.state.halted.load(Ordering::Relaxed)
    }

    /// Forgets a halt (after the player was rebuilt).
    pub(crate) fn reset(&self) {
        self.state.halted.store(false, Ordering::Relaxed);
    }
}

/// The layer behind [`RuffleCapture`].
struct CaptureLayer {
    state: Arc<CaptureState>,
}

/// Whether an event is one the Output panel shows: WARN or worse, from a
/// Ruffle crate. Spans are never enabled, so Ruffle's `#[instrument]` spans
/// cost nothing.
fn is_captured(metadata: &Metadata<'_>) -> bool {
    metadata.is_event()
        && *metadata.level() <= Level::WARN
        && metadata.target().starts_with("ruffle")
}

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        if is_captured(metadata) {
            Interest::always()
        } else {
            Interest::never()
        }
    }

    fn enabled(&self, metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        is_captured(metadata)
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::WARN)
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let message = visitor.finish();
        if message == AVM1_HALT_MESSAGE {
            self.state.halted.store(true, Ordering::Relaxed);
        }
        let level = event.metadata().level();
        let _ = self
            .state
            .sink
            .send(LogLine::new(LogKind::Ruffle, format!("{level}: {message}")));
    }
}

/// Collects an event's `message` field and appends the other fields as
/// `name=value`.
#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: String,
}

impl MessageVisitor {
    fn finish(self) -> String {
        match (self.message.is_empty(), self.fields.is_empty()) {
            (_, true) => self.message,
            (true, false) => self.fields,
            (false, false) => format!("{} {}", self.message, self.fields),
        }
    }

    fn push_field(&mut self, field: &Field, value: &dyn fmt::Display) {
        if !self.fields.is_empty() {
            self.fields.push(' ');
        }
        self.fields.push_str(&format!("{}={}", field.name(), value));
    }
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_owned();
        } else {
            self.push_field(field, &value);
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.push_field(field, &format_args!("{value:?}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn captures_ruffle_warnings_and_halts() {
        let (tx, rx) = mpsc::channel();
        let capture = RuffleCapture::new(tx);
        capture.scope(|| {
            tracing::warn!(target: "ruffle_core::avm1", "Encountered stub: {}", "x");
            tracing::info!(target: "ruffle_core::avm1", "not captured: info");
            tracing::warn!(target: "wgpu_core", "not captured: other crate");
            tracing::error!(target: "ruffle_core::avm1::runtime", "{}", AVM1_HALT_MESSAGE);
        });
        let lines: Vec<_> = rx.try_iter().collect();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines.iter().all(|l| l.kind == LogKind::Ruffle));
        assert_eq!(lines[0].text, "WARN: Encountered stub: x");
        assert!(capture.halted());
        capture.reset();
        assert!(!capture.halted());
    }

    #[test]
    fn events_outside_the_scope_are_not_captured() {
        let (tx, rx) = mpsc::channel();
        let _capture = RuffleCapture::new(tx);
        tracing::warn!(target: "ruffle_core", "outside");
        assert_eq!(rx.try_iter().count(), 0);
    }

    #[test]
    fn log_backend_tags_lines() {
        let (tx, rx) = mpsc::channel();
        let log = ChannelLog::new(tx);
        log.avm_trace("hello");
        log.avm_warning("Uncaught exception, x");
        let lines: Vec<_> = rx.try_iter().collect();
        assert_eq!(lines[0], LogLine::new(LogKind::Trace, "hello"));
        assert_eq!(lines[1].kind, LogKind::AvmWarning);
    }
}
