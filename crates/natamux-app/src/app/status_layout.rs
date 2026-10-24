//! Port of `condense`, `forcedByWidth` and friends in `packages/web/src/features/app/status-bar.ts`:
//! how the status bar loses labels and items as it gets narrow. Ids keep the order they were added
//! in, as the sets of the web do.

// Ported with their tests (spec D11). The bar uses only `forced_by_width` until it measures
// overflow, so the measured condensing below (marked) is for tests until then.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusItem {
    pub id: &'static str,
    /// Lower numbers matter more and are the last to lose their label or to fold away.
    pub priority: u32,
    /// False for items that are only an icon or a dot already.
    pub has_label: bool,
    /// False for items that never move into the "..." menu.
    pub folds: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
impl StatusItem {
    pub fn new(id: &'static str, priority: u32, has_label: bool) -> Self {
        Self {
            id,
            priority,
            has_label,
            folds: true,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    Label,
    Fold,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Condensed {
    /// Items shown as an icon with a tooltip.
    pub icon_only: Vec<&'static str>,
    /// Items moved into the "..." menu, or hidden (the bar has no menu now).
    pub folded: Vec<&'static str>,
}

#[cfg_attr(not(test), allow(dead_code))]
fn add(list: &mut Vec<&'static str>, id: &'static str) {
    if !list.contains(&id) {
        list.push(id);
    }
}

/// Every step takes something away. Labels go first, lowest priority first, then the items
/// themselves go into the menu in the same order.
#[cfg_attr(not(test), allow(dead_code))]
pub fn condense_steps(items: &[StatusItem]) -> Vec<(&'static str, StepKind)> {
    let mut low_first: Vec<&StatusItem> = items.iter().collect();
    // Stable, like the web's sort: equal priorities keep their order.
    low_first.sort_by_key(|item| std::cmp::Reverse(item.priority));
    let labels = low_first
        .iter()
        .filter(|i| i.has_label)
        .map(|i| (i.id, StepKind::Label));
    let folds = low_first
        .iter()
        .filter(|i| i.folds)
        .map(|i| (i.id, StepKind::Fold));
    labels.chain(folds).collect()
}

/// The state after `level` steps. Level 0 shows everything in full.
#[cfg_attr(not(test), allow(dead_code))]
pub fn condense(items: &[StatusItem], level: usize) -> Condensed {
    let mut out = Condensed::default();
    for (id, kind) in condense_steps(items).into_iter().take(level) {
        match kind {
            StepKind::Label => add(&mut out.icon_only, id),
            StepKind::Fold => add(&mut out.folded, id),
        }
    }
    out
}

/// How many steps exist, so the bar knows when nothing more can be taken away.
#[cfg_attr(not(test), allow(dead_code))]
pub fn max_condense_level(items: &[StatusItem]) -> usize {
    condense_steps(items).len()
}

/// What the window width alone decides, before anything is measured (spec section 3.2). Under
/// 1024 the jobs item is icon-only. The other items are an icon and a count already, and the
/// link word shows only while the link is down, so nothing else changes by width.
pub fn forced_by_width(width: Option<f32>) -> Condensed {
    let mut out = Condensed::default();
    if width.is_some_and(|width| width < 1024.0) {
        out.icon_only.push("jobs");
    }
    out
}

/// The width rules and the measured condensing, put together.
#[cfg_attr(not(test), allow(dead_code))]
pub fn merge_condensed(a: &Condensed, b: &Condensed) -> Condensed {
    let mut out = a.clone();
    for id in &b.icon_only {
        add(&mut out.icon_only, id);
    }
    for id in &b.folded {
        add(&mut out.folded, id);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<StatusItem> {
        vec![
            StatusItem::new("live", 1, true),
            StatusItem::new("repos", 5, true),
            StatusItem::new("import", 6, true),
            StatusItem::new("account", 2, false),
        ]
    }

    fn sorted(list: &[&'static str]) -> Vec<&'static str> {
        let mut list = list.to_vec();
        list.sort_unstable();
        list
    }

    #[test]
    fn shows_everything_at_level_0() {
        let out = condense(&items(), 0);
        assert!(out.icon_only.is_empty() && out.folded.is_empty());
    }

    #[test]
    fn drops_the_label_of_the_lowest_priority_first() {
        assert_eq!(condense(&items(), 1).icon_only, ["import"]);
        assert_eq!(condense(&items(), 2).icon_only, ["import", "repos"]);
    }

    #[test]
    fn drops_every_label_before_it_folds_an_item() {
        let after_labels = condense(&items(), 3);
        assert_eq!(after_labels.icon_only, ["import", "repos", "live"]);
        assert!(after_labels.folded.is_empty());
    }

    #[test]
    fn then_folds_the_lowest_priority_first() {
        assert_eq!(condense(&items(), 4).folded, ["import"]);
        assert_eq!(condense(&items(), 5).folded, ["import", "repos"]);
    }

    #[test]
    fn has_one_step_per_label_and_one_per_item() {
        assert_eq!(max_condense_level(&items()), 3 + 4);
        assert_eq!(condense_steps(&items()).len(), 7);
    }

    #[test]
    fn keeps_the_most_important_item_to_the_very_end() {
        assert_eq!(
            condense_steps(&items()).last(),
            Some(&("live", StepKind::Fold))
        );
    }

    #[test]
    fn takes_the_label_of_an_item_that_never_folds_but_never_folds_it() {
        let mut with_jobs = items();
        with_jobs.push(StatusItem {
            id: "jobs",
            priority: 4,
            has_label: true,
            folds: false,
        });
        let steps = condense_steps(&with_jobs);
        assert!(steps.contains(&("jobs", StepKind::Label)));
        assert!(!steps.contains(&("jobs", StepKind::Fold)));
        let all = condense(&with_jobs, max_condense_level(&with_jobs));
        assert!(!all.folded.contains(&"jobs"));
    }

    #[test]
    fn forces_nothing_when_the_width_is_unknown_or_large() {
        for width in [None, Some(1280.0), Some(1920.0)] {
            let out = forced_by_width(width);
            assert!(out.icon_only.is_empty() && out.folded.is_empty());
        }
    }

    #[test]
    fn keeps_every_label_from_1024_up() {
        let out = forced_by_width(Some(1024.0));
        assert!(out.icon_only.is_empty() && out.folded.is_empty());
    }

    #[test]
    fn shortens_only_jobs_and_folds_nothing_under_1024() {
        let out = forced_by_width(Some(800.0));
        assert_eq!(out.icon_only, ["jobs"]);
        assert!(out.folded.is_empty());
    }

    #[test]
    fn merges_with_the_measured_result() {
        let merged = merge_condensed(&forced_by_width(Some(800.0)), &condense(&items(), 4));
        assert_eq!(
            sorted(&merged.icon_only),
            ["import", "jobs", "live", "repos"]
        );
        assert_eq!(merged.folded, ["import"]);
    }
}
