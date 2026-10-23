//! Output of a shell to its clients (`session-output.ts`): batches, frames, the ring, and the
//! clients that fall behind.

use std::sync::Arc;

use natamux_screen::frames::{char_border, split_frames};

use super::limits::{
    CLIENT_HIGH_WATER, HIDDEN_BUDGET_BYTES, HIDDEN_BUDGET_WINDOW_MS, OUTPUT_COALESCE_MS,
    OUTPUT_FRAME_BYTES, OUTPUT_IDLE_BYTES, OUTPUT_RING_BYTES, RESYNC_QUIET_WINDOW_MS,
};
use super::session_snapshot::catch_up_behind;
use super::types::{ClientState, Frame, Phase, Session};

/// The frames sent since `offset`, while the ring still has them. None when it does not (too
/// old, in the future, or inside a frame).
pub fn ring_since(session: &Session, offset: u64) -> Option<Vec<Arc<[u8]>>> {
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
    session.batch.extend_from_slice(bytes);
    if session.batch.len() >= OUTPUT_FRAME_BYTES {
        send_full_frames(session);
    }
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

/// Sends the full frames of the batch and keeps the rest. A planned flush stays planned for it.
fn send_full_frames(session: &mut Session) {
    let all = std::mem::take(&mut session.batch);
    let mut start = 0;
    while all.len() - start >= OUTPUT_FRAME_BYTES {
        let end = char_border(&all, start, start + OUTPUT_FRAME_BYTES);
        send_frame(session, &all[start..end]);
        start = end;
    }
    session.batch = all[start..].to_vec();
    session.last_flush_at = Some(session.now());
}

/// Also called before every control message and pty resize, so output comes before them.
pub fn flush_output(session: &mut Session) {
    session.scheduler.clear(&mut session.flush_timer);
    if session.batch.is_empty() {
        return;
    }
    let bytes = std::mem::take(&mut session.batch);
    session.last_flush_at = Some(session.now());
    for frame in split_frames(&bytes, OUTPUT_FRAME_BYTES) {
        send_frame(session, frame);
    }
}

fn send_frame(session: &mut Session, bytes: &[u8]) {
    let start = session.offset;
    let frame: Arc<[u8]> = Arc::from(bytes);
    session.offset += frame.len() as u64;
    session.ring_bytes += frame.len();
    session.ring.push_back(Arc::clone(&frame));
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
            client.send(Frame::Bytes(Arc::clone(&frame)));
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
/// while the frame fits the budget of the current window.
fn admits(state: &mut ClientState, bytes: u64, now: u64) -> bool {
    if state.sent + bytes - state.acked > CLIENT_HIGH_WATER {
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
