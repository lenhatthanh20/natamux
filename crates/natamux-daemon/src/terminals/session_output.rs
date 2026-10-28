//! Output of a shell to its clients (`session-output.ts`): batches, frames, the ring, and the
//! clients that fall behind. A byte is copied at most once into the batch and once into its
//! frame; a frame of a batch is a range of it.

use hyper::body::Bytes;
use natamux_screen::frames::{char_border, split_frames};

use super::limits::{
    CLIENT_HIGH_WATER, HIDDEN_BUDGET_BYTES, HIDDEN_BUDGET_WINDOW_MS, OUTPUT_COALESCE_MS,
    OUTPUT_FRAME_BYTES, OUTPUT_IDLE_BYTES, OUTPUT_RING_BYTES, RESYNC_QUIET_WINDOW_MS,
};
use super::session_snapshot::catch_up_behind;
use super::types::{ClientState, Frame, Phase, Session};

/// The frames sent since `offset`, while the ring still has them. None when it does not (too
/// old, in the future, or inside a frame).
pub fn ring_since(session: &Session, offset: u64) -> Option<Vec<Bytes>> {
    if offset < session.ring_start || offset > session.offset {
        return None;
    }
    let mut at = session.ring_start;
    for (index, frame) in session.ring.iter().enumerate() {
        if at == offset {
            return Some(session.ring.iter().skip(index).cloned().collect());
        }
        if at > offset {
            return None;
        }
        at += frame.len() as u64;
    }
    (at == offset).then(Vec::new)
}

pub fn drop_old_output(session: &mut Session, now: u64) {
    while let Some(&(at, bytes)) = session.recent.front() {
        if at + RESYNC_QUIET_WINDOW_MS > now {
            break;
        }
        session.recent.pop_front();
        session.recent_bytes -= bytes;
    }
}

/// Output goes out in batches: one frame for many small chunks, see OUTPUT_COALESCE_MS.
pub fn queue_output(session: &mut Session, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let now = session.now();
    session.recent.push_back((now, bytes.len() as u64));
    session.recent_bytes += bytes.len() as u64;
    drop_old_output(session, now);
    let rest = send_full_frames(session, bytes);
    session.batch.extend_from_slice(rest);
    if session.batch.is_empty() || session.flush_timer.is_some() {
        return;
    }
    let idle = session
        .last_flush_at
        .is_none_or(|last| now >= last + OUTPUT_COALESCE_MS);
    let delay = if idle && session.batch.len() < OUTPUT_IDLE_BYTES {
        0
    } else {
        OUTPUT_COALESCE_MS
    };
    session.flush_timer = Some(session.later(delay, |session| {
        session.flush_timer = None;
        flush_output(session);
    }));
}

/// Sends the frames that the batch and `bytes` fill: the batch topped up from `bytes`, then full
/// frames straight from `bytes`. Gives the rest, for the batch. A planned flush stays planned.
fn send_full_frames<'a>(session: &mut Session, bytes: &'a [u8]) -> &'a [u8] {
    if session.batch.len() + bytes.len() < OUTPUT_FRAME_BYTES {
        return bytes;
    }
    let mut rest = bytes;
    if !session.batch.is_empty() {
        let take = fits(rest, OUTPUT_FRAME_BYTES - session.batch.len());
        session.batch.extend_from_slice(&rest[..take]);
        rest = &rest[take..];
        let batch = Bytes::from(std::mem::take(&mut session.batch));
        send_frame(session, batch);
    }
    while rest.len() >= OUTPUT_FRAME_BYTES {
        let end = char_border(rest, 0, OUTPUT_FRAME_BYTES);
        send_frame(session, Bytes::copy_from_slice(&rest[..end]));
        rest = &rest[end..];
    }
    session.last_flush_at = Some(session.now());
    rest
}

/// The most of `bytes` that fits in `room` and ends on a character border; 0 when none does.
fn fits(bytes: &[u8], room: usize) -> usize {
    let mut end = room.min(bytes.len());
    while end > 0 && end < bytes.len() && bytes[end] & 0xc0 == 0x80 {
        end -= 1;
    }
    end
}

/// Also called before every control message and pty resize, so output comes before them.
pub fn flush_output(session: &mut Session) {
    session.scheduler.clear(&mut session.flush_timer);
    if session.batch.is_empty() {
        return;
    }
    let bytes = Bytes::from(std::mem::take(&mut session.batch));
    session.last_flush_at = Some(session.now());
    for frame in split_frames(&bytes, OUTPUT_FRAME_BYTES) {
        send_frame(session, bytes.slice_ref(frame));
    }
}

fn send_frame(session: &mut Session, frame: Bytes) {
    let start = session.offset;
    session.offset += frame.len() as u64;
    session.ring_bytes += frame.len();
    session.ring.push_back(frame.clone());
    while session.ring_bytes > OUTPUT_RING_BYTES && session.ring.len() > 1 {
        if let Some(dropped) = session.ring.pop_front() {
            session.ring_bytes -= dropped.len();
            session.ring_start += dropped.len() as u64;
        }
    }
    let now = session.now();
    let offset = session.offset;
    let mut fell_behind = false;
    for (client, state) in &mut session.clients {
        if state.phase != Phase::Live {
            continue;
        }
        if admits(state, frame.len() as u64, now) {
            client.send(Frame::Bytes(frame.clone()));
            state.sent = offset;
        } else {
            // Dropped, not queued, so a slow or hidden client costs no memory.
            state.phase = Phase::Behind;
            state.behind_from = Some(start);
            state.behind_since = now;
            fell_behind = true;
        }
    }
    // A look soon, not now (this runs inside a flush): one whose acks stop needs a timer.
    if fell_behind && session.resync_timer.is_none() {
        session.resync_timer = Some(session.later(0, |session| {
            session.resync_timer = None;
            catch_up_behind(session);
        }));
    }
}

/// A live client takes a frame while it is within CLIENT_HIGH_WATER of its acks and, when hidden,
/// while the frame fits the budget of the current window. A parked client takes none.
fn admits(state: &mut ClientState, bytes: u64, now: u64) -> bool {
    if state.parked || state.sent + bytes - state.acked > CLIENT_HIGH_WATER {
        return false;
    }
    if state.visible {
        return true;
    }
    if now >= state.budget_from + HIDDEN_BUDGET_WINDOW_MS {
        let windows = (now - state.budget_from) / HIDDEN_BUDGET_WINDOW_MS;
        state.budget_from += windows * HIDDEN_BUDGET_WINDOW_MS;
        state.budget_bytes = 0;
    }
    if state.budget_bytes + bytes > HIDDEN_BUDGET_BYTES {
        return false;
    }
    state.budget_bytes += bytes;
    true
}

#[cfg(test)]
mod tests {
    use super::fits;

    #[test]
    fn a_top_up_never_ends_inside_a_character() {
        let text = "aé€".as_bytes();
        assert_eq!(fits(text, 10), text.len());
        assert_eq!(fits(text, 1), 1);
        // "é" is bytes 1 and 2: room for 2 takes only the "a".
        assert_eq!(fits(text, 2), 1);
        assert_eq!(fits(text, 3), 3);
        // "€" is bytes 3 to 5.
        assert_eq!(fits(text, 5), 3);
        assert_eq!(fits("é".as_bytes(), 1), 0);
        assert_eq!(fits(b"", 4), 0);
    }
}
