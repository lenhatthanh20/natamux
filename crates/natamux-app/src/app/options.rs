//! Command line flags. CI uses them to check the app and take screenshots.

pub const USAGE: &str =
    "usage: natamux-app [--smoke] [--theme light|dark] [--size WxH] [--hold SECONDS] [--fake-daemon]
  --smoke   draw a frame, type into the text box, check it, then quit (exit 1 on failure)
  --theme   use this theme instead of following the system
  --size    window size in points, placed at the top left of the screen
  --hold    quit after this many seconds (for screenshots)
  --fake-daemon  show fixed tasks and a fixed terminal instead of the daemon's (for screenshots)

The daemon: NATAMUX_URL and NATAMUX_TOKEN, else http://127.0.0.1:4317 and the Natamux app's token.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub smoke: bool,
    pub theme: Option<ThemeMode>,
    pub size: Option<(u32, u32)>,
    pub hold_seconds: Option<u64>,
    pub fake_daemon: bool,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
        let mut options = Options {
            smoke: false,
            theme: None,
            size: None,
            hold_seconds: None,
            fake_daemon: false,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--smoke" => options.smoke = true,
                "--fake-daemon" => options.fake_daemon = true,
                "--theme" => options.theme = Some(parse_theme(&value(&mut args, &arg)?)?),
                "--size" => options.size = Some(parse_size(&value(&mut args, &arg)?)?),
                "--hold" => {
                    let seconds = value(&mut args, &arg)?;
                    let seconds = seconds
                        .parse()
                        .map_err(|_| format!("--hold takes whole seconds, not {seconds:?}"))?;
                    options.hold_seconds = Some(seconds);
                }
                _ => return Err(format!("unknown argument {arg:?}")),
            }
        }
        Ok(options)
    }
}

fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_theme(text: &str) -> Result<ThemeMode, String> {
    match text {
        "light" => Ok(ThemeMode::Light),
        "dark" => Ok(ThemeMode::Dark),
        _ => Err(format!("--theme is light or dark, not {text:?}")),
    }
}

fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let bad = || format!("--size looks like 1280x800, not {text:?}");
    let (width, height) = text.split_once('x').ok_or_else(bad)?;
    let width: u32 = width.parse().map_err(|_| bad())?;
    let height: u32 = height.parse().map_err(|_| bad())?;
    if width < 200 || height < 200 {
        return Err(bad());
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn no_flags_follow_the_system() {
        let options = parse(&[]).unwrap();
        assert!(!options.smoke);
        assert_eq!(options.theme, None);
        assert_eq!(options.size, None);
        assert_eq!(options.hold_seconds, None);
        assert!(!options.fake_daemon);
    }

    #[test]
    fn reads_every_flag() {
        let options = parse(&[
            "--fake-daemon",
            "--smoke",
            "--theme",
            "light",
            "--size",
            "1600x1000",
            "--hold",
            "12",
        ])
        .unwrap();
        assert!(options.smoke);
        assert_eq!(options.theme, Some(ThemeMode::Light));
        assert_eq!(options.size, Some((1600, 1000)));
        assert_eq!(options.hold_seconds, Some(12));
        assert!(options.fake_daemon);
    }

    #[test]
    fn refuses_bad_values() {
        assert!(parse(&["--theme", "blue"]).is_err());
        assert!(parse(&["--size", "1280"]).is_err());
        assert!(parse(&["--size", "10x10"]).is_err());
        assert!(parse(&["--hold", "soon"]).is_err());
        assert!(parse(&["--hold"]).is_err());
        assert!(parse(&["--fast"]).is_err());
    }
}
