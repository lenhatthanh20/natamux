//! The contrast of text over painted marks in a code view.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use gpui::font;

/// A grey of lightness `l`.
fn grey(l: f32) -> Hsla {
    Hsla {
        h: 0.,
        s: 0.,
        l,
        a: 1.,
    }
}

fn run(len: usize, color: Hsla) -> TextRun {
    TextRun {
        len,
        font: font(".ZedMono"),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// The plain form: each cut of each run, each mark of each piece. The sweep must give the same.
fn with_contrast_reference(
    runs: Vec<TextRun>,
    marks: &[(Range<usize>, Hsla)],
    background: Hsla,
) -> Vec<TextRun> {
    if marks.is_empty() {
        return runs;
    }
    let mut cuts: Vec<usize> = marks
        .iter()
        .flat_map(|(range, _)| [range.start, range.end])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let under = |at: usize| {
        let mut covered = marks
            .iter()
            .filter(|(range, _)| range.contains(&at))
            .peekable();
        covered.peek()?;
        Some(covered.fold(background, |color, (_, mark)| color.blend(*mark)))
    };
    let mut out = Vec::with_capacity(runs.len() + cuts.len());
    let mut at = 0;
    for run in runs {
        let end = at + run.len;
        let inner = cuts.iter().copied().filter(|cut| at < *cut && *cut < end);
        let mut start = at;
        for cut in inner.chain(std::iter::once(end)) {
            let color = under(start).map_or(run.color, |under| {
                with_minimum_contrast(run.color, under, MINIMUM)
            });
            out.push(TextRun {
                len: cut - start,
                color,
                ..run.clone()
            });
            start = cut;
        }
        at = end;
    }
    out
}

/// A small fixed-seed generator, so each run of the test sees the same inputs.
struct Lcg(u64);

const MUL: u64 = 6_364_136_223_846_793_005;
const ADD: u64 = 1_442_695_040_888_963_407;

impl Lcg {
    fn pick(&mut self, below: usize) -> usize {
        self.0 = self.0.wrapping_mul(MUL).wrapping_add(ADD);
        ((self.0 >> 33) % below as u64) as usize
    }

    /// A colour with a random alpha, now and then fully clear or fully solid.
    fn color(&mut self) -> Hsla {
        let a = match self.pick(5) {
            0 => 0.,
            1 => 1.,
            _ => self.pick(100) as f32 / 100.,
        };
        Hsla {
            h: self.pick(100) as f32 / 100.,
            s: self.pick(100) as f32 / 100.,
            l: self.pick(100) as f32 / 100.,
            a,
        }
    }
}

#[test]
fn text_under_a_mark_gets_the_least_contrast_and_the_rest_keeps_its_colour() {
    let (dark, text) = (grey(0.15), grey(0.4));
    let marks = [(2..5, grey(0.35))];
    let out = with_contrast(vec![run(8, text)], &marks, dark);
    let lens: Vec<usize> = out.iter().map(|run| run.len).collect();
    assert_eq!(lens, vec![2, 3, 3]);
    assert_eq!(out[0].color, text);
    assert_eq!(out[2].color, text);
    assert_ne!(out[1].color, text);
}

#[test]
fn no_marks_leave_the_runs_as_they_are() {
    let runs = vec![run(3, grey(0.5))];
    let out = with_contrast(runs.clone(), &[], grey(0.));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].color, runs[0].color);
}

#[test]
fn the_sweep_gives_the_same_runs_as_the_plain_form() {
    let mut lcg = Lcg(7);
    for case in 0..1_000 {
        let span = 1 + lcg.pick(60);
        // Empty and reversed ranges too: they cut runs but never lie under text.
        let marks: Vec<(Range<usize>, Hsla)> = (0..lcg.pick(12))
            .map(|_| (lcg.pick(span + 4)..lcg.pick(span + 4), lcg.color()))
            .collect();
        // Runs of length 0 too, and runs that end before or after the marks.
        let runs: Vec<TextRun> = (0..lcg.pick(10))
            .map(|_| run(lcg.pick(9), lcg.color()))
            .collect();
        let background = lcg.color();
        assert_eq!(
            with_contrast(runs.clone(), &marks, background),
            with_contrast_reference(runs, &marks, background),
            "case {case}, marks {marks:?}"
        );
    }
}

#[test]
fn ten_thousand_marks_take_little_time() {
    let text = grey(1.);
    let mark = Hsla {
        a: 0.3,
        ..grey(0.2)
    };
    // One mark of 2 every 4 bytes, plus a selection over all of them, over runs of 3.
    let mut marks: Vec<(Range<usize>, Hsla)> = (0..10_000)
        .map(|index| (index * 4..index * 4 + 2, mark))
        .collect();
    marks.push((1..39_999, grey(0.3)));
    let runs: Vec<TextRun> = (0..13_334).map(|_| run(3, text)).collect();
    let start = std::time::Instant::now();
    let out = with_contrast(runs, &marks, grey(0.));
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(out.iter().map(|run| run.len).sum::<usize>(), 40_002);
}
