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
}
