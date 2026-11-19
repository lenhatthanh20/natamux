//! The palette list as flat items: section titles and rows, so a `list` can draw only the ones
//! on screen.

use super::palette_row::row_height;
use super::palette_rows::{PaletteRow, PaletteSection};

const TITLE_HEIGHT: f32 = 24.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Title(&'static str),
    /// An index into the flat rows.
    Row(usize),
}

pub fn flatten(sections: &[PaletteSection]) -> Vec<Item> {
    let mut items = Vec::new();
    let mut index = 0;
    for section in sections {
        if let Some(title) = section.title {
            items.push(Item::Title(title));
        }
        for _ in &section.rows {
            items.push(Item::Row(index));
            index += 1;
        }
    }
    items
}

/// The rows of the sections in order, moved out of them: the view keeps each row once.
pub fn into_rows(sections: Vec<PaletteSection>) -> Vec<PaletteRow> {
    sections
        .into_iter()
        .flat_map(|section| section.rows)
        .collect()
}

/// The row with this key; none, or a row that is gone, means the first row.
pub fn active_at(rows: &[PaletteRow], key: Option<&str>) -> usize {
    key.and_then(|key| rows.iter().position(|row| row.has_key(key)))
        .unwrap_or(0)
}

/// The list item that draws row `row`.
pub fn item_of(items: &[Item], row: usize) -> Option<usize> {
    items.iter().position(|item| *item == Item::Row(row))
}

/// Pixels all items take, so the box can shrink to few rows.
pub fn content_height(items: &[Item], rows: &[PaletteRow]) -> f32 {
    items
        .iter()
        .map(|item| match item {
            Item::Title(_) => TITLE_HEIGHT,
            Item::Row(index) => rows.get(*index).map_or(0., row_height),
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::palette_rows::{PaletteCommand, RepoItem};

    #[test]
    fn titles_do_not_count_as_rows() {
        let items = [
            Item::Title("A"),
            Item::Row(0),
            Item::Title("B"),
            Item::Row(1),
        ];
        assert_eq!(item_of(&items, 1), Some(3));
        assert_eq!(item_of(&items, 2), None);
        assert_eq!(content_height(&items, &[]), 48.);
    }

    fn rows() -> Vec<PaletteRow> {
        let task = |id: &str| natamux_proto::tasks::task::Task {
            id: id.into(),
            ..Default::default()
        };
        let plain = |id: &str| PaletteRow::Task {
            task: task(id),
            title_ranges: Vec::new(),
            id_ranges: Vec::new(),
        };
        let shell = |id: &str, name: &str| PaletteRow::Shell {
            task: task(id),
            name: name.into(),
        };
        let repo = |id: &str| PaletteRow::Repo {
            repo: RepoItem {
                id: id.into(),
                name: "Project".into(),
            },
            ranges: Vec::new(),
        };
        let command = |id: &'static str| PaletteRow::Command {
            command: PaletteCommand {
                id,
                title: "Do it".into(),
                key: None,
                suggested: None,
            },
            ranges: Vec::new(),
        };
        vec![
            plain("T-1"),
            plain("T-12"),
            shell("T-1", "2"),
            shell("T-1", "2:3"),
            shell("T-1:2", "3"),
            shell("T-12", "2"),
            repo("T-1"),
            repo("web"),
            command("task:T-1"),
            command("view.tasks"),
            PaletteRow::CreateTask { title: "a".into() },
            PaletteRow::NewWorkspace { title: "a".into() },
        ]
    }

    #[test]
    fn a_row_knows_its_key_without_making_the_text() {
        let rows = rows();
        for row in &rows {
            for other in &rows {
                let key = other.key();
                assert_eq!(row.has_key(&key), row.key() == key, "{key}");
            }
            assert!(!row.has_key(""));
            assert!(!row.has_key(&format!("{}x", row.key())));
        }
    }

    #[test]
    fn the_active_row_is_the_first_one_with_the_key_or_the_first_row() {
        let rows = rows();
        for (index, row) in rows.iter().enumerate() {
            let first = rows.iter().position(|each| each.key() == row.key());
            assert_eq!(Some(active_at(&rows, Some(&row.key()))), first, "{index}");
        }
        assert_eq!(active_at(&rows, None), 0);
        assert_eq!(active_at(&rows, Some("task:gone")), 0);
        assert_eq!(active_at(&[], Some("task:T-1")), 0);
    }

    #[test]
    fn the_rows_move_out_of_their_sections_in_order() {
        let rows = rows();
        let section = |id: &'static str, rows: &[PaletteRow]| PaletteSection {
            id,
            title: None,
            rows: rows.to_vec(),
        };
        let sections = vec![
            section("a", &rows[..3]),
            section("b", &[]),
            section("c", &rows[3..]),
        ];
        assert_eq!(flatten(&sections).len(), rows.len());
        assert_eq!(into_rows(sections), rows);
    }
}
