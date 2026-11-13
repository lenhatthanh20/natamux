//! What a paste sends to a terminal: ESC is dropped so the text cannot end the paste early, and
//! the program's bracketed paste marks go around it when it turned them on. Also what a drop of
//! files from the OS file manager types.

use std::path::PathBuf;

const START: &str = "\x1b[200~";
const END: &str = "\x1b[201~";

pub fn bracketed(text: &str, on: bool) -> String {
    let clean = text.replace('\x1b', "");
    if on {
        format!("{START}{clean}{END}")
    } else {
        clean
    }
}

/// What a drop of OS files types: each path as given, quoted when the shell would split or expand
/// it, then a space. `windows` picks PowerShell quoting (the daemon's shell there), else POSIX.
pub fn os_drop_text(paths: &[PathBuf], windows: bool) -> String {
    paths
        .iter()
        .map(|path| format!("{} ", quote_path(&path.to_string_lossy(), windows)))
        .collect()
}

fn quote_path(text: &str, windows: bool) -> String {
    let safe = if windows { "._-\\:/" } else { "._/@+-" };
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || safe.contains(c));
    if plain {
        text.to_string()
    } else if windows {
        format!("'{}'", text.replace('\'', "''"))
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_the_text_when_bracketed_paste_is_on() {
        assert_eq!(bracketed("src/a.ts ", true), "\x1b[200~src/a.ts \x1b[201~");
    }

    #[test]
    fn sends_the_text_as_it_is_when_it_is_off() {
        assert_eq!(bracketed("src/a.ts ", false), "src/a.ts ");
    }

    #[test]
    fn strips_escape_so_the_text_cannot_end_the_paste() {
        assert_eq!(bracketed("a\x1b[201~b", true), "\x1b[200~a[201~b\x1b[201~");
        assert_eq!(bracketed("a\x1bb", false), "ab");
    }

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn types_a_plain_posix_path_as_it_is_then_a_space() {
        assert_eq!(os_drop_text(&paths(&["/tmp/a.txt"]), false), "/tmp/a.txt ");
    }

    #[test]
    fn quotes_a_posix_path_with_a_space_or_a_quote() {
        assert_eq!(
            os_drop_text(&paths(&["/Users/a/my docs/x.txt"]), false),
            "'/Users/a/my docs/x.txt' "
        );
        assert_eq!(
            os_drop_text(&paths(&["/a/it's.txt"]), false),
            "'/a/it'\\''s.txt' "
        );
    }

    #[test]
    fn types_every_dropped_path_in_order() {
        assert_eq!(
            os_drop_text(&paths(&["/a/x.txt", "/b/my y.txt"]), false),
            "/a/x.txt '/b/my y.txt' "
        );
    }

    #[test]
    fn quotes_for_powershell_on_windows() {
        assert_eq!(
            os_drop_text(&paths(&["C:\\Users\\a\\x.txt"]), true),
            "C:\\Users\\a\\x.txt "
        );
        assert_eq!(
            os_drop_text(&paths(&["C:\\my docs\\x.txt"]), true),
            "'C:\\my docs\\x.txt' "
        );
        assert_eq!(
            os_drop_text(&paths(&["C:\\a\\it's.txt"]), true),
            "'C:\\a\\it''s.txt' "
        );
    }

    #[test]
    fn types_nothing_for_no_paths() {
        assert_eq!(os_drop_text(&[], false), "");
    }

    fn posix(path: &str) -> String {
        os_drop_text(&[PathBuf::from(path)], false)
    }

    fn win(path: &str) -> String {
        os_drop_text(&[PathBuf::from(path)], true)
    }

    #[test]
    fn keeps_every_allowed_posix_char_plain_and_quotes_any_other() {
        assert_eq!(posix("/a/b_c.d@e+f-g"), "/a/b_c.d@e+f-g ");
        for c in ['~', '$', '&', '(', '#', 'é'] {
            let path = format!("/a/x{c}y");
            assert_eq!(posix(&path), format!("'{path}' "));
        }
    }

    #[test]
    fn quotes_posix_paths_with_quotes_anywhere() {
        assert_eq!(posix("'"), "''\\''' ");
        assert_eq!(posix("a''b"), "'a'\\'''\\''b' ");
        assert_eq!(posix("'a"), "''\\''a' ");
        assert_eq!(posix("a'"), "'a'\\''' ");
    }

    #[test]
    fn keeps_shell_chars_literal_inside_posix_quotes() {
        assert_eq!(posix("/a/$x`y\"z\\w"), "'/a/$x`y\"z\\w' ");
    }

    #[test]
    fn keeps_forward_slashes_plain_on_windows_and_quotes_shell_chars() {
        assert_eq!(win("C:/Users/a/x.txt"), "C:/Users/a/x.txt ");
        for c in ['$', '`', '(', '&', ';'] {
            let path = format!("C:\\a{c}b");
            assert_eq!(win(&path), format!("'{path}' "));
        }
    }

    #[test]
    fn quotes_backslash_and_colon_in_posix_style() {
        assert_eq!(posix("C:\\a"), "'C:\\a' ");
    }

    #[test]
    fn types_nothing_for_no_paths_on_windows() {
        assert_eq!(os_drop_text(&[], true), "");
    }

    #[test]
    fn types_a_folder_as_given_without_adding_a_slash() {
        assert_eq!(posix("/Users/a/proj"), "/Users/a/proj ");
        assert_eq!(posix("/Users/a/proj/"), "/Users/a/proj/ ");
        assert_eq!(posix("/Users/a/my proj"), "'/Users/a/my proj' ");
    }

    #[cfg(unix)]
    #[test]
    fn quotes_a_path_that_is_not_utf8() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let path = PathBuf::from(OsStr::from_bytes(b"/a/x\xffy"));
        assert_eq!(os_drop_text(&[path], false), "'/a/x\u{fffd}y' ");
    }
}
