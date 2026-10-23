//! Reading the edit form (`parseLabels`, `parseTaskIds` in `features/tasks/TaskEditForm.tsx`) and
//! the text of a label chip (`labelText` in `TaskDetail.tsx`).

// Ported with their tests (spec D14); the details panel calls them.
#![cfg_attr(not(test), allow(dead_code))]

/// "ui, parser , ,api" becomes ["ui", "parser", "api"]; a label stays once.
pub fn parse_labels(text: &str) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for label in text
        .split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
    {
        if !labels.iter().any(|known| known == label) {
            labels.push(label.to_string());
        }
    }
    labels
}

/// `LETTERS-DIGITS`, as `^[A-Z]+-\d+$` reads it.
fn is_task_id_text(id: &str) -> bool {
    id.split_once('-').is_some_and(|(letters, digits)| {
        !letters.is_empty()
            && letters.bytes().all(|byte| byte.is_ascii_uppercase())
            && !digits.is_empty()
            && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// "t-1, T-0002 x" becomes ["T-1", "T-0002"]: anything that is not a task id is dropped.
pub fn parse_task_ids(text: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let words = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(|word| word.trim().to_uppercase());
    for id in words.filter(|id| is_task_id_text(id)) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// "tries-t-0002" is how the daemon groups tries. People read it as "Tries of T-0002". A chosen
/// id works the same: "tries-task-0005" is "Tries of TASK-0005".
pub fn label_text(label: &str) -> String {
    let rest = label
        .get(..6)
        .filter(|head| head.eq_ignore_ascii_case("tries-"))
        .map(|_| &label[6..]);
    match rest.and_then(|rest| rest.split_once('-')) {
        Some((letters, digits))
            if !letters.is_empty()
                && letters.bytes().all(|byte| byte.is_ascii_alphabetic())
                && !digits.is_empty()
                && digits.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            format!("Tries of {}", format!("{letters}-{digits}").to_uppercase())
        }
        _ => label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_commas_trims_and_drops_empty_and_repeated_labels() {
        assert_eq!(
            parse_labels("ui, parser , ,api,ui"),
            ["ui", "parser", "api"]
        );
        assert!(parse_labels("  ").is_empty());
    }

    #[test]
    fn keeps_task_ids_upper_case_once() {
        assert_eq!(parse_task_ids("t-1, T-0002 x T-0002,,"), ["T-1", "T-0002"]);
    }

    #[test]
    fn keeps_custom_task_ids() {
        assert_eq!(parse_task_ids("task-5 ABC-12 a1-2"), ["TASK-5", "ABC-12"]);
        assert_eq!(
            parse_task_ids("t-3, task-0005 ABC-12"),
            ["T-3", "TASK-0005", "ABC-12"]
        );
        assert_eq!(parse_task_ids("a1-2, foo, T-, T-7"), ["T-7"]);
    }

    #[test]
    fn shows_each_label_as_text() {
        let table = [
            ("tries-t-0002", "Tries of T-0002"),
            ("tries-task-0005", "Tries of TASK-0005"),
            ("tries-a1-2", "tries-a1-2"),
            ("tries-task", "tries-task"),
            ("perf", "perf"),
            ("tries-é-1", "tries-é-1"),
        ];
        for (label, text) in table {
            assert_eq!(label_text(label), text, "{label}");
        }
    }
}
