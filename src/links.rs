//! Turning a link in a document into a path, and handing a path to the operating system.

use std::io;
use std::path::{Component, Path, PathBuf};

/// Longer hrefs give `None` without touching the file system: no real link is this long, and
/// the cap bounds the work a hostile document can cause with one click.
const MAX_HREF_LEN: usize = 32 * 1024;

/// The local path a link points at, or `None` when it names none (empty, fragment only,
/// an embedded NUL, too long). `file:` URLs are decoded; anything else is a path relative to
/// `base` with `%XX` escapes. The result is absolute and resolved, except a network path,
/// which is left untouched so the caller can ask before this PC signs in to another host.
pub fn path_from_href(href: &str, base: &Path) -> Option<PathBuf> {
    if href.len() > MAX_HREF_LEN {
        return None;
    }
    let raw = href.trim();
    let raw = raw.split_once('#').map_or(raw, |(head, _)| head);
    if raw.is_empty() {
        return None;
    }
    let path = match raw.get(..5) {
        Some(scheme) if scheme.eq_ignore_ascii_case("file:") => file_url_path(raw.get(5..)?)?,
        _ => base.join(native(percent_decode(raw))),
    };
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return None;
    }
    if is_network_path(&path) {
        return Some(path);
    }
    resolve(&path)
}

/// The path of a `file:` URL, split as Python's `urlsplit` does: `//host` first, `?query` dropped.
fn file_url_path(rest: &str) -> Option<PathBuf> {
    let rest: String = rest
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .collect();
    let (host, path) = match rest.strip_prefix("//") {
        Some(after) => after.split_at(after.find(['/', '?']).unwrap_or(after.len())),
        None => ("", rest.as_str()),
    };
    let path = path.split_once('?').map_or(path, |(head, _)| head);
    let mut local = percent_decode(path);
    if cfg!(windows) {
        // "/C:/x" is the drive path "C:/x"; a host makes it the share "\\host\x".
        if matches!(local.as_bytes(), [b'/', drive, b':', ..] if drive.is_ascii_alphabetic()) {
            local.remove(0);
        }
        if !host.is_empty() {
            local = format!(r"\\{host}{local}");
        }
    }
    (!local.is_empty()).then(|| native(local))
}

/// A decoded URL path as a native path: on Windows `/` becomes `\`, as `url2pathname` does.
fn native(path: String) -> PathBuf {
    PathBuf::from(if cfg!(windows) {
        path.replace('/', r"\")
    } else {
        path
    })
}

/// Python's `Path.resolve()` (not strict): absolute, symlinks followed even when their target is
/// missing, `.` and `..` of the missing part applied as written. One pass, left to right, so a
/// `..` applies to what a symlink points at. Links are followed here one hop at a time rather
/// than by the OS, so a link to a network share comes back unresolved, before anything
/// touches the share, and the caller asks first.
fn resolve(path: &Path) -> Option<PathBuf> {
    let mut links_left = MAX_LINKS;
    resolve_following(path, &mut links_left)
}

/// How many links one href may follow, in total, before it gives `None`: a budget, not a
/// depth, so links that loop or fan out stay bounded.
const MAX_LINKS: usize = 40;

fn resolve_following(path: &Path, links_left: &mut usize) -> Option<PathBuf> {
    if is_network_path(path) {
        return Some(path.to_path_buf());
    }
    let abs = std::path::absolute(path).ok()?;
    let mut out = PathBuf::new();
    // Components pushed below one that does not exist: they cannot exist either, so they
    // cost no lookup. This keeps a long href to about one lookup per `..`, not per prefix.
    let mut missing = 0usize;
    let mut parts = abs.components();
    while let Some(part) = parts.next() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
                missing = missing.saturating_sub(1);
            }
            Component::Normal(name) => {
                out.push(name);
                if missing > 0 {
                    missing += 1;
                } else if let Ok(target) = std::fs::read_link(&out) {
                    *links_left = links_left.checked_sub(1)?;
                    out.pop();
                    out = resolve_following(&out.join(target), links_left)?;
                    if is_network_path(&out) {
                        return Some(out.join(parts.as_path()));
                    }
                    missing = usize::from(std::fs::symlink_metadata(&out).is_err());
                } else if let Ok(real) = dunce::canonicalize(&out) {
                    // Not a link: this only fixes the spelling (case, 8.3 names on Windows).
                    out = real;
                } else {
                    missing = 1;
                }
            }
            // The drive and root: canonicalizing a bare "C:" would give the current folder.
            other => out.push(other),
        }
    }
    Some(out)
}

/// Python's `urllib.parse.unquote`: `%XX` with two hex digits becomes that byte, anything
/// else stays, and bytes that are not UTF-8 become U+FFFD.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        let escaped = match bytes.get(i + 1..i + 3) {
            Some(&[hi, lo]) if b == b'%' => hex(hi).zip(hex(lo)).map(|(hi, lo)| hi << 4 | lo),
            _ => None,
        };
        match escaped {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .and_then(|d| u8::try_from(d).ok())
}

/// A Windows path starting with two separators: touching it may make this PC sign in to
/// another host. That includes the device spellings `\\?\UNC\host\share` and `\\.\UNC\host\share`
/// a `file://` link can produce; a local `\\?\C:\` path is treated the same. Always false
/// elsewhere.
pub fn is_network_path(path: &Path) -> bool {
    cfg!(windows) && starts_with_two_separators(path.as_os_str().as_encoded_bytes())
}

fn starts_with_two_separators(s: &[u8]) -> bool {
    matches!(s, [b'\\' | b'/', b'\\' | b'/', ..])
}

/// Hands `path` to the operating system to open with its default app (Windows: ShellExecute,
/// as `os.startfile` does; macOS: `open`; elsewhere `xdg-open` and its fallbacks). This can run
/// a program, so the caller asks first. The error is returned for the caller to report.
pub fn open_path(path: &Path) -> io::Result<()> {
    open::that_detached(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real(dir: &Path) -> PathBuf {
        dunce::canonicalize(dir).unwrap()
    }

    #[test]
    fn decodes_percent_escapes_like_python_unquote() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("%C3%A9t%c3%a9"), "été");
        // Python leaves anything that is not exactly two hex digits alone.
        assert_eq!(percent_decode("%zz %+f %2 %"), "%zz %+f %2 %");
        // Bytes that are not UTF-8 become U+FFFD, as with errors="replace".
        assert_eq!(percent_decode("%ff"), "\u{fffd}");
    }

    #[test]
    fn relative_href_resolves_from_the_document() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            path_from_href("sub/a%20b.md", tmp.path()),
            Some(real(tmp.path()).join("sub").join("a b.md"))
        );
        assert_eq!(
            path_from_href("missing/../x.md#part", tmp.path()),
            Some(real(tmp.path()).join("x.md"))
        );
    }

    #[test]
    fn symlinks_are_followed_after_a_cancelled_missing_part() {
        let docs = tempfile::tempdir().unwrap();
        let srv = tempfile::tempdir().unwrap();
        let chapters = srv.path().join("chapters");
        std::fs::create_dir(&chapters).unwrap();
        let alias = docs.path().join("alias");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&chapters, &alias).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&chapters, &alias).is_err() {
            eprintln!("skipped: creating a symlink needs Developer Mode or elevation");
            return;
        }
        // Python's resolve() follows `alias` before applying the `..` after it on Unix; on
        // Windows it first removes `..` as text (GetFullPathNameW), as std::path::absolute does.
        let parent = if cfg!(windows) {
            docs.path()
        } else {
            srv.path()
        };
        assert_eq!(
            path_from_href("missing/../alias/../notes.md", docs.path()),
            Some(real(parent).join("notes.md"))
        );
    }

    #[test]
    fn dangling_and_looping_symlinks_are_followed_within_a_budget() {
        let docs = tempfile::tempdir().unwrap();
        let srv = tempfile::tempdir().unwrap();
        let gone = srv.path().join("gone");
        let link = |target: &Path, name: &str| {
            let at = docs.path().join(name);
            #[cfg(unix)]
            return std::os::unix::fs::symlink(target, at).is_ok();
            #[cfg(windows)]
            return std::os::windows::fs::symlink_dir(target, at).is_ok();
        };
        if !link(&gone, "alias") || !link(Path::new("loop"), "loop") {
            eprintln!("skipped: creating a symlink needs Developer Mode or elevation");
            return;
        }
        // Python's non-strict resolve() follows a link whose target does not exist.
        assert_eq!(
            path_from_href("alias/notes.md", docs.path()),
            Some(real(srv.path()).join("gone").join("notes.md"))
        );
        // A link to itself ends instead of recursing.
        let _ = path_from_href("loop/x.md", docs.path());
    }

    #[test]
    fn a_link_found_through_another_link_is_still_followed() {
        let docs = tempfile::tempdir().unwrap();
        let srv = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(
            srv.path().join("missing").join(".."),
            docs.path().join("up"),
        )
        .and_then(|()| std::os::unix::fs::symlink(other.path(), srv.path().join("child")));
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(
            srv.path().join("missing").join(".."),
            docs.path().join("up"),
        )
        .and_then(|()| std::os::windows::fs::symlink_dir(other.path(), srv.path().join("child")));
        if made.is_err() {
            eprintln!("skipped: creating a symlink needs Developer Mode or elevation");
            return;
        }
        assert_eq!(
            path_from_href("up/child/x.md", docs.path()),
            Some(real(other.path()).join("x.md"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_local_link_to_a_share_comes_back_unresolved_for_the_caller_to_ask() {
        let docs = tempfile::tempdir().unwrap();
        let share = Path::new(r"\\127.0.0.1\md-view-no-such-share");
        if std::os::windows::fs::symlink_dir(share, docs.path().join("alias")).is_err() {
            eprintln!("skipped: creating a symlink needs Developer Mode or elevation");
            return;
        }
        let path = path_from_href("alias/sub/a.md", docs.path()).unwrap();
        assert_eq!(path, share.join("sub").join("a.md"));
        assert!(is_network_path(&path));
    }

    #[test]
    fn fragment_and_empty_hrefs_give_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        for href in ["", "   ", "#only-a-fragment", "#", "file:", "file:#x"] {
            assert_eq!(path_from_href(href, tmp.path()), None, "{href:?}");
        }
    }

    #[test]
    fn fragment_is_dropped_and_whitespace_trimmed() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            path_from_href("  a.md#sec#tion  ", tmp.path()),
            Some(real(tmp.path()).join("a.md"))
        );
    }

    #[test]
    fn file_url_to_an_absolute_path() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = real(tmp.path());
        let slashed = dir.to_string_lossy().replace('\\', "/");
        let url = if slashed.starts_with('/') {
            format!("FILE://{slashed}/a%20b.md?q=1#part")
        } else {
            format!("FILE:///{slashed}/a%20b.md?q=1#part")
        };
        assert_eq!(
            path_from_href(&url, Path::new("elsewhere")),
            Some(dir.join("a b.md"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn file_url_with_drive_and_escapes() {
        assert_eq!(
            path_from_href("file:///C:/My%20Docs/notes.md#part", Path::new("D:/x")),
            Some(PathBuf::from(r"C:\My Docs\notes.md"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn network_file_urls_stay_unresolved_unc_paths() {
        for (href, want) in [
            ("file://zeus/share/a.md", r"\\zeus\share\a.md"),
            ("file://./UNC/zeus/share/a.md", r"\\.\UNC\zeus\share\a.md"),
            ("file:////zeus/share/a.md", r"\\zeus\share\a.md"),
            ("//zeus/share/a.md", r"\\zeus\share\a.md"),
        ] {
            let path = path_from_href(href, Path::new(r"C:\docs")).unwrap();
            assert_eq!(path.as_os_str(), want, "{href}");
            assert!(is_network_path(&path), "{href}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn network_path_spellings_on_windows() {
        for p in [
            r"\\zeus\share",
            r"\\?\UNC\zeus\share",
            r"\\.\UNC\zeus\share",
            r"\\?\C:\x",
            "//zeus/share",
        ] {
            assert!(is_network_path(Path::new(p)), "{p}");
        }
        for p in [r"C:\x", r"\x", "x", ""] {
            assert!(!is_network_path(Path::new(p)), "{p}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn nothing_is_a_network_path_elsewhere() {
        assert!(!is_network_path(Path::new(r"\\zeus\share")));
        assert!(!is_network_path(Path::new("//zeus/share")));
        // The host part of a file URL is dropped, as Python's urlparse path does.
        assert_eq!(
            path_from_href("file://zeus/tmp", Path::new("/")),
            Some(real(Path::new("/tmp")))
        );
    }

    #[test]
    fn two_leading_separators_are_detected_on_every_platform() {
        for s in [
            r"\\zeus",
            r"\\?\UNC\zeus\s",
            r"\\.\pipe",
            "//zeus",
            r"\/zeus",
            r"/\zeus",
        ] {
            assert!(starts_with_two_separators(s.as_bytes()), "{s}");
        }
        for s in [r"C:\x", r"\x", "/x", "", r"\", "x\\\\"] {
            assert!(!starts_with_two_separators(s.as_bytes()), "{s}");
        }
    }

    #[test]
    fn hostile_hrefs_never_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let long_ok = "a/".repeat(5_000);
        let too_long = "a".repeat(MAX_HREF_LEN + 1);
        for href in [
            "file://%zz",
            "%",
            "%%",
            "%e2%82",
            "file:%",
            "file://",
            "file:///",
            "file://[",
            "file://[::1",
            "file:///%00",
            "a%00b",
            "\0",
            "file:\t//x\n/y",
            "..",
            "../../../../../../../..",
            "?",
            "file:?#",
            long_ok.as_str(),
            too_long.as_str(),
        ] {
            let _ = path_from_href(href, tmp.path());
        }
        assert_eq!(path_from_href("a%00b", tmp.path()), None);
        assert_eq!(path_from_href(&too_long, tmp.path()), None);
        let padded = format!("{}a.md", " ".repeat(MAX_HREF_LEN));
        assert_eq!(path_from_href(&padded, tmp.path()), None);
    }
}
