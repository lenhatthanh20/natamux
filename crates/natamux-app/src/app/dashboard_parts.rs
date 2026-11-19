//! A generation number per part of the dashboard, so each view that follows it draws again only
//! when a part it reads changed. A dashboard notify alone says nothing about what changed.

/// The parts of the dashboard a view can read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Tasks,
    Repos,
    Jobs,
    Activity,
    Usage,
    Limits,
    DiffStats,
    Terminals,
    Preparing,
    /// The link to the daemon and whether it refused the token.
    Link,
    /// The event logs of the attention cards.
    Events,
    /// The dashboard's own state: filter, sort, open task, view state, settings. A reader of
    /// View also follows Selection and Widths (`Changed::any`).
    /// Toasts are no part: only Root reads them, by their ids.
    View,
    /// The selected task.
    Selection,
    /// The widths of the sidebar and the details panel.
    Widths,
}

const PARTS: usize = 14;

/// How often each part changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Generations([u64; PARTS]);

impl Generations {
    pub fn bump(&mut self, part: Part) {
        self.0[part as usize] += 1;
    }

    /// The data was replaced whole (a new daemon): every part changed.
    pub fn bump_all(&mut self) {
        for count in &mut self.0 {
            *count += 1;
        }
    }

    pub fn get(&self, part: Part) -> u64 {
        self.0[part as usize]
    }
}

/// The generations an observer last used. A new one has seen nothing, so its first check is a
/// change.
#[derive(Debug, Default)]
pub struct Seen(Option<Generations>);

impl Seen {
    /// Whether one of `parts` changed since the last check, and records `gens` as seen.
    pub fn changed(&mut self, gens: &Generations, parts: &[Part]) -> bool {
        self.take(gens).any(parts)
    }

    /// What changed since the last check, for an observer that draws more than one thing;
    /// records `gens` as seen.
    pub fn take(&mut self, gens: &Generations) -> Changed {
        Changed {
            before: self.0.replace(*gens),
            now: *gens,
        }
    }
}

/// The generations before and after one dashboard notify.
pub struct Changed {
    before: Option<Generations>,
    now: Generations,
}

impl Changed {
    /// View also counts Selection and Widths, so a view that read them through View before
    /// they were parts of their own still draws.
    pub fn any(&self, parts: &[Part]) -> bool {
        self.any_exact(parts)
            || (parts.contains(&Part::View) && self.any_exact(&[Part::Selection, Part::Widths]))
    }

    /// `any` without that rule, for a reader of View that reads neither the selection nor the
    /// widths: a key move or a panel drag then costs it nothing.
    pub fn any_exact(&self, parts: &[Part]) -> bool {
        let Some(before) = self.before else {
            return true;
        };
        parts.iter().any(|&p| before.get(p) != self.now.get(p))
    }

    /// Whether a part outside `quiet` changed, for a reader of every other part.
    pub fn any_but(&self, quiet: &[Part]) -> bool {
        let Some(before) = self.before else {
            return true;
        };
        let counts = before.0.iter().zip(&self.now.0).enumerate();
        counts
            .filter(|(_, (was, now))| was != now)
            .any(|(index, _)| !quiet.iter().any(|&part| part as usize == index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bump_changes_only_its_part() {
        let mut gens = Generations::default();
        gens.bump(Part::Activity);
        gens.bump(Part::Activity);
        assert_eq!(gens.get(Part::Activity), 2);
        assert_eq!(gens.get(Part::Tasks), 0);
        assert_eq!(gens.get(Part::View), 0);
        gens.bump_all();
        assert_eq!(gens.get(Part::Activity), 3);
        assert_eq!(gens.get(Part::View), 1);
    }

    #[test]
    fn an_observer_sees_a_change_only_of_its_parts() {
        let mut gens = Generations::default();
        let mut seen = Seen::default();
        // Nothing seen yet: the first check draws.
        assert!(seen.changed(&gens, &[Part::Tasks]));
        assert!(!seen.changed(&gens, &[Part::Tasks]));
        gens.bump(Part::Activity);
        assert!(!seen.changed(&gens, &[Part::Tasks, Part::View]));
        gens.bump(Part::View);
        assert!(seen.changed(&gens, &[Part::Tasks, Part::View]));
        assert!(!seen.changed(&gens, &[Part::Tasks, Part::View]));
    }

    #[test]
    fn one_check_tells_each_group_of_parts() {
        let mut gens = Generations::default();
        let mut seen = Seen::default();
        assert!(seen.take(&gens).any(&[]));
        gens.bump(Part::Usage);
        let changed = seen.take(&gens);
        assert!(changed.any(&[Part::Tasks, Part::Usage]));
        assert!(!changed.any(&[Part::Tasks, Part::View]));
        assert!(!seen.take(&gens).any(&[Part::Usage]));
    }

    #[test]
    fn a_reader_of_view_follows_the_selection_and_the_widths_unless_exact() {
        let mut gens = Generations::default();
        let mut seen = Seen::default();
        seen.take(&gens);
        for part in [Part::Selection, Part::Widths] {
            gens.bump(part);
            let changed = seen.take(&gens);
            assert!(changed.any(&[Part::Tasks, Part::View]));
            assert!(changed.any(&[part]));
            assert!(!changed.any(&[Part::Tasks]));
            assert!(!changed.any_exact(&[Part::Tasks, Part::View]));
        }
        gens.bump(Part::View);
        assert!(seen.take(&gens).any_exact(&[Part::View]));
    }

    #[test]
    fn a_reader_of_all_but_some_parts_skips_only_those() {
        let mut gens = Generations::default();
        let mut seen = Seen::default();
        assert!(seen.take(&gens).any_but(&[Part::Usage]));
        assert!(!seen.take(&gens).any_but(&[]));
        gens.bump(Part::Usage);
        assert!(!seen.take(&gens).any_but(&[Part::Usage, Part::Activity]));
        gens.bump(Part::Usage);
        assert!(seen.take(&gens).any_but(&[Part::Activity]));
        gens.bump(Part::Usage);
        gens.bump(Part::Widths);
        assert!(seen.take(&gens).any_but(&[Part::Usage]));
    }

    #[test]
    fn a_part_skipped_once_is_not_news_later() {
        // A sort by changes after DiffStats moved: the sort itself bumps View, so the view
        // draws; the skipped DiffStats bump must not count twice.
        let mut gens = Generations::default();
        let mut seen = Seen::default();
        seen.changed(&gens, &[Part::Tasks]);
        gens.bump(Part::DiffStats);
        assert!(!seen.changed(&gens, &[Part::Tasks]));
        assert!(!seen.changed(&gens, &[Part::Tasks, Part::DiffStats]));
    }
}
