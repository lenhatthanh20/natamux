//! Which columns the task table has at a width (`features/tasks/table-columns.ts`, section 4.25
//! of the UI spec).

use super::task_sort::SortKey;

pub type ColumnKey = SortKey;

#[derive(Clone, Debug, PartialEq)]
pub enum TableLayout {
    Cards,
    Table {
        columns: Vec<ColumnKey>,
        /// Task has no width of its own: it takes what is left.
        widths: Vec<(ColumnKey, Option<f32>)>,
        min_width: f32,
    },
}

impl TableLayout {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn width_of(&self, column: ColumnKey) -> Option<f32> {
        match self {
            TableLayout::Table { widths, .. } => widths
                .iter()
                .find(|(key, _)| *key == column)
                .and_then(|(_, width)| *width),
            TableLayout::Cards => None,
        }
    }

    /// Where the Task column starts, from the left edge of a row: a menu opened by the keyboard
    /// goes there, under the title.
    pub fn task_column_x(&self) -> f32 {
        match self {
            TableLayout::Table { widths, .. } => widths
                .iter()
                .take_while(|(key, _)| *key != ColumnKey::Task)
                .filter_map(|(_, width)| *width)
                .sum(),
            TableLayout::Cards => 0.0,
        }
    }
}

/// The width of a column in px. When the main area is narrower than the sum, the table scrolls.
fn width(column: ColumnKey) -> Option<f32> {
    match column {
        ColumnKey::Id => Some(72.0),
        ColumnKey::Task => None,
        ColumnKey::State => Some(128.0),
        ColumnKey::Agent => Some(120.0),
        ColumnKey::Branch => Some(200.0),
        ColumnKey::Changes => Some(88.0),
        ColumnKey::Pr => Some(120.0),
        ColumnKey::Cost => Some(88.0),
        ColumnKey::Activity => Some(104.0),
    }
}

/// The task column never gets less than this.
pub const TASK_MIN: f32 = 240.0;

/// The full set needs this much room: all its columns and the task column at its minimum.
const FULL_FROM: f32 = 1160.0;

pub fn column_label(column: ColumnKey) -> &'static str {
    match column {
        ColumnKey::Id => "Id",
        ColumnKey::Task => "Task",
        ColumnKey::State => "State",
        ColumnKey::Agent => "Agent",
        ColumnKey::Branch => "Branch",
        ColumnKey::Changes => "Changes",
        ColumnKey::Pr => "PR",
        ColumnKey::Cost => "Cost",
        ColumnKey::Activity => "Last activity",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HasValue {
    pub changes: bool,
    pub pr: bool,
    pub cost: bool,
}

/// Changes, PR and Cost are left out when no row has a value. None width: not measured, so the
/// widest set.
pub fn table_layout(main_width: Option<f32>, has: HasValue) -> TableLayout {
    use SortKey::{Activity, Agent, Branch, Changes, Cost, Id, Pr, State, Task};
    let main_width = main_width.unwrap_or(f32::INFINITY);
    let columns: &[ColumnKey] = if main_width < 560.0 {
        return TableLayout::Cards;
    } else if main_width >= FULL_FROM {
        &[Id, Task, State, Agent, Branch, Changes, Pr, Cost, Activity]
    } else if main_width >= 760.0 {
        &[Id, Task, State, Changes, Activity]
    } else {
        &[Id, Task, State, Activity]
    };
    let columns: Vec<ColumnKey> = columns
        .iter()
        .copied()
        .filter(|column| match column {
            Changes => has.changes,
            Pr => has.pr,
            Cost => has.cost,
            _ => true,
        })
        .collect();
    let fixed: f32 = columns
        .iter()
        .map(|column| width(*column).unwrap_or(0.0))
        .sum();
    TableLayout::Table {
        widths: columns
            .iter()
            .map(|column| (*column, width(*column)))
            .collect(),
        columns,
        min_width: fixed + TASK_MIN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY: HasValue = HasValue {
        changes: true,
        pr: true,
        cost: true,
    };

    fn columns(main_width: Option<f32>, has: HasValue) -> Option<Vec<ColumnKey>> {
        match table_layout(main_width, has) {
            TableLayout::Cards => None,
            TableLayout::Table { columns, .. } => Some(columns),
        }
    }

    use SortKey::{Activity, Agent, Branch, Changes, Cost, Id, Pr, State, Task};

    #[test]
    fn has_every_column_from_1160() {
        let all = vec![Id, Task, State, Agent, Branch, Changes, Pr, Cost, Activity];
        assert_eq!(columns(Some(1160.0), EVERY), Some(all));
        assert_eq!(columns(Some(1920.0), EVERY).map(|c| c.len()), Some(9));
    }

    #[test]
    fn drops_agent_branch_pull_request_and_cost_from_760_to_1159() {
        let set = Some(vec![Id, Task, State, Changes, Activity]);
        assert_eq!(columns(Some(1159.0), EVERY), set);
        assert_eq!(columns(Some(760.0), EVERY), set);
    }

    #[test]
    fn keeps_id_task_state_and_last_activity_from_560_to_759() {
        let set = Some(vec![Id, Task, State, Activity]);
        assert_eq!(columns(Some(759.0), EVERY), set);
        assert_eq!(columns(Some(560.0), EVERY), set);
    }

    #[test]
    fn a_keyboard_menu_opens_at_the_task_column() {
        assert_eq!(table_layout(Some(1200.0), EVERY).task_column_x(), 72.0);
        assert_eq!(table_layout(Some(400.0), EVERY).task_column_x(), 0.0);
    }

    #[test]
    fn is_a_card_list_under_560() {
        assert_eq!(columns(Some(559.0), EVERY), None);
        assert_eq!(columns(Some(390.0), EVERY), None);
    }

    #[test]
    fn hides_changes_pull_request_and_cost_when_no_row_has_a_value() {
        let has_no = |column| {
            let has = HasValue {
                changes: column != Changes,
                pr: column != Pr,
                cost: column != Cost,
            };
            !columns(Some(1200.0), has)
                .unwrap_or_default()
                .contains(&column)
        };
        assert!(has_no(Changes) && has_no(Pr) && has_no(Cost));
        let none = HasValue {
            changes: false,
            pr: false,
            cost: false,
        };
        assert_eq!(
            columns(Some(900.0), none),
            Some(vec![Id, Task, State, Activity])
        );
    }

    #[test]
    fn uses_the_widest_set_when_the_width_is_unknown() {
        assert_eq!(columns(None, EVERY).map(|c| c.len()), Some(9));
    }

    #[test]
    fn gives_the_task_column_no_width_of_its_own_and_a_minimum_for_the_whole_table() {
        let layout = table_layout(Some(1200.0), EVERY);
        assert_eq!(layout.width_of(Task), None);
        assert_eq!(layout.width_of(Id), Some(72.0));
        assert_eq!(layout.width_of(State), Some(128.0));
        assert_eq!(layout.width_of(Cost), Some(88.0));
        let TableLayout::Table { min_width, .. } = layout else {
            panic!("expected a table");
        };
        assert_eq!(
            min_width,
            72.0 + 128.0 + 120.0 + 200.0 + 88.0 + 120.0 + 88.0 + 104.0 + 240.0
        );
    }

    #[test]
    fn has_a_smaller_minimum_with_fewer_columns() {
        let min = |w| match table_layout(Some(w), EVERY) {
            TableLayout::Table { min_width, .. } => min_width,
            TableLayout::Cards => f32::NAN,
        };
        assert!(min(700.0) < min(1200.0));
    }
}
