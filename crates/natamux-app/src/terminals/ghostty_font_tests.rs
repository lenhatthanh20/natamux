use super::*;

fn font(family: Option<&str>, size: Option<f32>) -> TerminalFont {
    TerminalFont {
        family: family.map(str::to_owned),
        size,
    }
}

#[test]
fn is_read_from_the_xdg_folder_and_on_macos_from_application_support_after_it() {
    assert_eq!(
        ghostty_config_paths(None, Path::new("/home/me"), false),
        [
            PathBuf::from("/home/me/.config/ghostty/config"),
            PathBuf::from("/home/me/.config/ghostty/config.ghostty"),
        ]
    );
    let support = "/Users/me/Library/Application Support/com.mitchellh.ghostty";
    assert_eq!(
        ghostty_config_paths(Some(Path::new("/xdg")), Path::new("/Users/me"), true),
        [
            PathBuf::from("/xdg/ghostty/config"),
            PathBuf::from("/xdg/ghostty/config.ghostty"),
            PathBuf::from(format!("{support}/config")),
            PathBuf::from(format!("{support}/config.ghostty")),
        ]
    );
}

#[test]
fn gives_the_first_font_family_without_quotes_and_the_last_size() {
    let text = [
        "# My terminal",
        "theme = Catppuccin Mocha",
        "font-family = \"JetBrains Mono\"",
        "font-family = Symbols Nerd Font",
        "font-size = 13",
        "font-size = 14.5",
        "# font-family = Comic Mono",
    ]
    .join("\n");
    assert_eq!(
        parse_ghostty_font(&[&text]),
        Some(font(Some("JetBrains Mono"), Some(14.5)))
    );
}

#[test]
fn starts_the_family_list_again_after_an_empty_value() {
    let text = "font-family = Menlo\nfont-family = \"\"\nfont-family = Iosevka";
    assert_eq!(
        parse_ghostty_font(&[text]),
        Some(font(Some("Iosevka"), None))
    );
}

#[test]
fn ignores_a_size_that_is_not_a_number() {
    assert_eq!(parse_ghostty_font(&["font-size = big"]), None);
}

#[test]
fn lets_a_later_file_set_the_font_again_and_keeps_what_it_does_not_set() {
    let read = |path: &Path| match path.to_str() {
        Some("/home/me/.config/ghostty/config") => {
            Some("font-family = Menlo\nfont-size = 12\n".to_owned())
        }
        Some("/home/me/.config/ghostty/config.ghostty") => Some("font-family = Iosevka\n".into()),
        _ => None,
    };
    assert_eq!(
        read_ghostty_font_with(None, Path::new("/home/me"), false, read),
        Some(font(Some("Iosevka"), Some(12.0)))
    );
}

#[test]
fn gives_nothing_when_ghostty_has_no_config_or_sets_no_font() {
    assert_eq!(
        read_ghostty_font_with(None, Path::new("/home/me"), false, |_| None),
        None
    );
    assert_eq!(
        read_ghostty_font_with(None, Path::new("/home/me"), false, |_| Some(
            "theme = dark\n".into()
        )),
        None
    );
}

#[test]
fn reads_crlf_lines_and_spaces_around_the_equals_sign() {
    let text = "  font-family=\"Menlo\"\r\nfont-size =  15 \r\n";
    assert_eq!(
        parse_ghostty_font(&[text]),
        Some(font(Some("Menlo"), Some(15.0)))
    );
}
