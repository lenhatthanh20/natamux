use std::time::Duration;

use natamux_proto::terminals::messages::{RECONNECT_BASE, RECONNECT_JITTER, RECONNECT_MAX};

/// The wait before try `tries` (0 first) to bring back a lost socket, as `reconnectDelay` in
/// packages/web/src/features/terminals/terminal.ts. `random` is in [0, 1): 0 waits
/// RECONNECT_JITTER less, and near 1 that much more.
pub fn reconnect_delay(tries: u32, random: f64) -> Duration {
    let max = RECONNECT_MAX.as_millis() as f64;
    let base = (RECONNECT_BASE.as_millis() as f64 * 2f64.powi(tries.min(30) as i32)).min(max);
    let millis = (base * (1.0 + RECONNECT_JITTER * (2.0 * random - 1.0))).round();
    Duration::from_millis(millis.max(0.0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_from_half_a_second_up_to_ten() {
        let delays: Vec<u64> = (0..7)
            .map(|tries| reconnect_delay(tries, 0.5).as_millis() as u64)
            .collect();
        assert_eq!(delays, [500, 1000, 2000, 4000, 8000, 10_000, 10_000]);
        assert_eq!(reconnect_delay(u32::MAX, 0.5), Duration::from_secs(10));
    }

    #[test]
    fn jitter_is_a_fifth_either_way() {
        assert_eq!(reconnect_delay(1, 0.0), Duration::from_millis(800));
        assert_eq!(reconnect_delay(1, 0.999_999), Duration::from_millis(1200));
    }
}
