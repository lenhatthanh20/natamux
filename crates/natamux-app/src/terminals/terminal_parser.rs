//! The parser of one remote terminal: it writes the socket's bytes into Zed's screen on the
//! background pool, and acks what it parsed. How often the screen wakes and how soon its parser
//! runs depend on whether the terminal is focused, shown or hidden (T-0210).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use futures::channel::oneshot;
use gpui::{BackgroundExecutor, Priority, Task};
use natamux_client::TerminalHandle;
use terminal::DisplayOnlyWriter;

/// The most output parsed before the parser lets other work of the thread pool run. The paint
/// waits for the terminal's lock while a piece is parsed.
pub const PARSE_PIECE: usize = 4 * 1024;

/// Typing echo shows as soon as it is parsed.
pub const PACE_FOCUSED: Duration = Duration::from_millis(4);
/// About 60 updates a second is smooth for output, and half the repaints of 4 ms at 120 Hz.
pub const PACE_SHOWN: Duration = Duration::from_millis(16);
/// Nothing draws a hidden terminal; the daemon parks it, so this only covers the moment before.
pub const PACE_HIDDEN: Duration = Duration::from_millis(250);

/// How a terminal's screen wakes and how soon its parser runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lane {
    pub pace: Duration,
    pub priority: Priority,
}

/// The focused terminal parses first, so its echo does not wait behind other terminals' output.
pub fn lane(focused: bool, visible: bool) -> Lane {
    if focused {
        Lane {
            pace: PACE_FOCUSED,
            priority: Priority::High,
        }
    } else if visible {
        Lane {
            pace: PACE_SHOWN,
            priority: Priority::Medium,
        }
    } else {
        Lane {
            pace: PACE_HIDDEN,
            priority: Priority::Low,
        }
    }
}

/// Work for the parser of one terminal, in the order the socket gave it.
pub enum Job {
    /// Parse these bytes, then ack `ack` when it is set.
    Write { bytes: Vec<u8>, ack: Option<u64> },
    /// The catch-up's bytes are all parsed once the jobs before this one are done; `parsed`
    /// hears it then.
    CaughtUp {
        offset: u64,
        parsed: oneshot::Sender<()>,
    },
    /// Write to another screen from here on: a snapshot's own screen, or the one it replaced.
    /// Boxed: a writer is large and the other jobs are small.
    Writer(Box<DisplayOnlyWriter>),
    /// Stop and give back the writer and the jobs not done yet, to go on at another priority.
    Handoff,
}

/// What a parser gives back at a handoff: the jobs after it are still queued.
pub struct Parser {
    writer: DisplayOnlyWriter,
    jobs: UnboundedReceiver<Job>,
}

impl Parser {
    pub fn new(writer: DisplayOnlyWriter, jobs: UnboundedReceiver<Job>) -> Parser {
        Parser { writer, jobs }
    }

    /// Runs on the background pool at `priority` until a handoff, or until the jobs end.
    pub fn spawn(
        self,
        handle: Arc<TerminalHandle>,
        priority: Priority,
        executor: &BackgroundExecutor,
    ) -> Task<Option<Parser>> {
        executor.spawn_with_priority(priority, parse(self, handle))
    }
}

/// Where the parser tells what it parsed: the socket, or a test's record.
trait Acks: Send + Sync + 'static {
    fn parsed(&self, offset: u64);
    fn caught_up(&self, offset: u64);
}

impl Acks for TerminalHandle {
    fn parsed(&self, offset: u64) {
        TerminalHandle::parsed(self, offset);
    }

    fn caught_up(&self, offset: u64) {
        TerminalHandle::caught_up(self, offset);
    }
}

/// Pending once, so other work of the thread pool runs between two pieces of a flood.
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// It acks only what it parsed, so a flood slows down to the speed it parses at.
async fn parse<A: Acks>(mut parser: Parser, acks: Arc<A>) -> Option<Parser> {
    while let Some(job) = parser.jobs.next().await {
        match job {
            Job::Write { bytes, ack } => {
                let mut pieces = bytes.chunks(PARSE_PIECE).peekable();
                while let Some(piece) = pieces.next() {
                    parser.writer.write(piece);
                    if pieces.peek().is_some() {
                        YieldNow(false).await;
                    }
                }
                if let Some(offset) = ack {
                    acks.parsed(offset);
                }
            }
            Job::CaughtUp { offset, parsed } => {
                acks.caught_up(offset);
                // The app may have let go of the catch-up; nothing to tell then.
                parsed.send(()).ok();
            }
            Job::Writer(new) => parser.writer = *new,
            Job::Handoff => return Some(parser),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures::channel::mpsc::unbounded;
    use futures::executor::block_on;
    use gpui::{AppContext as _, Entity, TestAppContext};
    use settings::Settings;
    use terminal::terminal_settings::TerminalSettings;
    use terminal::{Terminal, TerminalBuilder};
    use util::paths::PathStyle;

    use super::*;

    /// What the parser acked, in order.
    #[derive(Default)]
    struct Record(Mutex<Vec<String>>);

    impl Record {
        fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    impl Acks for Record {
        fn parsed(&self, offset: u64) {
            self.0.lock().unwrap().push(format!("parsed {offset}"));
        }

        fn caught_up(&self, offset: u64) {
            self.0.lock().unwrap().push(format!("caught up {offset}"));
        }
    }

    fn screen(cx: &mut TestAppContext) -> Entity<Terminal> {
        crate::app::test_views::init(cx);
        cx.new(|cx| {
            let settings = TerminalSettings::get_global(cx).clone();
            TerminalBuilder::new_display_only(
                settings.cursor_shape,
                settings.alternate_scroll,
                None,
                0,
                cx.background_executor(),
                PathStyle::local(),
            )
            .subscribe(cx)
        })
    }

    fn new_parser(
        terminal: &Entity<Terminal>,
        jobs: UnboundedReceiver<Job>,
        cx: &TestAppContext,
    ) -> Parser {
        let writer = terminal.read_with(cx, |terminal, _| terminal.display_only_writer());
        Parser::new(writer.unwrap(), jobs)
    }

    fn write(text: &str, ack: u64) -> Job {
        Job::Write {
            bytes: text.as_bytes().to_vec(),
            ack: Some(ack),
        }
    }

    #[gpui::test]
    fn a_handoff_keeps_jobs_and_acks_in_order(cx: &mut TestAppContext) {
        let terminal = screen(cx);
        let (jobs, queued) = unbounded();
        let (parsed, mut caught_up) = oneshot::channel();
        // Two handoffs in a row, as when the lane changes again during a handover.
        for job in [
            write("one ", 4),
            Job::Handoff,
            write("two ", 8),
            Job::Handoff,
            Job::Handoff,
            write("three", 13),
            Job::CaughtUp { offset: 13, parsed },
        ] {
            jobs.unbounded_send(job).unwrap();
        }
        drop(jobs);
        let record = Arc::new(Record::default());
        let mut parser = new_parser(&terminal, queued, cx);
        let mut handoffs = 0;
        while let Some(back) = block_on(parse(parser, record.clone())) {
            parser = back;
            handoffs += 1;
        }
        assert_eq!(handoffs, 3);
        assert_eq!(
            record.take(),
            ["parsed 4", "parsed 8", "parsed 13", "caught up 13"]
        );
        assert_eq!(caught_up.try_recv().unwrap(), Some(()));
        let text = terminal.read_with(cx, |terminal, _| terminal.get_content());
        assert!(text.contains("one two three"), "{text}");
    }

    #[gpui::test]
    fn a_handoff_with_nothing_queued_gives_the_parser_back(cx: &mut TestAppContext) {
        let terminal = screen(cx);
        let (jobs, queued) = unbounded();
        jobs.unbounded_send(Job::Handoff).unwrap();
        let record = Arc::new(Record::default());
        let parser = new_parser(&terminal, queued, cx);
        let parser = block_on(parse(parser, record.clone())).unwrap();
        assert!(record.take().is_empty());
        // The parser goes on with the jobs that come after.
        jobs.unbounded_send(write("later", 5)).unwrap();
        drop(jobs);
        assert!(block_on(parse(parser, record.clone())).is_none());
        assert_eq!(record.take(), ["parsed 5"]);
    }

    #[test]
    fn parses_in_pieces_of_4_kib() {
        let flood = vec![b'x'; 64 * 1024];
        assert_eq!(flood.chunks(PARSE_PIECE).count(), 16);
        assert!(flood.chunks(PARSE_PIECE).all(|piece| piece.len() <= 4096));
    }

    #[test]
    fn the_lane_follows_focus_and_visibility() {
        assert_eq!(
            lane(true, true),
            Lane {
                pace: Duration::from_millis(4),
                priority: Priority::High
            }
        );
        // Focus wins even while the cache says hidden: a key must echo at once.
        assert_eq!(lane(true, false).priority, Priority::High);
        assert_eq!(
            lane(false, true),
            Lane {
                pace: Duration::from_millis(16),
                priority: Priority::Medium
            }
        );
        assert_eq!(
            lane(false, false),
            Lane {
                pace: Duration::from_millis(250),
                priority: Priority::Low
            }
        );
    }
}
