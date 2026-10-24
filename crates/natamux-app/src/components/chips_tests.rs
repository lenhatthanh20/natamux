/// The source of `StateChip::render`, so the look can be checked without a window.
fn state_chip_render() -> &'static str {
    let source = include_str!("chips.rs");
    let start = source
        .find("impl RenderOnce for StateChip")
        .expect("render");
    let rest = &source[start..];
    let end = rest.find("\n}\n").expect("end of render");
    &rest[..end]
}

#[test]
fn a_state_chip_is_grey_with_the_state_colour_only_in_its_dot() {
    // The owner found the filled state pills ugly (T-0169).
    let render = state_chip_render();
    assert!(render.contains(".bg(colors.element_background)"));
    assert!(render.contains(".border_color(colors.border_variant)"));
    assert!(render.contains(".color(Color::Muted)"));
    assert!(!render.contains("Color::Custom"), "state-coloured text");
    assert_eq!(
        render.matches("state_colors(").count(),
        1,
        "one state colour"
    );
    assert!(render.contains(".bg(dot)"));
}
