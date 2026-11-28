//! Command-line argument parsing matching py-md-view.

use crate::config::STYLES;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub paths: Vec<PathBuf>,
    pub style: Option<String>,
    pub no_watch: bool,
    pub help: bool,
    pub version: bool,
}

pub fn parse_args<I, T>(iter: I) -> Result<Args, String>
where
    I: IntoIterator<Item = T>,
    T: Into<String>,
{
    let mut paths = Vec::new();
    let mut style = None;
    let mut no_watch = false;
    let mut help = false;
    let mut version = false;

    let mut args = iter.into_iter().peekable();

    while let Some(arg) = args.next() {
        let s = arg.into();
        match s.as_str() {
            "-h" | "--help" => help = true,
            "-v" | "--version" => version = true,
            "--no-watch" => no_watch = true,
            "--light" => style = Some("light".to_string()),
            "--dark" => style = Some("dark".to_string()),
            "--github" => style = Some("github".to_string()),
            "--github-ext" => style = Some("github-ext".to_string()),
            "--style" => {
                if let Some(val) = args.next() {
                    let val_str = val.into();
                    if STYLES.contains(&val_str.as_str()) {
                        style = Some(val_str);
                    } else {
                        return Err(format!(
                            "unknown style: {val_str} (allowed: {})",
                            STYLES.join(", ")
                        ));
                    }
                } else {
                    return Err("--style requires an argument".to_string());
                }
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option: {other}"));
            }
            other => {
                paths.push(PathBuf::from(other));
            }
        }
    }

    Ok(Args {
        paths,
        style,
        no_watch,
        help,
        version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_paths_and_flags() {
        let args = parse_args(["readme.md", "--dark", "--no-watch", "notes.txt"]).unwrap();
        assert_eq!(
            args.paths,
            vec![PathBuf::from("readme.md"), PathBuf::from("notes.txt")]
        );
        assert_eq!(args.style.as_deref(), Some("dark"));
        assert!(args.no_watch);
        assert!(!args.help);
        assert!(!args.version);
    }

    #[test]
    fn parses_all_style_flags() {
        for (flag, expected) in [
            ("--light", "light"),
            ("--dark", "dark"),
            ("--github", "github"),
            ("--github-ext", "github-ext"),
        ] {
            let args = parse_args([flag]).unwrap();
            assert_eq!(args.style.as_deref(), Some(expected));
        }

        let args = parse_args(["--style", "light"]).unwrap();
        assert_eq!(args.style.as_deref(), Some("light"));
    }

    #[test]
    fn rejects_invalid_style() {
        assert!(parse_args(["--style", "neon"]).is_err());
        assert!(parse_args(["--style"]).is_err());
        assert!(parse_args(["--unknown"]).is_err());
    }

    #[test]
    fn parses_help_and_version() {
        assert!(parse_args(["--help"]).unwrap().help);
        assert!(parse_args(["-h"]).unwrap().help);
        assert!(parse_args(["--version"]).unwrap().version);
        assert!(parse_args(["-v"]).unwrap().version);
    }
}
