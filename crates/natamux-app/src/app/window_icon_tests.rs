use super::*;

#[test]
fn app_id_matches_the_desktop_file() {
    assert_eq!(APP_ID, "natamux");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_window_icon_is_256_px() {
    let icon = window_icon().expect("embedded icon decodes");
    assert_eq!(icon.dimensions(), (256, 256));
}
