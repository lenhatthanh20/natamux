//! `slugify` (`slug.ts`): an id made of lower case words, such as a template's.

const MAX_SLUG_LENGTH: usize = 40;

pub fn slugify(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut slug = String::new();
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let trimmed = slug.trim_matches('-');
    let cut: String = trimmed.chars().take(MAX_SLUG_LENGTH).collect();
    let slug = cut.trim_end_matches('-');
    if slug.is_empty() {
        "task".into()
    } else {
        slug.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_a_title_into_a_slug() {
        for (title, slug) in [
            ("Fix the bug", "fix-the-bug".to_string()),
            ("  Fix: the   BUG!!  ", "fix-the-bug".into()),
            ("日本語", "task".into()),
            ("---", "task".into()),
            (&"a".repeat(60), "a".repeat(40)),
            (
                &"word ".repeat(9),
                "word-word-word-word-word-word-word-word".into(),
            ),
        ] {
            assert_eq!(slugify(title), slug, "{title:?}");
        }
    }
}
