//! Part B of `terminal-service.test.ts`: tickets, the socket protocol, batching, the ring, who
//! sets the size, and the snapshot groups.

mod batching;
mod helpers;
mod large_snapshot;
mod one_snapshot;
mod output_before_control;
mod output_ring;
mod shell_size;
mod sliced_snapshot;
mod socket_protocol;
mod synchronized_frame;
mod tickets;
