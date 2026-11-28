//! File types, constants, the per-user data folder, preferences and the style choice.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{env, fs, process};

use serde_json::{Map, Value};

pub const APP_NAME: &str = "md-view";

// --------------------------------------------------------------------------- //
// Supported file types (lower case, with the dot, as `Path::extension` is compared)
// --------------------------------------------------------------------------- //

pub const MARKDOWN_EXTS: &[&str] = &[".md", ".markdown", ".mdown", ".mkd", ".mkdn", ".mdtext"];
pub const MERMAID_EXTS: &[&str] = &[".mmd", ".mermaid"];
pub const RASTER_EXTS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico", ".apng", ".avif",
];
pub const SVG_EXTS: &[&str] = &[".svg", ".svgz"];
pub const CSV_EXTS: &[&str] = &[".csv", ".tsv", ".tab"];
pub const TEXT_EXTS: &[&str] = &[
    ".txt", ".text", ".log", ".rst", ".json", ".yml", ".yaml", ".toml", ".ini",
];

/// Image files: raster or SVG.
pub fn is_image_ext(ext: &str) -> bool {
    RASTER_EXTS.contains(&ext) || SVG_EXTS.contains(&ext)
}

/// Every file type the viewer opens.
pub fn is_viewable_ext(ext: &str) -> bool {
    [MARKDOWN_EXTS, MERMAID_EXTS, CSV_EXTS, TEXT_EXTS]
        .iter()
        .any(|exts| exts.contains(&ext))
        || is_image_ext(ext)
}

// Pinned and checked: this script runs in a page that can call into the app.
// To upgrade, change all three (sha256 of dist/mermaid.min.js, as served); a test checks the URL's version.
pub const MERMAID_VERSION: &str = "11.17.2";
pub const MERMAID_SHA256: &str = "581ed7d74bd9048d0e3a91363927d72ef22942d7722546b27f7cc29e35390eb8";
pub const MERMAID_CDN: &str = "https://cdn.jsdelivr.net/npm/mermaid@11.17.2/dist/mermaid.min.js";

pub const ZOOM_STEPS: &[f64] = &[
    0.25, 0.33, 0.50, 0.67, 0.75, 0.90, 1.00, 1.10, 1.25, 1.50, 1.75, 2.00, 2.50, 3.00, 4.00, 5.00,
];

// Presentation styles. Each one is a full look: palette, syntax colours, mermaid theme and type stack.
pub const STYLES: &[&str] = &["light", "dark", "github", "github-ext"];
/// GitHub's look, with tables allowed to grow past the text column.
pub const DEFAULT_STYLE: &str = "github-ext";

pub fn style_label(style: &str) -> &'static str {
    match style {
        "light" => "Light",
        "dark" => "Dark",
        "github" => "GitHub",
        _ => "GitHub Ext",
    }
}

pub fn style_bg(style: &str) -> &'static str {
    match style {
        "light" => "#fcfcfd",
        "dark" => "#0d1117",
        _ => "#ffffff",
    }
}

/// Body typeface. "system" keeps whatever the chosen style specifies.
pub const FONTS: &[&str] = &["system", "sans", "serif", "mono"];

pub fn font_label(font: &str) -> &'static str {
    match font {
        "sans" => "Sans",
        "serif" => "Serif",
        "mono" => "Monospace",
        _ => "System",
    }
}

/// Rainbow CSV: one colour per column, cycling.
pub const CSV_COLOURS: usize = 10;
pub const CSV_MAX_ROWS: usize = 5000;

// --------------------------------------------------------------------------- //
// Paths, preferences, style
// --------------------------------------------------------------------------- //

fn home() -> PathBuf {
    env::home_dir().unwrap_or_default()
}

/// Per-user directory for cached assets and preferences.
///
/// Read from the environment on every call, so pointing `LOCALAPPDATA` (Windows) or
/// `XDG_DATA_HOME` at a scratch folder keeps a test run off the person's own data.
pub fn data_dir() -> PathBuf {
    let root = if cfg!(windows) {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join("AppData").join("Local"))
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support")
    } else {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".local").join("share"))
    };
    let d = root.join(APP_NAME);
    let _ = fs::create_dir_all(&d);
    d
}

pub fn assets_dir() -> PathBuf {
    let d = data_dir().join("assets");
    let _ = fs::create_dir_all(&d);
    d
}

pub fn prefs_path() -> PathBuf {
    data_dir().join("prefs.json")
}

pub type Prefs = Map<String, Value>;

/// The saved preferences, or none when the file is missing or unreadable.
pub fn load_prefs(path: &Path) -> Prefs {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

// Bridge calls persist from several threads; cleanup stays inside so it cannot delete another thread's staged file.
static PREFS_LOCK: Mutex<()> = Mutex::new(());

/// Write via a temp file and a rename, so a crash or a second writer never leaves a
/// half-written prefs.json (which would silently reset every setting).
/// Preferences are a convenience, never a failure mode: errors are swallowed.
/// "A crash" means the app's: nothing is synced to disk, so a power loss can lose the last save.
pub fn save_prefs(path: &Path, prefs: &Prefs) {
    let tmp = path.with_file_name(format!("prefs.{}.tmp", process::id()));
    let _guard = PREFS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let written = serde_json::to_string_pretty(prefs)
        .map_err(std::io::Error::other)
        .and_then(|text| fs::write(&tmp, text))
        .and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Pick a style: explicit flag, then the remembered one, then the default.
///
/// A remembered style only counts once it was chosen while DEFAULT_STYLE was the default:
/// prefs saved by an older version are moved onto the new default once, and any choice
/// made after that sticks.
pub fn resolve_style(requested: Option<&str>, prefs: &Prefs) -> &'static str {
    let known = |name: Option<&str>| name.and_then(|n| STYLES.iter().copied().find(|s| *s == n));
    if let Some(style) = known(requested) {
        return style;
    }
    if prefs.get("style_default").and_then(Value::as_str) == Some(DEFAULT_STYLE) {
        for key in ["style", "theme"] {
            if let Some(style) = known(prefs.get(key).and_then(Value::as_str)) {
                return style;
            }
        }
    }
    DEFAULT_STYLE
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    fn prefs(value: Value) -> Prefs {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn style_choice_rules() {
        assert_eq!(resolve_style(None, &Prefs::new()), DEFAULT_STYLE);
        // prefs from before github-ext existed move to the new default once ...
        assert_eq!(
            resolve_style(None, &prefs(json!({"style": "dark"}))),
            DEFAULT_STYLE
        );
        // ... and a choice made after that sticks; an explicit flag always wins
        let chosen = prefs(json!({"style": "dark", "style_default": DEFAULT_STYLE}));
        assert_eq!(resolve_style(None, &chosen), "dark");
        assert_eq!(resolve_style(Some("light"), &chosen), "light");
        // an unknown flag or remembered name falls through
        assert_eq!(
            resolve_style(Some("neon"), &prefs(json!({"style": 3}))),
            DEFAULT_STYLE
        );
    }

    #[test]
    fn mermaid_url_names_the_pinned_version() {
        assert!(MERMAID_CDN.contains(&format!("@{MERMAID_VERSION}/")));
    }

    #[test]
    fn viewable_extensions() {
        assert!(is_viewable_ext(".md") && is_viewable_ext(".svgz") && is_viewable_ext(".tab"));
        assert!(!is_viewable_ext(".exe") && !is_viewable_ext("md"));
    }

    #[test]
    fn unreadable_prefs_are_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        assert!(load_prefs(&path).is_empty());
        fs::write(&path, "{not json").unwrap();
        assert!(load_prefs(&path).is_empty());
    }

    // prefs.json is rewritten on every sidebar drag or zoom: it must never be seen half-written.
    #[test]
    fn concurrent_saves_never_expose_a_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let expanded: Vec<String> = (0..4000)
            .map(|i| format!("root\x1f/some/long/folder/{i}"))
            .collect();
        let data = prefs(json!({ "expanded": expanded }));
        save_prefs(&path, &prefs(json!({"seed": true})));

        let stop = Arc::new(AtomicBool::new(false));
        let broken = Arc::new(AtomicUsize::new(0));
        let reader = {
            let (path, stop, broken) = (path.clone(), stop.clone(), broken.clone());
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    // an Err is a mid-rename on Windows; a real reader just retries
                    if let Ok(text) = fs::read_to_string(&path) {
                        if serde_json::from_str::<Value>(&text).is_err() {
                            broken.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            })
        };
        let writers: Vec<_> = (0..4)
            .map(|_| {
                let (path, data) = (path.clone(), data.clone());
                thread::spawn(move || (0..20).for_each(|_| save_prefs(&path, &data)))
            })
            .collect();
        writers.into_iter().for_each(|w| w.join().unwrap());
        stop.store(true, Ordering::Relaxed);
        reader.join().unwrap();

        assert_eq!(broken.load(Ordering::Relaxed), 0);
        assert_eq!(load_prefs(&path), data);
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["prefs.json"]);
    }

    #[test]
    fn failed_save_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        // the target is a folder, so the rename fails after the temp file is written
        let path = dir.path().join("prefs.json");
        fs::create_dir(&path).unwrap();
        save_prefs(&path, &prefs(json!({"a": 1})));
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["prefs.json"]);
    }
}
