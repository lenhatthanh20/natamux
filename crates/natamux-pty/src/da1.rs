//! Newer ConPTY asks the terminal for its attributes (DA1, `ESC [ c`) when it starts and holds the
//! shell's output until the answer comes. The daemon answers that first query itself and takes it
//! out of the output: a dashboard that sees it would answer too (xterm.js answers queries in live
//! output), and ConPTY would pass the second answer to the shell as typed text.

/// The answer of a VT220-class terminal.
pub const DA1_ANSWER: &[u8] = b"\x1b[?61;4;6;7;14;21;22;23;24;28;32;42c";

const QUERIES: [&[u8]; 2] = [b"\x1b[c", b"\x1b[0c"];

/// The first query comes before the shell prints; after this much output none is looked for.
const LOOK_BYTES: usize = 4096;

#[derive(Default)]
pub struct Da1Filter {
    held: Vec<u8>,
    seen: usize,
    done: bool,
}

impl Da1Filter {
    /// The output to pass on, and whether to send `DA1_ANSWER` now.
    pub fn feed(&mut self, data: &[u8]) -> (Vec<u8>, bool) {
        if self.done {
            return (data.to_vec(), false);
        }
        let mut bytes = std::mem::take(&mut self.held);
        bytes.extend_from_slice(data);
        if let Some((at, len)) = find_query(&bytes) {
            self.done = true;
            bytes.drain(at..at + len);
            return (bytes, true);
        }
        self.seen += data.len();
        if self.seen >= LOOK_BYTES {
            self.done = true;
            return (bytes, false);
        }
        // A query cut between two reads: hold its start until the next one.
        let keep = partial_tail(&bytes);
        self.held = bytes.split_off(bytes.len() - keep);
        (bytes, false)
    }

    /// What is still held when the shell ended.
    pub fn flush(&mut self) -> Vec<u8> {
        self.done = true;
        std::mem::take(&mut self.held)
    }
}

fn find_query(bytes: &[u8]) -> Option<(usize, usize)> {
    QUERIES
        .iter()
        .filter_map(|query| {
            bytes
                .windows(query.len())
                .position(|window| window == *query)
                .map(|at| (at, query.len()))
        })
        .min()
}

/// How many bytes at the end could be the start of a query.
fn partial_tail(bytes: &[u8]) -> usize {
    (1..=3)
        .rev()
        .find(|len| {
            *len <= bytes.len()
                && QUERIES.iter().any(|query| {
                    *len < query.len() && query.starts_with(&bytes[bytes.len() - len..])
                })
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_the_first_query_and_takes_it_out() {
        let mut filter = Da1Filter::default();
        assert_eq!(
            filter.feed(b"\x1b[?9001h\x1b[?1004h\x1b[c"),
            (b"\x1b[?9001h\x1b[?1004h".to_vec(), true)
        );
        // A later query is a program's: the dashboard answers it.
        assert_eq!(filter.feed(b"x\x1b[c"), (b"x\x1b[c".to_vec(), false));
    }

    #[test]
    fn finds_a_query_cut_between_two_reads() {
        let mut filter = Da1Filter::default();
        assert_eq!(filter.feed(b"ab\x1b["), (b"ab".to_vec(), false));
        assert_eq!(filter.feed(b"0cPS> "), (b"PS> ".to_vec(), true));
    }

    #[test]
    fn passes_output_without_a_query() {
        let mut filter = Da1Filter::default();
        assert_eq!(filter.feed(b"\x1b[?25l"), (b"\x1b[?25l".to_vec(), false));
        assert_eq!(filter.feed(b"\x1b"), (Vec::new(), false));
        assert_eq!(filter.feed(b"[1mhi"), (b"\x1b[1mhi".to_vec(), false));
        let long = vec![b'x'; LOOK_BYTES];
        assert_eq!(filter.feed(&long), (long.clone(), false));
        assert_eq!(filter.feed(b"\x1b[c"), (b"\x1b[c".to_vec(), false));
    }
}
