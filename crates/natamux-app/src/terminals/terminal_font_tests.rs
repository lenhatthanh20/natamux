use super::*;

fn font(family: Option<&str>, size: Option<f32>) -> TerminalFont {
    TerminalFont {
        family: family.map(str::to_owned),
        size,
    }
}

#[test]
fn is_the_built_in_font_when_nothing_is_set() {
    assert_eq!(
        terminal_font(None),
        ResolvedFont {
            family: "Lilex".into(),
            size: 15.0,
            source: FontSource::Default
        }
    );
}

// The app reads no Ghostty config (owner, 2026-10-24): a part the user left out is Zed's.
#[test]
fn takes_the_users_font_part_by_part_and_the_built_in_one_for_the_rest() {
    let custom = font(None, Some(16.0));
    let resolved = terminal_font(Some(&custom));
    assert_eq!(
        (resolved.family.as_str(), resolved.size, resolved.source),
        ("Lilex", 16.0, FontSource::Custom)
    );
    let custom = font(Some("Fira Code"), None);
    assert_eq!(terminal_font(Some(&custom)).family, "Fira Code");
    assert_eq!(terminal_font(Some(&custom)).size, 15.0);
}

#[test]
fn keeps_quotes_out_of_the_family_so_the_css_stays_one_list() {
    let custom = font(Some("Fira \"Code\\"), None);
    assert_eq!(terminal_font(Some(&custom)).family, "Fira Code");
    assert_eq!(clean_family("Fira \"Code\\"), "Fira Code");
    let blank = font(Some("  "), None);
    assert_eq!(terminal_font(Some(&blank)).family, "Lilex");
}

#[test]
fn drops_a_broken_value_or_a_size_out_of_range() {
    assert_eq!(parse_terminal_font_text("{ not json"), None);
    assert_eq!(parse_terminal_font(&json!({ "size": 200 })), None);
    assert_eq!(
        parse_terminal_font(&json!({ "family": "Menlo", "size": 2 })),
        Some(font(Some("Menlo"), None))
    );
}

#[test]
fn reads_and_writes_a_font_as_json() {
    let read = parse_terminal_font(&json!({ "family": " Menlo ", "size": 13.5 }));
    assert_eq!(read, Some(font(Some("Menlo"), Some(13.5))));
    assert_eq!(
        font(Some("Menlo"), Some(13.5)).to_json(),
        json!({ "family": "Menlo", "size": 13.5 })
    );
    assert_eq!(parse_terminal_font(&json!("Menlo")), None);
    assert_eq!(parse_terminal_font(&json!({})), None);
}

#[test]
fn saves_the_font_the_user_types_and_closes() {
    let saved = font_to_save(" Iosevka ", "15");
    assert_eq!(saved, Some(font(Some("Iosevka"), Some(15.0))));
    let resolved = terminal_font(saved.as_ref());
    assert_eq!(
        (resolved.family.as_str(), resolved.size, resolved.source),
        ("Iosevka", 15.0, FontSource::Custom)
    );
    assert_eq!(font_to_save("", "  "), None);
    assert_eq!(font_to_save("", "9"), Some(font(None, Some(9.0))));
}

#[test]
fn names_the_built_in_font_in_the_text_and_as_the_placeholder() {
    assert!(dialog_description().contains("built-in font, Lilex, 15px"));
    assert_eq!(RESET_LABEL, "Use built-in font");
}

#[test]
fn does_not_save_a_size_out_of_range() {
    assert_eq!(size_ok("99", 13.0), None);
    assert_eq!(size_ok("7.9", 13.0), None);
    assert_eq!(size_ok("big", 13.0), None);
    assert_eq!(size_ok("nan", 13.0), None);
    assert_eq!(size_ok("8", 13.0), Some(8.0));
    assert_eq!(size_ok("32", 13.0), Some(32.0));
    assert_eq!(size_ok("", 14.0), Some(14.0));
    assert_eq!(SIZE_REASON, "Pick a size from 8 to 32");
}
