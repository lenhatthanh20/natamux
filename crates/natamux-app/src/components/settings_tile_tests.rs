use super::*;

#[test]
fn each_tile_size_has_the_mockup_box_radius_and_glyph() {
    assert_eq!(TileSize::Nav.metrics(), (20., 5., 12., 11.));
    assert_eq!(TileSize::Row.metrics(), (24., 6., 13., 11.));
    assert_eq!(TileSize::Header.metrics(), (32., 8., 16., 15.));
}
