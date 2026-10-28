//! Catching a client up (`session-snapshot.ts`): the output it missed from the ring, or a
//! snapshot of the screen. The screen parses output as it comes, so a snapshot is cut and sent in
//! one go; only a synchronized frame of a program makes it wait.

use std::sync::Arc;

use hyper::body::Bytes;
use natamux_proto::terminals::messages::{ACK_EVERY_BYTES, ServerMessage};
use natamux_screen::frames::split_frames;

use super::limits::{
    ACK_STALL_MS, OUTPUT_FRAME_BYTES, RESYNC_MAX_WAIT_MS, RESYNC_QUIET_WINDOW_MS,
    SNAPSHOT_SYNC_WAIT_MS,
};
use super::session_output::{drop_old_output, flush_output, ring_since};
use super::types::{ClientRef, Frame, Phase, Session, SnapshotJob, same_client};

/// Sends a client that comes back the output since `offset`, when the ring still has all of it.
/// False when it does not (too old, in the future, inside a frame, or no offset): the client then
/// needs a snapshot.
pub fn resume(session: &mut Session, client: &ClientRef, offset: Option<u64>) -> bool {
    // The client is in the snapshot phase, so the flush skips it and the ring gets the output.
    flush_output(session);
    let Some(offset) = offset else {
        return false;
    };
    let Some(missed) = ring_since(session, offset) else {
        return false;
    };
    send_missed(client, offset, missed, session.offset);
    let end = session.offset;
    if let Some(state) = session.client_state(client) {
        state.sent = end;
        state.acked = end;
        state.phase = Phase::Live;
    }
    true
}

fn send_missed(client: &ClientRef, from: u64, frames: Vec<Bytes>, to: u64) {
    client.send(Frame::Message(ServerMessage::Resume { offset: from }));
    for frame in frames {
        client.send(Frame::Bytes(frame));
    }
    client.send(Frame::Message(ServerMessage::Replayed { offset: to }));
}

/// A snapshot for the client: at once, or with the one that waits for a program's frame to end.
pub fn request_snapshot(session: &mut Session, client: &ClientRef) {
    if let Some(job) = &mut session.snapshot {
        job.clients.push(Arc::clone(client));
        return;
    }
    if session.screen.in_sync_frame() {
        let timer = session.later(SNAPSHOT_SYNC_WAIT_MS, |session| {
            if let Some(job) = session.snapshot.as_mut() {
                job.timer = None;
            }
            run_waiting_snapshot(session);
        });
        session.snapshot = Some(SnapshotJob {
            clients: vec![Arc::clone(client)],
            timer: Some(timer),
        });
        return;
    }
    run_snapshot(session, &[Arc::clone(client)]);
}

/// Called after each output: a snapshot that waited for a frame goes once the frame ended.
pub fn frame_parsed(session: &mut Session) {
    if session.snapshot.is_some() && !session.screen.in_sync_frame() {
        run_waiting_snapshot(session);
    }
}

fn run_waiting_snapshot(session: &mut Session) {
    if let Some(mut job) = session.snapshot.take() {
        session.scheduler.clear(&mut job.timer);
        run_snapshot(session, &job.clients);
    }
}

/// Cuts the screen at the output so far and sends it to `clients`, then the end of the catch-up.
fn run_snapshot(session: &mut Session, clients: &[ClientRef]) {
    let attached: Vec<ClientRef> = clients
        .iter()
        .filter(|client| {
            session
                .clients
                .iter()
                .any(|(known, _)| same_client(known, client))
        })
        .cloned()
        .collect();
    if attached.is_empty() {
        return;
    }
    // Clients in the snapshot phase drop this output: the snapshot has it.
    flush_output(session);
    let cut = session.offset;
    let size = session.size;
    // One buffer for the whole snapshot: each frame is a range of it, for every client.
    let snapshot = Bytes::from(session.screen.snapshot());
    let frames: Vec<Bytes> = split_frames(&snapshot, OUTPUT_FRAME_BYTES)
        .into_iter()
        .map(|frame| snapshot.slice_ref(frame))
        .collect();
    let now = session.now();
    for client in &attached {
        client.send(Frame::Message(ServerMessage::Snapshot {
            cols: size.cols,
            rows: size.rows,
        }));
        for frame in &frames {
            client.send(Frame::Bytes(frame.clone()));
        }
        client.send(Frame::Message(ServerMessage::Replayed { offset: cut }));
        if let Some(state) = session.client_state(client) {
            // A catch-up does not count as unacked: the client acks its end once it is parsed.
            state.sent = cut;
            state.acked = cut;
            state.acked_at = now;
            state.behind_from = None;
            state.show_now = false;
            state.phase = Phase::Live;
        }
    }
    catch_up_behind(session);
}

/// Catches up the clients that are behind once they can take it: they acked what they got, they
/// are visible, and the shell is quiet or they waited RESYNC_MAX_WAIT_MS. From the ring when it
/// still has what they missed, else with a snapshot. Otherwise it looks again at the earliest
/// time one of them could be ready. A snapshot for a shell that still floods comes at most once
/// per RESYNC_MAX_WAIT_MS, so it cannot loop; a quiet shell gets its snapshot at once. A client
/// shown from parked (`show_now`) does not wait for quiet: that one catch-up goes at once.
pub fn catch_up_behind(session: &mut Session) {
    session.scheduler.clear(&mut session.resync_timer);
    let now = session.now();
    let mut next = u64::MAX;
    let behind: Vec<ClientRef> = session
        .clients
        .iter()
        .filter(|(_, state)| {
            state.phase == Phase::Behind && state.behind_from.is_some() && state.visible
        })
        .map(|(client, _)| Arc::clone(client))
        .collect();
    for client in behind {
        let quiet_at = quiet_at(session, now);
        let Some(state) = session.client_state(&client).cloned() else {
            continue;
        };
        // The dashboard acks every ACK_EVERY_BYTES, so a smaller rest is all it can have parsed.
        if state.sent - state.acked >= ACK_EVERY_BYTES {
            // Its acks stopped: its offset cannot be trusted, so only a snapshot fixes it.
            let stalled = quiet_at
                .max(state.acked_at + ACK_STALL_MS)
                .max(state.snapshot_at + RESYNC_MAX_WAIT_MS);
            if stalled > now {
                next = next.min(stalled);
            } else {
                snapshot_catch_up(session, &client, now);
            }
            continue;
        }
        let ready = if state.show_now {
            now
        } else {
            (state.behind_since + RESYNC_MAX_WAIT_MS).min(quiet_at)
        };
        if ready > now {
            next = next.min(ready);
            continue;
        }
        flush_output(session);
        let from = state.behind_from.unwrap_or(session.offset);
        if let Some(missed) = ring_since(session, from) {
            send_missed(&client, from, missed, session.offset);
            let end = session.offset;
            if let Some(state) = session.client_state(&client) {
                state.sent = end;
                state.acked = end;
                state.acked_at = now;
                state.behind_from = None;
                state.show_now = false;
                state.phase = Phase::Live;
            }
        } else if state.show_now || quiet_at <= now || now >= state.snapshot_at + RESYNC_MAX_WAIT_MS
        {
            snapshot_catch_up(session, &client, now);
        } else {
            next = next
                .min(quiet_at)
                .min(state.snapshot_at + RESYNC_MAX_WAIT_MS);
        }
    }
    if next < u64::MAX {
        session.resync_timer = Some(session.later(next - now, |session| {
            session.resync_timer = None;
            catch_up_behind(session);
        }));
    }
}

fn snapshot_catch_up(session: &mut Session, client: &ClientRef, now: u64) {
    if let Some(state) = session.client_state(client) {
        state.behind_from = None;
        state.show_now = false;
        state.phase = Phase::Snapshot;
        state.snapshot_at = now;
    }
    request_snapshot(session, client);
}

/// When the shell counts as quiet if it prints nothing more: less than cols * rows / 8 bytes in
/// the last RESYNC_QUIET_WINDOW_MS.
fn quiet_at(session: &mut Session, now: u64) -> u64 {
    drop_old_output(session, now);
    let cells = u64::from(session.size.cols) * u64::from(session.size.rows);
    let mut left = session.recent_bytes;
    let mut at = now;
    for &(time, bytes) in &session.recent {
        if left * 8 < cells {
            break;
        }
        left -= bytes;
        at = time + RESYNC_QUIET_WINDOW_MS;
    }
    at
}

/// The shell ended: every client that is behind, visible or parked, and every one that waits for
/// a snapshot gets the output it missed now, before `exit`. An ended terminal keeps its last
/// screen, so that screen must be current. No wait for quiet or for a program's frame: no more
/// output comes. Call it after the last flush.
pub fn catch_up_at_exit(session: &mut Session) {
    let mut snapshot = Vec::new();
    if let Some(mut job) = session.snapshot.take() {
        session.scheduler.clear(&mut job.timer);
        snapshot = job.clients;
    }
    let behind: Vec<(ClientRef, Option<u64>)> = session
        .clients
        .iter()
        .filter(|(_, state)| state.phase == Phase::Behind)
        .map(|(client, state)| (Arc::clone(client), state.behind_from))
        .collect();
    for (client, from) in behind {
        let missed = from.and_then(|from| Some((from, ring_since(session, from)?)));
        let Some((from, missed)) = missed else {
            snapshot.push(client);
            continue;
        };
        send_missed(&client, from, missed, session.offset);
        let end = session.offset;
        if let Some(state) = session.client_state(&client) {
            state.sent = end;
            state.acked = end;
            state.behind_from = None;
            state.phase = Phase::Live;
        }
    }
    // Sent in one go, so the snapshot's frames and its `replayed` come before `exit`.
    run_snapshot(session, &snapshot);
}

pub fn end_snapshots(session: &mut Session) {
    if let Some(mut job) = session.snapshot.take() {
        session.scheduler.clear(&mut job.timer);
    }
}
