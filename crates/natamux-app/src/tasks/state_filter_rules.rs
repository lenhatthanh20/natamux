//! How many state filter chips fit (`features/tasks/state-filter.ts`).

/// The gap between two chips in px.
pub const CHIP_GAP: f32 = 2.0;

/// How many chips fit in `available` px. When not all fit, room is kept for the "+N" button.
pub fn chips_that_fit(widths: &[f32], available: f32, more_width: f32, gap: f32) -> usize {
    let total = widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    if total <= available {
        return widths.len();
    }
    let mut used = more_width;
    let mut count = 0;
    for width in widths {
        used += width + gap;
        if used > available {
            break;
        }
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_every_chip_when_they_all_fit_without_room_for_plus_n() {
        assert_eq!(
            chips_that_fit(&[80.0, 80.0, 80.0], 244.0, 40.0, CHIP_GAP),
            3
        );
    }

    #[test]
    fn folds_the_chips_that_do_not_fit_and_keeps_room_for_plus_n() {
        // 80 + 2 + 80 + 2 + 40 (the button) = 204 fits in 210; a third chip would not.
        assert_eq!(
            chips_that_fit(&[80.0, 80.0, 80.0], 210.0, 40.0, CHIP_GAP),
            2
        );
    }

    #[test]
    fn can_fold_every_chip() {
        assert_eq!(chips_that_fit(&[80.0, 80.0], 50.0, 40.0, CHIP_GAP), 0);
    }

    #[test]
    fn has_nothing_to_fit_without_chips() {
        assert_eq!(chips_that_fit(&[], 100.0, 40.0, CHIP_GAP), 0);
    }
}
