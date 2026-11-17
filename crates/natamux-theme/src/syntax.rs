//! The code colours of a theme: one whole style per capture name, in the order of the theme file.

use std::collections::HashMap;

use gpui::{HighlightStyle, SharedString};

#[derive(Clone, Debug, Default)]
pub struct SyntaxTheme {
    names: Vec<SharedString>,
    styles: Vec<HighlightStyle>,
    index: HashMap<SharedString, usize>,
}

impl SyntaxTheme {
    /// A name given twice is found at its last entry.
    pub fn new(entries: impl IntoIterator<Item = (String, HighlightStyle)>) -> Self {
        let mut theme = SyntaxTheme::default();
        for (name, style) in entries {
            let name = SharedString::from(name);
            theme.index.insert(name.clone(), theme.names.len());
            theme.names.push(name);
            theme.styles.push(style);
        }
        theme
    }

    pub fn get(&self, index: impl Into<usize>) -> Option<&HighlightStyle> {
        self.styles.get(index.into())
    }

    pub fn get_capture_name(&self, index: impl Into<usize>) -> Option<&str> {
        self.names.get(index.into()).map(SharedString::as_ref)
    }

    /// The style of exactly this name.
    pub fn style_for_name(&self, name: &str) -> Option<HighlightStyle> {
        self.index.get(name).map(|&at| self.styles[at])
    }

    /// The index of the longest name that is `capture` or a part of it up to a dot:
    /// `string.special.url` takes `string.special`, else `string`.
    pub fn highlight_id(&self, capture: &str) -> Option<u32> {
        let mut name = capture;
        loop {
            if let Some(&at) = self.index.get(name) {
                return u32::try_from(at).ok();
            }
            name = &name[..name.rfind('.')?];
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{FontWeight, HighlightStyle};

    use super::*;

    fn weight(value: f32) -> HighlightStyle {
        HighlightStyle {
            font_weight: Some(FontWeight(value)),
            ..HighlightStyle::default()
        }
    }

    fn theme() -> SyntaxTheme {
        SyntaxTheme::new([
            ("string".to_string(), weight(1.)),
            ("string.special".to_string(), weight(2.)),
            ("comment".to_string(), weight(3.)),
        ])
    }

    #[test]
    fn keeps_the_order_of_the_file() {
        let theme = theme();
        assert_eq!(theme.get_capture_name(2usize), Some("comment"));
        assert_eq!(theme.get(1usize), Some(&weight(2.)));
        assert_eq!(theme.get(3usize), None);
    }

    #[test]
    fn finds_the_longest_part_up_to_a_dot() {
        let theme = theme();
        assert_eq!(theme.highlight_id("string.special.url"), Some(1));
        assert_eq!(theme.highlight_id("string.escape"), Some(0));
        assert_eq!(theme.highlight_id("string"), Some(0));
        assert_eq!(theme.highlight_id("strings"), None);
        assert_eq!(theme.highlight_id("keyword"), None);
        assert_eq!(theme.style_for_name("string.escape"), None);
        assert_eq!(theme.style_for_name("comment"), Some(weight(3.)));
    }

    #[test]
    fn a_name_given_twice_is_found_at_its_last_entry() {
        let theme = SyntaxTheme::new([
            ("a".to_string(), weight(1.)),
            ("a".to_string(), weight(2.)),
        ]);
        assert_eq!(theme.style_for_name("a"), Some(weight(2.)));
        assert_eq!(theme.highlight_id("a.b"), Some(1));
    }
}
