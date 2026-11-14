use super::*;

#[test]
fn a_tile_fill_is_its_colour_at_20_percent_and_red_at_18() {
    for color in [
        TileColor::Gray,
        TileColor::Blue,
        TileColor::Green,
        TileColor::Yellow,
        TileColor::Cyan,
        TileColor::Violet,
    ] {
        assert_eq!(color.fill_alpha(), 0.2, "{color:?}");
    }
    assert_eq!(TileColor::Red.fill_alpha(), 0.18);
}

#[test]
fn a_project_keeps_the_avatar_colour_of_the_mockup_rule() {
    // Sum of the UTF-16 first units of each char, % 5 over blue, green, violet, cyan, yellow.
    assert_eq!(avatar_color("storefront"), TileColor::Blue);
    assert_eq!(avatar_color("natamux"), TileColor::Green);
    assert_eq!(avatar_color("a"), TileColor::Violet);
    assert_eq!(avatar_color("natamux-site"), TileColor::Cyan);
    assert_eq!(avatar_color("api"), TileColor::Yellow);
    assert_eq!(avatar_color("é"), TileColor::Cyan);
    // JS `charCodeAt(0)` of an emoji is its high surrogate.
    assert_eq!(avatar_color("😀"), TileColor::Violet);
    assert_eq!(avatar_color(""), TileColor::Blue);
}

#[test]
fn an_avatar_shows_the_first_letter_in_upper_case() {
    assert_eq!(avatar_letter("storefront"), "S");
    assert_eq!(avatar_letter("ébène"), "É");
    assert_eq!(avatar_letter(""), "");
}
