//! The order of the task table (`sortTasks`, `nextSort`, `STATE_ORDER` in `packages/web/src/lib/
//! tasks.ts`).

use std::cmp::Ordering;
use std::collections::BTreeMap;

use natamux_proto::agents::activity::TaskUsage;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::workspaces::workspace::DiffStat;

/// What needs the user comes first, finished work last. The default sort and the order of the
/// filter chips.
pub const STATE_ORDER: [TaskState; 9] = [
    TaskState::NeedsInput,
    TaskState::Review,
    TaskState::Failed,
    TaskState::Blocked,
    TaskState::Running,
    TaskState::Ready,
    TaskState::Draft,
    TaskState::Done,
    TaskState::Cancelled,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SortKey {
    Id,
    Task,
    State,
    Agent,
    Branch,
    Changes,
    Pr,
    Cost,
    Activity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// None is the default order: state group, then id descending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnSort {
    pub key: SortKey,
    pub direction: SortDirection,
}

pub type TaskSort = Option<ColumnSort>;

/// A state's place in `STATE_ORDER`. A state this build does not know sorts first, as `indexOf`
/// gives -1 in the web app.
pub fn state_rank(state: TaskState) -> i32 {
    STATE_ORDER
        .iter()
        .position(|known| *known == state)
        .map_or(-1, |at| at as i32)
}

/// "T-0042" -> 42, so T-10000 sorts after T-9999.
pub fn task_number(id: &str) -> i64 {
    let digits: String = id
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap_or(0)
}

fn by_id(a: &Task, b: &Task) -> Ordering {
    task_number(&b.id)
        .cmp(&task_number(&a.id))
        .then_with(|| b.id.cmp(&a.id))
}

fn by_default(a: &Task, b: &Task) -> Ordering {
    state_rank(a.state)
        .cmp(&state_rank(b.state))
        .then_with(|| by_id(a, b))
}

/// The number at the end of the pull request link, with an optional slash after it.
fn pull_request_order(task: &Task) -> Option<f64> {
    let link = task.pr.as_deref()?;
    let link = link.strip_suffix('/').unwrap_or(link);
    let start = link.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    link[start..]
        .parse::<u64>()
        .ok()
        .map(|number| number as f64)
}

/// What a task is worth under a sort key. None means it has no value, and such tasks go last.
#[derive(Clone, Debug, PartialEq)]
enum SortValue {
    Number(f64),
    Text(String),
}

fn compare_values(left: &SortValue, right: &SortValue) -> Ordering {
    match (left, right) {
        (SortValue::Number(a), SortValue::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (SortValue::Text(a), SortValue::Text(b)) => a.cmp(b),
        _ => Ordering::Equal,
    }
}

fn sort_value(
    task: &Task,
    key: SortKey,
    diff_stats: &BTreeMap<String, DiffStat>,
    usage: &BTreeMap<String, TaskUsage>,
) -> Option<SortValue> {
    let text = |value: &Option<String>| value.clone().map(SortValue::Text);
    match key {
        SortKey::Id => Some(SortValue::Number(task_number(&task.id) as f64)),
        SortKey::Task => Some(SortValue::Text(task.title.to_lowercase())),
        SortKey::State => Some(SortValue::Number(f64::from(state_rank(task.state)))),
        SortKey::Agent => text(&task.agent),
        SortKey::Branch => text(&task.branch),
        SortKey::Changes => diff_stats
            .get(&task.id)
            .filter(|stat| stat.files != 0)
            .map(|stat| SortValue::Number((stat.added + stat.removed) as f64)),
        SortKey::Pr => pull_request_order(task).map(SortValue::Number),
        SortKey::Cost => usage
            .get(&task.id)
            .and_then(|row| row.cost_usd)
            .map(SortValue::Number),
        SortKey::Activity => Some(SortValue::Text(task.updated_at.clone())),
    }
}

/// The same tasks in a stable order. Ties fall back to the default order, so two rows never swap
/// places just because a value is equal.
pub fn sort_tasks(
    tasks: &[Task],
    sort: TaskSort,
    diff_stats: &BTreeMap<String, DiffStat>,
    usage: &BTreeMap<String, TaskUsage>,
) -> Vec<Task> {
    let mut sorted = tasks.to_vec();
    let Some(column) = sort else {
        sorted.sort_by(by_default);
        return sorted;
    };
    sorted.sort_by(|a, b| {
        let left = sort_value(a, column.key, diff_stats, usage);
        let right = sort_value(b, column.key, diff_stats, usage);
        match (left, right) {
            (None, None) => by_default(a, b),
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(left), Some(right)) => {
                let order = compare_values(&left, &right);
                let order = if column.direction == SortDirection::Desc {
                    order.reverse()
                } else {
                    order
                };
                order.then_with(|| by_default(a, b))
            }
        }
    });
    sorted
}

/// A click on a column header: first ascending, then descending, then the default order again.
pub fn next_sort(current: TaskSort, key: SortKey) -> TaskSort {
    match current {
        Some(column) if column.key == key => match column.direction {
            SortDirection::Asc => Some(ColumnSort {
                key,
                direction: SortDirection::Desc,
            }),
            SortDirection::Desc => None,
        },
        _ => Some(ColumnSort {
            key,
            direction: SortDirection::Asc,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, title: &str, state: TaskState) -> Task {
        Task {
            id: id.into(),
            title: title.into(),
            state,
            ..Task::default()
        }
    }

    fn tasks() -> Vec<Task> {
        let mut list = vec![
            task("T-0001", "beta", TaskState::Done),
            task("T-0002", "Alpha", TaskState::Running),
            task("T-0010", "gamma", TaskState::Running),
            task("T-0003", "delta", TaskState::NeedsInput),
            task("T-0004", "eps", TaskState::Review),
            task("T-0005", "zeta", TaskState::Review),
        ];
        list[0].agent = Some("claude-code".into());
        list[1].branch = Some("natamux/b".into());
        list[2].branch = Some("natamux/a".into());
        list[4].pr = Some("https://github.com/o/r/pull/12".into());
        list[5].pr = Some("https://github.com/o/r/pull/9".into());
        list
    }

    fn sorted_ids(
        sort: Option<(SortKey, SortDirection)>,
        stats: &BTreeMap<String, DiffStat>,
        usage: &BTreeMap<String, TaskUsage>,
    ) -> Vec<String> {
        let sort = sort.map(|(key, direction)| ColumnSort { key, direction });
        sort_tasks(&tasks(), sort, stats, usage)
            .into_iter()
            .map(|task| task.id)
            .collect()
    }

    fn ids(sort: Option<(SortKey, SortDirection)>) -> Vec<String> {
        sorted_ids(sort, &BTreeMap::new(), &BTreeMap::new())
    }

    fn strs(ids: &[String]) -> Vec<&str> {
        ids.iter().map(String::as_str).collect()
    }

    use SortDirection::{Asc, Desc};

    #[test]
    fn puts_the_states_that_need_the_user_first_then_the_newest_id_first() {
        assert_eq!(
            strs(&ids(None)),
            ["T-0003", "T-0005", "T-0004", "T-0010", "T-0002", "T-0001"]
        );
    }

    #[test]
    fn sorts_ids_as_numbers_not_as_text() {
        assert_eq!(ids(Some((SortKey::Id, Asc)))[5], "T-0010");
        assert_eq!(ids(Some((SortKey::Id, Desc)))[0], "T-0010");
    }

    #[test]
    fn sorts_titles_without_regard_to_case() {
        let result = ids(Some((SortKey::Task, Asc)));
        assert_eq!(strs(&result[..2]), ["T-0002", "T-0001"]);
    }

    #[test]
    fn sorts_pull_requests_as_numbers_and_keeps_tasks_without_one_last_in_both_directions() {
        let asc = ids(Some((SortKey::Pr, Asc)));
        let desc = ids(Some((SortKey::Pr, Desc)));
        assert_eq!(strs(&asc[..2]), ["T-0005", "T-0004"]);
        assert_eq!(strs(&desc[..2]), ["T-0004", "T-0005"]);
        let rest: Vec<String> = ids(None)
            .into_iter()
            .filter(|id| !["T-0004", "T-0005"].contains(&id.as_str()))
            .collect();
        assert_eq!(desc[2..], rest[..]);
    }

    #[test]
    fn reads_the_pull_request_number_before_an_optional_slash() {
        let mut one = task("T-0001", "a", TaskState::Ready);
        one.pr = Some("https://github.com/o/r/pull/7/".into());
        assert_eq!(pull_request_order(&one), Some(7.0));
        one.pr = Some("https://github.com/o/r/pulls".into());
        assert_eq!(pull_request_order(&one), None);
    }

    #[test]
    fn breaks_ties_with_the_default_order_so_equal_rows_keep_their_place() {
        let branch = ids(Some((SortKey::Branch, Asc)));
        assert_eq!(strs(&branch[..2]), ["T-0010", "T-0002"]);
        assert_eq!(ids(Some((SortKey::State, Asc))), ids(None));
    }

    #[test]
    fn sorts_by_the_size_of_the_work_and_puts_tasks_with_no_changes_last() {
        let stat = |files, added, removed| DiffStat {
            files,
            added,
            removed,
        };
        let stats = BTreeMap::from([
            ("T-0001".to_string(), stat(1, 50, 0)),
            ("T-0002".to_string(), stat(2, 3, 2)),
            ("T-0003".to_string(), stat(0, 0, 0)),
        ]);
        let desc = sorted_ids(Some((SortKey::Changes, Desc)), &stats, &BTreeMap::new());
        let asc = sorted_ids(Some((SortKey::Changes, Asc)), &stats, &BTreeMap::new());
        assert_eq!(strs(&desc[..2]), ["T-0001", "T-0002"]);
        assert_eq!(strs(&asc[..2]), ["T-0002", "T-0001"]);
    }

    #[test]
    fn sorts_by_cost_and_puts_tasks_with_no_price_last() {
        let usage = |cost_usd| TaskUsage {
            tokens: 10,
            cost_usd,
            ..TaskUsage::default()
        };
        let costs = BTreeMap::from([
            ("T-0001".to_string(), usage(Some(0.5))),
            ("T-0002".to_string(), usage(Some(2.0))),
            ("T-0003".to_string(), usage(None)),
        ]);
        let by_cost =
            |direction| sorted_ids(Some((SortKey::Cost, direction)), &BTreeMap::new(), &costs);
        assert_eq!(strs(&by_cost(Desc)[..2]), ["T-0002", "T-0001"]);
        assert_eq!(strs(&by_cost(Asc)[..2]), ["T-0001", "T-0002"]);
        let rest: Vec<String> = ids(None)
            .into_iter()
            .filter(|id| !["T-0001", "T-0002"].contains(&id.as_str()))
            .collect();
        assert_eq!(by_cost(Asc)[2..], rest[..]);
    }

    #[test]
    fn sorts_a_task_with_a_chosen_id_by_the_number_at_the_end_of_its_id() {
        let mixed: Vec<Task> = ["T-0010", "TASK-0005", "T-0003"]
            .into_iter()
            .map(|id| task(id, "x", TaskState::Draft))
            .collect();
        let sort = Some(ColumnSort {
            key: SortKey::Id,
            direction: Asc,
        });
        let result = sort_tasks(&mixed, sort, &BTreeMap::new(), &BTreeMap::new());
        let order: Vec<&str> = result.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(order, ["T-0003", "TASK-0005", "T-0010"]);
    }

    #[test]
    fn does_not_change_the_list_it_is_given() {
        let before = tasks();
        let sort = Some(ColumnSort {
            key: SortKey::Id,
            direction: Asc,
        });
        sort_tasks(&before, sort, &BTreeMap::new(), &BTreeMap::new());
        assert_eq!(before, tasks());
    }

    #[test]
    fn goes_ascending_descending_then_back_to_the_default_order() {
        let first = next_sort(None, SortKey::Task);
        assert_eq!(
            first,
            Some(ColumnSort {
                key: SortKey::Task,
                direction: Asc
            })
        );
        let second = next_sort(first, SortKey::Task);
        assert_eq!(
            second,
            Some(ColumnSort {
                key: SortKey::Task,
                direction: Desc
            })
        );
        assert_eq!(next_sort(second, SortKey::Task), None);
    }

    #[test]
    fn starts_ascending_on_another_column() {
        let current = Some(ColumnSort {
            key: SortKey::Task,
            direction: Desc,
        });
        assert_eq!(
            next_sort(current, SortKey::Id),
            Some(ColumnSort {
                key: SortKey::Id,
                direction: Asc
            })
        );
    }
}
