//! A project's color index (`features/sidebar/project-color.ts`). The colors themselves come from
//! `components/colors.rs`.

/// The `--project-0` to `--project-5` colors.
pub const PROJECT_COLORS: usize = 6;

/// An index from 0 to 5 from the project's id, so the color stays the same across restarts with
/// nothing stored. The hash is the one of the React app: 31 times the hash plus the first UTF-16
/// unit of each character, in 32 bits.
pub fn project_color(id: &str) -> usize {
    let mut hash: u32 = 0;
    for char in id.chars() {
        let mut units = [0u16; 2];
        let unit = char.encode_utf16(&mut units)[0];
        hash = hash.wrapping_mul(31).wrapping_add(u32::from(unit));
    }
    hash as usize % PROJECT_COLORS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_a_project_the_same_color_every_time() {
        assert_eq!(project_color("web"), project_color("web"));
        assert_eq!(project_color("default"), 3);
        assert_eq!(project_color("web"), 0);
    }

    #[test]
    fn always_picks_one_of_the_palette_colors() {
        let long = "x".repeat(60);
        for id in [
            "default",
            "web",
            "a",
            long.as_str(),
            "my-very-long-project-name",
            "日本語😀",
        ] {
            assert!(project_color(id) < PROJECT_COLORS);
        }
    }

    #[test]
    fn spreads_projects_over_the_palette() {
        let ids = ["default", "web", "old", "natamux", "infra", "docs"];
        let mut colors: Vec<usize> = ids.iter().map(|id| project_color(id)).collect();
        colors.sort_unstable();
        colors.dedup();
        assert!(colors.len() >= 5);
    }
}
