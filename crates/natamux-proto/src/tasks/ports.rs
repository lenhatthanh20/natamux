//! `portRange` in `packages/core/src/terminals/ports.ts`: each task gets its own block of ports,
//! so the dev servers of tasks that run side by side do not clash.

pub const PORT_BASE: u32 = 20_000;
pub const PORTS_PER_TASK: u32 = 10;
const BLOCKS: u32 = 1000;

/// The first and last port of the task's block: T-0001 gets 20010 to 20019. The block comes from
/// the number at the end of the id and wraps after 1000 tasks.
pub fn port_range(task_id: &str) -> (u32, u32) {
    let digits = task_id
        .rsplit_once('-')
        .map(|(_, digits)| digits)
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .unwrap_or("0");
    // Only the last three digits matter for a number modulo 1000.
    let tail = &digits[digits.len().saturating_sub(3)..];
    let number: u32 = tail.parse().unwrap_or(0);
    let first = PORT_BASE + (number % BLOCKS) * PORTS_PER_TASK;
    (first, first + PORTS_PER_TASK - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_per_task_number_that_wraps_after_1000() {
        assert_eq!(port_range("T-0001"), (20_010, 20_019));
        assert_eq!(port_range("TASK-0001"), (20_010, 20_019));
        assert_eq!(port_range("T-1001"), (20_010, 20_019));
        assert_eq!(port_range("T-0"), (20_000, 20_009));
        assert_eq!(port_range("odd"), (20_000, 20_009));
    }
}
