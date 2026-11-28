//! What a document is and what it holds: file types, text reading, delimited text, SVG size.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::config::{
    CSV_COLOURS, CSV_MAX_ROWS, MARKDOWN_EXTS, MERMAID_EXTS, RASTER_EXTS, SVG_EXTS,
};
use crate::html::{escape, thousands};

/// The lower-cased extension with its dot, or "" (Python's `path.suffix.lower()`).
pub fn suffix(path: &Path) -> String {
    path.extension()
        .filter(|e| !e.is_empty()) // "notes." has no suffix in Python
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Markdown,
    Mermaid,
    Svg,
    Raster,
    Csv,
    Text,
}

pub fn classify(path: &Path) -> Kind {
    let ext = suffix(path);
    let ext = ext.as_str();
    if MARKDOWN_EXTS.contains(&ext) {
        Kind::Markdown
    } else if MERMAID_EXTS.contains(&ext) {
        Kind::Mermaid
    } else if SVG_EXTS.contains(&ext) {
        Kind::Svg
    } else if RASTER_EXTS.contains(&ext) {
        Kind::Raster
    } else if crate::config::CSV_EXTS.contains(&ext) {
        Kind::Csv
    } else {
        Kind::Text
    }
}

// --------------------------------------------------------------------------- //
// Delimited text: rainbow columns
// --------------------------------------------------------------------------- //

const DELIMITERS: [char; 4] = [',', ';', '\t', '|'];

/// Best guess at the field separator: Python's `csv.Sniffer().sniff(sample, ",;\t|")`, and where
/// that gives up, the candidate seen most often on the first line.
pub fn sniff_delimiter(sample: &str, ext: &str) -> char {
    if ext == ".tsv" || ext == ".tab" {
        return '\t';
    }
    let chars: Vec<char> = sample.chars().collect();
    if let Some(d) = quoted_field_delimiter(&chars).or_else(|| consistent_delimiter(sample)) {
        return d;
    }
    // Irregular files: count candidates on the first line instead.
    let first = sample.split('\n').next().unwrap_or("");
    let (best, count) = DELIMITERS
        .iter()
        .map(|&d| (d, first.matches(d).count()))
        .fold((',', 0), |best, c| if c.1 > best.1 { c } else { best });
    if count > 0 {
        best
    } else {
        ','
    }
}

// Python's `\w` on str: letters (L*), numbers (N*) and `_`. `char::is_alphanumeric` also takes
// the combining marks of the Other_Alphabetic property, such as Devanagari vowel signs.
static WORD_CHAR: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^[\p{L}\p{N}_]$").ok());

fn is_word(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_';
    }
    WORD_CHAR
        .as_ref()
        .is_some_and(|re| re.is_match(c.encode_utf8(&mut [0; 4])))
}

/// The Sniffer's first guess: the delimiter that most often sits next to a quoted field.
///
/// Its three patterns, tried in order until one matches anywhere, with `re.findall`'s
/// non-overlapping scan: `,".*?",` then `^".*?",` then `,".*?"$` (DOTALL, MULTILINE; a
/// delimiter is any character but a word character, newline or quote, and may be followed by
/// one space). The fourth pattern has no delimiter group, so it never yields one.
/// Quadratic at worst, like the regexes: callers pass the 8192-character sample, not the file.
fn quoted_field_delimiter(s: &[char]) -> Option<char> {
    let is_quote = |c: char| c == '"' || c == '\'';
    let is_delim = |c: char| !(is_word(c) || c == '\n' || is_quote(c));
    let at = |i: usize| s.get(i).copied();
    let quotes: Vec<usize> = (0..s.len()).filter(|&i| is_quote(s[i])).collect();
    // `.*?(?P=quote)` followed by `follows`: the first matching quote after `open`.
    let close = |open: usize, follows: &dyn Fn(usize) -> bool| {
        quotes[quotes.partition_point(|&j| j <= open)..]
            .iter()
            .copied()
            .find(|&j| s[j] == s[open] && follows(j + 1))
    };
    // A delimiter, an optional space, an opening quote: where that quote is.
    let opening = |pos: usize| {
        let open = if at(pos + 1) == Some(' ') {
            pos + 2
        } else {
            pos + 1
        };
        (is_delim(s[pos]) && at(open).is_some_and(is_quote)).then_some(open)
    };
    let line_end = |i: usize| i == s.len() || s[i] == '\n';

    let patterns: [&dyn Fn(usize) -> Option<(char, usize)>; 3] = [
        // ,".*?",
        &|pos| {
            let open = opening(pos)?;
            close(open, &|i| at(i) == Some(s[pos])).map(|j| (s[pos], j + 2))
        },
        // (^|\n)".*?",
        &|pos| {
            let line_start = (pos == 0 || s[pos - 1] == '\n').then_some(pos);
            let after_newline = (s[pos] == '\n').then_some(pos + 1);
            let open = [line_start, after_newline]
                .into_iter()
                .flatten()
                .find(|&o| at(o).is_some_and(is_quote))?;
            let j = close(open, &|i| at(i).is_some_and(is_delim))?;
            Some((s[j + 1], if at(j + 2) == Some(' ') { j + 3 } else { j + 2 }))
        },
        // ,".*?"($|\n): `$` matches first and consumes nothing
        &|pos| {
            let open = opening(pos)?;
            close(open, &line_end).map(|j| (s[pos], j + 1))
        },
    ];
    for pattern in patterns {
        let mut found: Vec<char> = Vec::new();
        let mut pos = 0;
        while pos < s.len() {
            match pattern(pos) {
                Some((d, end)) => {
                    found.push(d);
                    pos = end;
                }
                None => pos += 1,
            }
        }
        if found.is_empty() {
            continue;
        }
        // The most frequent counted delimiter; a tie goes to the one seen first.
        let mut counts: Vec<(char, usize)> = Vec::new();
        for d in found.into_iter().filter(|d| DELIMITERS.contains(d)) {
            match counts.iter_mut().find(|(c, _)| *c == d) {
                Some(entry) => entry.1 += 1,
                None => counts.push((d, 1)),
            }
        }
        return counts
            .into_iter()
            .fold(None, |best: Option<(char, usize)>, c| match best {
                Some(b) if b.1 >= c.1 => Some(b),
                _ => Some(c),
            })
            .map(|(d, _)| d);
    }
    None
}

/// The Sniffer's second guess: a candidate that occurs the same number of times on (nearly)
/// every line, judged ten lines at a time, preferring `,` then tab then `;`.
fn consistent_delimiter(sample: &str) -> Option<char> {
    let lines: Vec<&str> = sample.split('\n').filter(|l| !l.is_empty()).collect();
    let chunk = lines.len().min(10);
    // Per candidate: (occurrences on a line, lines with that many), in first-seen order.
    let mut seen: [Vec<(usize, i64)>; 4] = Default::default();
    let mut found: Vec<char> = Vec::new();
    let (mut start, mut iteration) = (0, 0);
    while start < lines.len() {
        iteration += 1;
        for line in &lines[start..(start + chunk).min(lines.len())] {
            for (table, &d) in seen.iter_mut().zip(&DELIMITERS) {
                let n = line.matches(d).count();
                match table.iter_mut().find(|(count, _)| *count == n) {
                    Some(entry) => entry.1 += 1,
                    None => table.push((n, 1)),
                }
            }
        }
        // The mode, discounted by every line that disagrees with it.
        let modes: Vec<(char, usize, i64)> = seen
            .iter()
            .zip(DELIMITERS)
            .filter_map(|(table, d)| {
                let mode =
                    table
                        .iter()
                        .fold(None, |best: Option<&(usize, i64)>, e| match best {
                            Some(b) if b.1 >= e.1 => Some(b),
                            _ => Some(e),
                        })?;
                let others: i64 = table.iter().map(|e| e.1).sum::<i64>() - mode.1;
                Some((d, mode.0, mode.1 - others))
            })
            .collect();
        let total = (chunk * iteration).min(lines.len()) as f64;
        // Python steps 1.0 down by 0.01 to 0.9; the same float steps give the same cut-offs.
        let mut consistency = 1.0;
        while found.is_empty() && consistency >= 0.9 {
            for &(d, count, lines_agreeing) in &modes {
                if count > 0 && lines_agreeing > 0 && lines_agreeing as f64 / total >= consistency {
                    found.push(d);
                }
            }
            consistency -= 0.01;
        }
        if let [d] = found[..] {
            return Some(d);
        }
        start += chunk;
    }
    [',', '\t', ';', '|']
        .into_iter()
        .find(|d| found.contains(d))
}

/// Python's `csv.field_size_limit()` default, in characters.
const FIELD_LIMIT: usize = 131_072;

/// Splits delimited text the way Python's csv.reader does with its default dialect:
/// `"` quotes, `""` is a literal quote, quoted fields may hold delimiters and newlines,
/// a blank line is an empty row and stray quotes are kept as text. Returns None where
/// Python raises: a field longer than its field size limit (an unclosed quote, say).
fn csv_rows(text: &str, delimiter: char, limit: usize) -> Option<(Vec<Vec<String>>, bool)> {
    #[derive(PartialEq)]
    enum State {
        Start,
        Field,
        Quoted,
        QuoteInQuoted,
    }
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut field_len = 0;
    let mut state = State::Start;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let newline = c == '\n' || c == '\r';
        if newline && state != State::Quoted {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            if state != State::Start || !row.is_empty() {
                row.push(std::mem::take(&mut field));
            }
            if rows.len() >= limit {
                return Some((rows, true));
            }
            rows.push(std::mem::take(&mut row));
            (state, field_len) = (State::Start, 0);
            continue;
        }
        let add = match state {
            State::Start | State::Field if c == delimiter => {
                row.push(std::mem::take(&mut field));
                (state, field_len) = (State::Start, 0);
                None
            }
            State::Start if c == '"' => {
                state = State::Quoted;
                None
            }
            State::Start | State::Field => {
                state = State::Field;
                Some(c)
            }
            State::Quoted if c == '"' => {
                state = State::QuoteInQuoted;
                None
            }
            State::Quoted => Some(c),
            State::QuoteInQuoted if c == '"' => {
                state = State::Quoted;
                Some('"')
            }
            State::QuoteInQuoted if c == delimiter => {
                row.push(std::mem::take(&mut field));
                (state, field_len) = (State::Start, 0);
                None
            }
            State::QuoteInQuoted => {
                state = State::Field;
                Some(c)
            }
        };
        if let Some(c) = add {
            if field_len >= FIELD_LIMIT {
                return None;
            }
            field.push(c);
            field_len += 1;
        }
    }
    if state != State::Start || !row.is_empty() {
        row.push(field);
        if rows.len() >= limit {
            return Some((rows, true));
        }
        rows.push(row);
    }
    Some((rows, false))
}

/// Python's `str.splitlines()`: every Unicode line boundary, `\r\n` as one.
fn split_lines(text: &str) -> Vec<&str> {
    let is_break = |c: char| {
        matches!(
            c,
            '\n' | '\r'
                | '\x0b'
                | '\x0c'
                | '\x1c'
                | '\x1d'
                | '\x1e'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        )
    };
    let mut lines = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(is_break) {
        lines.push(&rest[..i]);
        let width = if rest[i..].starts_with("\r\n") {
            2
        } else {
            rest[i..].chars().next().map_or(1, char::len_utf8)
        };
        rest = &rest[i + width..];
    }
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines
}

/// Rows, the delimiter used, and whether the file was truncated.
pub fn parse_delimited(text: &str, ext: &str) -> (Vec<Vec<String>>, char, bool) {
    let end = text.char_indices().nth(8192).map_or(text.len(), |(i, _)| i);
    let delimiter = sniff_delimiter(&text[..end], ext);
    match csv_rows(text, delimiter, CSV_MAX_ROWS) {
        Some((rows, truncated)) => (rows, delimiter, truncated),
        None => {
            // Malformed quoting: fall back to a plain split so something still shows.
            let lines = split_lines(text);
            let rows = lines
                .iter()
                .take(CSV_MAX_ROWS)
                .map(|line| line.split(delimiter).map(str::to_string).collect())
                .collect();
            (rows, delimiter, lines.len() > CSV_MAX_ROWS)
        }
    }
}

/// Padding entities, across the whole file, that alignment may add.
const MAX_ALIGN_PADDING: usize = 8 << 20;

/// Colour every column, optionally padding fields so columns line up.
///
/// Kept as monospaced text rather than a table: that keeps the file's own shape
/// visible, and makes alignment something the reader can turn on and off.
pub fn render_delimited(
    rows: &[Vec<String>],
    delimiter: char,
    aligned: bool,
    truncated: bool,
) -> String {
    if rows.is_empty() {
        return r#"<div class="csv-info">Empty file.</div>"#.to_string();
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0; columns];
    if aligned {
        for row in rows {
            for (index, field) in row.iter().enumerate() {
                widths[index] = widths[index].max(field.chars().count());
            }
        }
    }
    // One wide field per column pads every other row to it: 1,000 x 1,000 cells from a 5 MB
    // file asks for gigabytes of `&nbsp;`. Past the budget, show the file unaligned.
    // shortcut: a fixed budget (about 48 MB of padding); raise it if real files hit it.
    let aligned = aligned && {
        let last = columns.saturating_sub(1);
        // Spend the budget field by field: stops early, and cannot overflow a 32-bit usize.
        let mut budget = MAX_ALIGN_PADDING;
        rows.iter()
            .flat_map(|row| row.iter().enumerate().take(last))
            .all(|(index, field)| {
                budget
                    .checked_sub(widths[index] - field.chars().count())
                    .map(|left| budget = left)
                    .is_some()
            })
    };

    let delim = delimiter.to_string();
    let shown = escape(if delimiter == '\t' { "\\t" } else { &delim });
    let mut out = format!(
        r#"<div class="csv-info">{} rows &times; {columns} columns &middot; delimiter <code>{shown}</code>{}</div><pre class="csv">"#,
        thousands(rows.len()),
        if aligned { " &middot; aligned" } else { "" },
    );
    let sep = format!(r#"<span class="csv-d">{}</span>"#, escape(&delim));
    // Up to CSV_MAX_ROWS rows of fields: build each column's opening tag once, not per cell.
    let opens: Vec<String> = (0..columns)
        .map(|index| format!(r#"<span class="c{}">"#, index % CSV_COLOURS))
        .collect();
    let last = columns.saturating_sub(1);
    for (number, row) in rows.iter().enumerate() {
        out.push_str(if number == 0 {
            r#"<div class="csv-row csv-head">"#
        } else {
            r#"<div class="csv-row">"#
        });
        for (index, field) in row.iter().enumerate() {
            if index > 0 {
                out.push_str(&sep);
            }
            out.push_str(&opens[index]);
            out.push_str(&escape(field));
            if aligned && index < last {
                out.push_str(&"&nbsp;".repeat(widths[index] - field.chars().count()));
            }
            out.push_str("</span>");
        }
        out.push_str("</div>");
    }
    out.push_str("</pre>");
    if truncated {
        out.push_str(&format!(
            r#"<div class="csv-info csv-cut">Showing the first {} rows.</div>"#,
            thousands(CSV_MAX_ROWS)
        ));
    }
    out
}

// --------------------------------------------------------------------------- //
// Text files
// --------------------------------------------------------------------------- //

// shortcut: one fixed cap; the page, not the read, is what breaks on a huge file.
// Make it a preference if people hit it.
pub const MAX_TEXT_BYTES: usize = 5 * 1024 * 1024;

pub fn read_text(path: &Path) -> std::io::Result<String> {
    read_text_capped(path, MAX_TEXT_BYTES)
}

/// Reads at most `max` bytes and decodes them as Windows tools actually write text:
/// UTF-16 with a BOM, UTF-8 (BOM or not), then cp1252, then Latin-1, which never fails.
/// When the file is cut, a character the cut split in two is held back, not mangled.
pub fn read_text_capped(path: &Path, max: usize) -> std::io::Result<String> {
    let mut data = Vec::new();
    File::open(path)?
        .take((max as u64).saturating_add(1))
        .read_to_end(&mut data)?;
    let cut = data.len() > max;
    data.truncate(max);
    Ok(decode_text(&data, cut))
}

fn decode_text(data: &[u8], cut: bool) -> String {
    if let Some(rest) = data.strip_prefix(b"\xff\xfe") {
        // UTF-16 with a BOM: what Windows PowerShell 5 writes for "cmd > out.txt".
        return decode_utf16(rest, u16::from_le_bytes, cut);
    }
    if let Some(rest) = data.strip_prefix(b"\xfe\xff") {
        return decode_utf16(rest, u16::from_be_bytes, cut);
    }
    let utf8 = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    match std::str::from_utf8(utf8) {
        Ok(text) => return text.to_string(),
        // An incomplete last character: the cut split it, so drop it. Uncut, it is not UTF-8.
        Err(e) if cut && e.error_len().is_none() => {
            return String::from_utf8_lossy(&utf8[..e.valid_up_to()]).into_owned();
        }
        Err(_) => {}
    }
    // a lone b"\xef" must still fall to cp1252
    data.iter()
        .map(|&b| cp1252(b))
        .collect::<Option<String>>()
        .unwrap_or_else(|| data.iter().map(|&b| char::from(b)).collect()) // Latin-1
}

fn decode_utf16(data: &[u8], unit: fn([u8; 2]) -> u16, cut: bool) -> String {
    let mut units: Vec<u16> = data.as_chunks::<2>().0.iter().map(|&p| unit(p)).collect();
    let ends_in_high = units.last().is_some_and(|u| (0xD800..0xDC00).contains(u));
    if cut && ends_in_high {
        units.pop(); // a high surrogate whose pair the cut removed
    }
    let mut text: String = char::decode_utf16(units)
        .map(|c| c.unwrap_or('\u{FFFD}'))
        .collect();
    // An odd last byte is one more U+FFFD, unless it is the half of a pair whose high surrogate
    // already gave one (Python replaces the incomplete pair once). When cut it is held back.
    if data.len() % 2 == 1 && !cut && !ends_in_high {
        text.push('\u{FFFD}');
    }
    text
}

/// One byte as Windows-1252, or None for its five unassigned bytes (Python's strict cp1252).
fn cp1252(b: u8) -> Option<char> {
    const HIGH: [u16; 32] = [
        0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
        0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
        0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
    ];
    match b {
        0x80..=0x9F => match HIGH[usize::from(b - 0x80)] {
            0 => None,
            c => char::from_u32(c.into()),
        },
        _ => Some(char::from(b)),
    }
}

/// Says so when `read_text` showed only the start of the file, or "".
pub fn cut_note(path: &Path) -> String {
    cut_note_capped(path, MAX_TEXT_BYTES)
}

fn cut_note_capped(path: &Path, max: usize) -> String {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > max as u64 => format!(
            "first {} of {}",
            human_size(max as u64),
            human_size(meta.len())
        ),
        _ => String::new(),
    }
}

pub fn human_size(n: u64) -> String {
    let mut size = n as f64;
    for unit in ["B", "KB", "MB"] {
        if size < 1024.0 {
            return format!("{size:.0} {unit}");
        }
        size /= 1024.0;
    }
    format!("{size:.0} GB")
}

// --------------------------------------------------------------------------- //
// SVG size
// --------------------------------------------------------------------------- //

// A pattern that fails to compile matches nothing rather than panicking; a test checks they all compile.
static SVG_TAG: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?is)<svg\b[^>]*>").ok());
static SVG_VIEWBOX: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r#"(?i)viewBox\s*=\s*["']\s*[-\d.eE]+[ ,]+[-\d.eE]+[ ,]+(?P<w>[-\d.eE]+)[ ,]+(?P<h>[-\d.eE]+)"#).ok()
});
// The regex engine is linear, so a hostile width="1<spaces>x" cannot stall it.
static SVG_ABSOLUTE_LEN: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*[-\d.eE]+(?:\s*(?:px|pt|pc|cm|mm|in|Q))?\s*$").ok());
static SVG_WIDTH: LazyLock<Option<Regex>> = LazyLock::new(|| attr_regex("width"));
static SVG_HEIGHT: LazyLock<Option<Regex>> = LazyLock::new(|| attr_regex("height"));

fn attr_regex(name: &str) -> Option<Regex> {
    Regex::new(&format!(
        r#"(?is)\b{name}\s*=\s*(?:"(?P<d>[^"]*)"|'(?P<s>[^']*)')"#
    ))
    .ok()
}

/// A length in absolute units (a bare number or px, pt, pc, cm, mm, in, Q).
pub fn is_absolute_length(value: &str) -> bool {
    SVG_ABSOLUTE_LEN
        .as_ref()
        .is_some_and(|re| re.is_match(value))
}

/// Intrinsic size for an SVG, or None to trust what the engine reports.
///
/// An SVG that declares only a viewBox has no intrinsic size, so the engine
/// substitutes a placeholder (a 300x150 box shrunk to the viewBox aspect).
/// Zooming from that placeholder makes "100%" meaningless, so in that case the
/// viewBox extent is the honest natural size. When width/height *are* given in
/// absolute units, the author stated a size and the engine already has it right.
pub fn svg_dimensions(path: &Path) -> Option<(f64, f64)> {
    let mut head = Vec::new();
    File::open(path)
        .ok()?
        .take(16384)
        .read_to_end(&mut head)
        .ok()?;
    let head = String::from_utf8_lossy(&head);
    let attrs = SVG_TAG.as_ref()?.find(&head)?.as_str();

    let sized = |re: &Option<Regex>| {
        re.as_ref()
            .and_then(|re| re.captures(attrs))
            .and_then(|c| c.name("d").or(c.name("s")))
            .is_some_and(|v| is_absolute_length(v.as_str()))
    };
    if sized(&SVG_WIDTH) && sized(&SVG_HEIGHT) {
        return None;
    }
    let m = SVG_VIEWBOX.as_ref()?.captures(attrs)?;
    let w: f64 = m.name("w")?.as_str().parse().ok()?;
    let h: f64 = m.name("h")?.as_str().parse().ok()?;
    (w > 0.0 && h > 0.0).then_some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Instant;

    fn rows(lines: &[&[&str]]) -> Vec<Vec<String>> {
        lines
            .iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    #[test]
    fn classify_by_extension() {
        assert_eq!(classify(Path::new("a/README.MD")), Kind::Markdown);
        assert_eq!(classify(Path::new("x.mmd")), Kind::Mermaid);
        assert_eq!(classify(Path::new("x.svgz")), Kind::Svg);
        assert_eq!(classify(Path::new("x.avif")), Kind::Raster);
        assert_eq!(classify(Path::new("x.tab")), Kind::Csv);
        assert_eq!(classify(Path::new("Makefile")), Kind::Text);
        // Python: a trailing dot or a leading one is not a suffix
        assert_eq!(suffix(Path::new("notes.")), "");
        assert_eq!(suffix(Path::new(".md")), "");
        assert_eq!(suffix(Path::new("a.tar.GZ")), ".gz");
    }

    // --- delimited files (test_core.py) ---

    #[test]
    fn quoted_fields_keep_delimiters_and_newlines() {
        let (got, delimiter, truncated) =
            parse_delimited("a,b\n\"x, y\",\"line1\nline2\"\n", ".csv");
        assert_eq!(delimiter, ',');
        assert_eq!(got, rows(&[&["a", "b"], &["x, y", "line1\nline2"]]));
        assert!(!truncated);
    }

    #[test]
    fn csv_reader_matches_python_on_quotes_and_blank_lines() {
        let (got, _) = csv_rows("a,\"b\"\"c\"\r\n\nx\"y,\"q\"z\n,", ',', 100).unwrap();
        assert_eq!(
            got,
            rows(&[&["a", "b\"c"], &[], &["x\"y", "qz"], &["", ""]])
        );
    }

    #[test]
    fn semicolons_are_sniffed_and_tsv_is_always_tab() {
        assert_eq!(parse_delimited("a;b;c\n1;2;3\n", ".csv").1, ';');
        assert_eq!(parse_delimited("a,b\tc\n", ".tsv").1, '\t');
        assert_eq!(parse_delimited("one\ntwo\n", ".csv").1, ',');
        assert_eq!(parse_delimited("a|b|c\n1|2\n", ".csv").1, '|');
    }

    // csv.Sniffer looks at quoted fields first: here `;` borders them, though `,` is as regular.
    #[test]
    fn delimiter_next_to_quoted_fields_wins() {
        let (got, delimiter, _) = parse_delimited("a,b;\"c\"\nd,e;\"f\"\n", ".csv");
        assert_eq!(delimiter, ';');
        assert_eq!(got, rows(&[&["a,b", "c"], &["d,e", "f"]]));
    }

    // A combining mark is not a word character to Python, so `ͅ"x"ͅ` counts as a quoted
    // field (with no allowed delimiter), and the line counts then pick `,`.
    #[test]
    fn word_characters_follow_python() {
        assert!(is_word('é') && is_word('_') && is_word('\u{2160}'));
        assert!(!is_word('\u{345}') && !is_word('\u{947}') && !is_word('\u{24b6}'));
        let sample = "a\u{345}\"x\"\u{345}b,c\n\"p\";q,r\n";
        assert_eq!(sniff_delimiter(sample, ".csv"), ',');
    }

    // Without quotes the Sniffer judges ten lines at a time and accepts 90% agreement, stepping
    // down from 1.0 in floats: 18 of 20 lines misses the cut, 28 of 30 passes (Python 3.12).
    #[test]
    fn nearly_regular_lines_are_judged_in_chunks() {
        let text = |n| format!("a;b;c\n{}", "x,y\n".repeat(n));
        assert_eq!(sniff_delimiter(&text(19), ".csv"), ';'); // first-line fallback
        assert_eq!(sniff_delimiter(&text(29), ".csv"), ',');
    }

    // Past its field size limit Python's reader raises (an unclosed quote swallowing the
    // file, say), and parse_delimited falls back to a plain split.
    #[test]
    fn oversized_field_falls_back_to_a_plain_split() {
        let fits = format!("a,\"b\n{}\"\n", "c".repeat(FIELD_LIMIT - 2));
        let (got, _, _) = parse_delimited(&fits, ".csv");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0][1].chars().count(), FIELD_LIMIT);

        let over = format!("a,\"b\n{}\"\n", "c".repeat(FIELD_LIMIT - 1));
        let (got, delimiter, truncated) = parse_delimited(&over, ".csv");
        assert_eq!((delimiter, truncated), (',', false));
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], ["a", "\"b"]);
        assert!(got[1][0].starts_with("ccc"));
    }

    #[test]
    fn lines_split_like_python_splitlines() {
        assert_eq!(
            split_lines("a\x0bb\u{85}c\u{2028}d\r\n\re"),
            ["a", "b", "c", "d", "", "e"]
        );
        assert!(split_lines("").is_empty());
    }

    #[test]
    fn alignment_past_the_padding_budget_shows_the_file_unaligned() {
        let wide = "x".repeat(MAX_ALIGN_PADDING + 1);
        let html = render_delimited(&rows(&[&[&wide, "a"], &["", "b"]]), ',', true, false);
        assert!(!html.contains("aligned") && !html.contains("&nbsp;"));
        let html = render_delimited(&rows(&[&["xx", "a"], &["", "b"]]), ',', true, false);
        assert!(html.contains("&middot; aligned") && html.contains("&nbsp;&nbsp;"));
    }

    #[test]
    fn long_files_are_cut_and_say_so() {
        let text = format!("n\n{}", "1\n".repeat(CSV_MAX_ROWS + 10));
        let (got, _, truncated) = parse_delimited(&text, ".csv");
        assert!(got.len() == CSV_MAX_ROWS && truncated);
        assert!(render_delimited(&got, ',', false, true).contains("Showing the first 5,000 rows."));
    }

    #[test]
    fn aligned_columns_are_padded_to_the_widest_field() {
        let html = render_delimited(&rows(&[&["a", "b"], &["long", "c"]]), ',', true, false);
        assert!(html.contains(r#"<span class="c0">a&nbsp;&nbsp;&nbsp;</span>"#));
        assert!(html.contains(
            "2 rows &times; 2 columns &middot; delimiter <code>,</code> &middot; aligned"
        ));
        assert_eq!(
            render_delimited(&[], ',', false, false),
            r#"<div class="csv-info">Empty file.</div>"#
        );
    }

    // --- read_text (test_read_text.py) ---

    fn write(dir: &tempfile::TempDir, name: &str, data: &[u8]) -> std::path::PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, data).unwrap();
        path
    }

    fn utf16(text: &str, le: bool) -> Vec<u8> {
        let mut out = if le {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for u in text.encode_utf16() {
            out.extend(if le { u.to_le_bytes() } else { u.to_be_bytes() });
        }
        out
    }

    #[test]
    fn unicode_files_decode() {
        let dir = tempfile::tempdir().unwrap();
        let text = "Héllo wörld\nline 2\n";
        let mut bom = b"\xef\xbb\xbf".to_vec();
        bom.extend(text.as_bytes());
        for data in [
            utf16(text, true),
            utf16(text, false),
            bom,
            text.as_bytes().to_vec(),
        ] {
            assert_eq!(read_text(&write(&dir, "out.log", &data)).unwrap(), text);
        }
    }

    #[test]
    fn legacy_windows_text_still_decodes() {
        let dir = tempfile::tempdir().unwrap();
        // "café – 5 €" in cp1252
        let path = write(&dir, "old.txt", b"caf\xe9 \x96 5 \x80");
        assert_eq!(read_text(&path).unwrap(), "café – 5 €");
        // an unassigned cp1252 byte sends the whole file to Latin-1
        assert_eq!(
            read_text(&write(&dir, "l1.txt", b"\x80\x81")).unwrap(),
            "\u{80}\u{81}"
        );
    }

    #[test]
    fn huge_file_is_cut_at_a_character_not_read_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "big.log", "é\n".repeat(100).as_bytes()); // 300 bytes, 2-byte letters
        let text = read_text_capped(&path, 101).unwrap();
        assert!(!text.is_empty() && text.len() <= 101);
        assert!(text.chars().all(|c| c == 'é' || c == '\n')); // no split character, no mojibake
    }

    #[test]
    fn cut_inside_a_character_keeps_the_encoding() {
        let dir = tempfile::tempdir().unwrap();
        let one_line = write(&dir, "min.json", "é".repeat(100).as_bytes()); // 101 cuts a letter in two
        assert_eq!(read_text_capped(&one_line, 101).unwrap(), "é".repeat(50));
        let wide = write(&dir, "out.log", &utf16(&"A\n".repeat(200), true));
        assert_eq!(read_text_capped(&wide, 99).unwrap(), "A\n".repeat(24)); // the odd byte held back
    }

    // Python's utf-16 "replace" on broken tails (bytes after the BOM).
    #[test]
    fn broken_utf16_tails_are_replaced_like_python() {
        let dir = tempfile::tempdir().unwrap();
        for (tail, want) in [
            (&b"\x00\xd8\x00"[..], "\u{fffd}"), // high surrogate + half its pair: once
            (b"\x00\xd8", "\u{fffd}"),
            (b"\x00\xd8A\x00", "\u{fffd}A"),
            (b"\x00\xd8\x00\xd8\x00", "\u{fffd}\u{fffd}"),
            (b"\x00\xdc\x00", "\u{fffd}\u{fffd}"), // a low surrogate, then an odd byte
            (b"A\x00\x00", "A\u{fffd}"),
        ] {
            let path = write(&dir, "w.txt", &[b"\xff\xfe", tail].concat());
            assert_eq!(read_text(&path).unwrap(), want, "{tail:?}");
        }
    }

    #[test]
    fn an_unbounded_cap_reads_the_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "x.txt", b"abc");
        assert_eq!(read_text_capped(&path, usize::MAX).unwrap(), "abc");
    }

    #[test]
    fn partial_bom_file_is_shown_not_swallowed() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_text(&write(&dir, "x.txt", b"\xef")).unwrap(), "ï");
        assert_eq!(read_text(&write(&dir, "x.txt", b"\xef\xbb")).unwrap(), "ï»");
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(5 * 1024 * 1024), "5 MB");
        assert_eq!(human_size(3 << 30), "3 GB");
        // Python's format rounds half to even
        assert_eq!(human_size(2560), "2 KB");
        assert_eq!(human_size(3584), "4 KB");
    }

    #[test]
    fn cut_file_says_how_much_is_shown() {
        let dir = tempfile::tempdir().unwrap();
        let big = write(&dir, "big.txt", "x\n".repeat(500).as_bytes());
        assert_eq!(cut_note_capped(&big, 100), "first 100 B of 1000 B");
        let small = write(&dir, "small.txt", b"x\n");
        assert_eq!(cut_note_capped(&small, 100), "");
        assert_eq!(cut_note(&small), "");
    }

    // --- SVG size (test_core.py) ---

    #[test]
    fn svg_without_size_uses_its_viewbox() {
        let dir = tempfile::tempdir().unwrap();
        let svg = write(
            &dir,
            "s.svg",
            br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"></svg>"#,
        );
        assert_eq!(svg_dimensions(&svg), Some((640.0, 480.0)));
        let svg = write(
            &dir,
            "s.svg",
            br#"<svg width="10px" height='20' viewBox="0 0 640 480"></svg>"#,
        );
        assert_eq!(svg_dimensions(&svg), None);
        let svg = write(
            &dir,
            "s.svg",
            br#"<svg width="100%" height="20" viewBox="0,0,64,48"></svg>"#,
        );
        assert_eq!(svg_dimensions(&svg), Some((64.0, 48.0)));
    }

    #[test]
    fn every_pattern_compiles() {
        for re in [
            &SVG_TAG,
            &SVG_VIEWBOX,
            &SVG_ABSOLUTE_LEN,
            &SVG_WIDTH,
            &SVG_HEIGHT,
            &WORD_CHAR,
        ] {
            assert!(re.is_some());
        }
    }

    #[test]
    fn svg_absolute_length() {
        let spaced_x = format!("1{}x", " ".repeat(50));
        for (value, absolute) in [
            ("640", true),
            (" 640px ", true),
            ("12.5 cm", true),
            ("1e2PT", true),
            ("-3 in", true),
            ("100%", false),
            ("auto", false),
            ("", false),
            ("640 px x", false),
            (spaced_x.as_str(), false),
        ] {
            assert_eq!(is_absolute_length(value), absolute, "{value:?}");
        }
    }

    #[test]
    fn svg_length_check_is_linear_on_hostile_input() {
        is_absolute_length(""); // compile the pattern before the clock starts
        let start = Instant::now();
        assert!(!is_absolute_length(&format!("1{}x", " ".repeat(50_000))));
        assert!(start.elapsed().as_secs_f64() < 1.0);
    }

    #[test]
    fn svg_size_reads_only_the_head() {
        let dir = tempfile::tempdir().unwrap();
        // the viewBox sits past the 16 KB head, so reading the whole file would find it
        let late = format!("<svg {} viewBox=\"0 0 640 480\"></svg>", " ".repeat(20_000));
        assert_eq!(
            svg_dimensions(&write(&dir, "late.svg", late.as_bytes())),
            None
        );
        let early = format!("<svg viewBox=\"0 0 640 480\">{}</svg>", " ".repeat(100_000));
        assert_eq!(
            svg_dimensions(&write(&dir, "big.svg", early.as_bytes())),
            Some((640.0, 480.0))
        );
    }
}
