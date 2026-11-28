use crate::html;
use pulldown_cmark::{html as cmark_html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use regex::{Captures, Regex};
use std::collections::HashSet;
use std::sync::LazyLock;
use syntect::highlighting::ThemeSet;
use syntect::html::highlighted_html_for_string;
use syntect::parsing::SyntaxSet;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

fn is_tag_boundary(c: char) -> bool {
    " \t\n\x0C\r/>".contains(c)
}

#[allow(clippy::panic)]
static RAW_META: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<meta").unwrap_or_else(|e| panic!("invalid regex: {}", e)));

#[allow(clippy::panic)]
static NAMED_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<(img|form|embed|iframe|object)")
        .unwrap_or_else(|e| panic!("invalid regex: {}", e))
});

#[allow(clippy::panic)]
static TASK_ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<li>(\s*)\[([ xX])\]\s+").unwrap_or_else(|e| panic!("invalid regex: {}", e))
});

#[allow(clippy::panic)]
static SCRIPT_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?si)<script[^>]*>.*?</script>").unwrap_or_else(|e| panic!("invalid regex: {}", e))
});

pub fn render_markdown(text: &str) -> (String, String) {
    let options = Options::all();

    // First pass: collect explicit heading IDs so generated ones never collide.
    let reserved_ids: HashSet<String> = Parser::new_ext(text, options)
        .filter_map(|ev| match ev {
            Event::Start(Tag::Heading { id, .. }) => id.map(|s| s.to_string()),
            _ => None,
        })
        .collect();

    let parser = Parser::new_ext(text, options);

    let mut events = Vec::new();
    let mut toc_entries = Vec::new();
    let mut current_heading_level = 0;
    let mut current_heading_id = String::new();
    let mut current_heading_text = String::new();
    let mut in_heading = false;
    let mut heading_counter = 0;

    let mut iter = parser.into_iter();
    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
                ..
            }) => {
                let lvl = level as u32;
                let resolved_id = if let Some(explicit) = id.as_ref() {
                    explicit.to_string()
                } else {
                    // Generate an ID that does not collide with any explicit one.
                    loop {
                        heading_counter += 1;
                        let candidate = format!("heading-{}", heading_counter);
                        if !reserved_ids.contains(&candidate) {
                            break candidate;
                        }
                    }
                };

                if (2..=4).contains(&lvl) {
                    current_heading_level = lvl;
                    current_heading_id = resolved_id.clone();
                    current_heading_text = String::new();
                    in_heading = true;
                }
                events.push(Event::Start(Tag::Heading {
                    level,
                    id: Some(resolved_id.into()),
                    classes,
                    attrs,
                }));
            }
            Event::End(TagEnd::Heading(level)) => {
                if in_heading {
                    toc_entries.push((
                        current_heading_level,
                        current_heading_id.clone(),
                        current_heading_text.clone(),
                    ));
                    in_heading = false;
                }
                events.push(Event::End(TagEnd::Heading(level)));
            }
            Event::Text(ref t) if in_heading => {
                current_heading_text.push_str(t);
                events.push(event);
            }
            Event::Code(ref t) if in_heading => {
                current_heading_text.push_str(t);
                events.push(event);
            }
            Event::InlineMath(ref t) | Event::DisplayMath(ref t) if in_heading => {
                current_heading_text.push_str(t);
                events.push(event);
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang))) => {
                let info = lang.as_ref();
                let lang_str = info.split_whitespace().next().unwrap_or("");
                let mut code = String::new();
                for next_event in iter.by_ref() {
                    match next_event {
                        Event::Text(t) => code.push_str(&t),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                if lang_str == "mermaid" {
                    events.push(Event::Html(mermaid_block(&code).into()));
                } else {
                    events.push(Event::Html(highlight(lang_str, &code).into()));
                }
            }
            _ => {
                events.push(event);
            }
        }
    }

    let mut body = String::new();
    cmark_html::push_html(&mut body, events.into_iter());

    let toc = build_toc(&toc_entries);
    let body = post_process(&body);

    (body, toc)
}

fn highlight(lang: &str, code: &str) -> String {
    let syntax = SYNTAX_SET
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());
    let theme = &THEME_SET.themes["base16-ocean.dark"];
    highlighted_html_for_string(code, &SYNTAX_SET, syntax, theme).unwrap_or_else(|_| {
        format!(
            "<pre class=\"codehilite\"><code>{}</code></pre>",
            html::escape(code)
        )
    })
}

fn mermaid_block(source: &str) -> String {
    let escaped = html::escape(source.trim());
    format!(
        r#"<div class="mermaid-figure"><pre class="mermaid" data-src="{}">{}</pre></div>"#,
        escaped, escaped
    )
}

fn build_toc(entries: &[(u32, String, String)]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out = String::from("<div class=\"toc\">\n<ul>\n");
    let mut current_level = 2;

    for (level, id, text) in entries {
        let level = *level;
        while current_level < level {
            out.push_str("<ul>\n");
            current_level += 1;
        }
        while current_level > level {
            out.push_str("</ul>\n");
            current_level -= 1;
        }
        out.push_str(&format!(
            "<li><a href=\"#{}\">{}</a></li>\n",
            html::escape(id),
            html::escape(text)
        ));
    }
    while current_level > 2 {
        out.push_str("</ul>\n");
        current_level -= 1;
    }
    out.push_str("</ul>\n</div>");
    out
}

fn post_process(html: &str) -> String {
    let html = _task_lists(html);
    let html = SCRIPT_TAG.replace_all(&html, "").into_owned();

    // RAW_META lookahead replacement logic
    let mut out = String::new();
    let mut last_pos = 0;
    for caps in RAW_META.captures_iter(&html) {
        if let Some(m) = caps.get(0) {
            out.push_str(&html[last_pos..m.start()]);

            let next_char = html[m.end()..].chars().next().unwrap_or(' ');
            if is_tag_boundary(next_char) {
                out.push_str("<meta http-equiv=\"\"");
            } else {
                out.push_str(m.as_str());
            }
            last_pos = m.end();
        }
    }
    out.push_str(&html[last_pos..]);

    _unname_tags(&out)
}

fn _task_lists(html: &str) -> String {
    TASK_ITEM
        .replace_all(html, |caps: &Captures| {
            let indent = &caps[1];
            let checked = if caps[2].to_lowercase() == "x" {
                " checked"
            } else {
                ""
            };
            format!(
                "<li class=\"task-item\">{}<input type=\"checkbox\" disabled{}> ",
                indent, checked
            )
        })
        .into_owned()
}

/// Rename `name` (and `id` for `<object>`) attributes to `data-mdv-*`
/// on security-sensitive tags, preventing DOM clobbering.
/// Uses byte offsets throughout (O(N)); skips matches inside already-consumed regions.
fn _unname_tags(html_text: &str) -> String {
    let bytes = html_text.as_bytes();
    let len = bytes.len();
    let mut out = String::new();
    // Byte offset up to which input has been consumed.
    let mut pos: usize = 0;

    for caps in NAMED_TAG.captures_iter(html_text) {
        let Some(m) = caps.get(0) else { continue };

        // Skip matches that start inside already-consumed text (nested tags in attributes).
        if m.start() < pos {
            continue;
        }

        let m_end = m.end(); // byte offset just past the tag name

        // Boundary check: the char right after the tag name must be a tag boundary.
        let is_boundary = if m_end < len {
            // Tag names are ASCII so m_end is on a char boundary.
            let next_char = html_text[m_end..].chars().next().unwrap_or(' ');
            is_tag_boundary(next_char)
        } else {
            true
        };
        if !is_boundary {
            continue;
        }

        // Copy text from previous position up to (and including) the tag name.
        out.push_str(&html_text[pos..m_end]);

        let tag_name = caps
            .get(1)
            .map(|v| v.as_str().to_lowercase())
            .unwrap_or_default();
        let renamed: &[&str] = if tag_name == "object" {
            &["name", "id"]
        } else {
            &["name"]
        };

        let mut i = m_end;

        // Walk the attributes until we hit an unquoted `>` or end of input.
        // Quoted attribute values are consumed whole so `>` inside them is skipped.
        while i < len && bytes[i] != b'>' {
            let c = bytes[i];
            if b" \t\n\x0C\r/".contains(&c) {
                out.push(c as char);
                i += 1;
                continue;
            }

            // Attribute name (ASCII).
            let mut j = i + 1;
            while j < len && !b" \t\n\x0C\r/>=\"".contains(&bytes[j]) {
                j += 1;
            }
            let attr_name = &html_text[i..j];
            if renamed.contains(&attr_name.to_lowercase().as_str()) {
                out.push_str("data-mdv-");
            }
            out.push_str(attr_name);
            i = j;

            // Optional whitespace then `=`.
            let mut k = i;
            while k < len && b" \t\n\x0C\r".contains(&bytes[k]) {
                k += 1;
            }
            if k < len && bytes[k] == b'=' {
                k += 1;
                // Whitespace after `=`.
                while k < len && b" \t\n\x0C\r".contains(&bytes[k]) {
                    k += 1;
                }
                if k < len && (bytes[k] == b'"' || bytes[k] == b'\'') {
                    let quote = bytes[k];
                    k += 1;
                    while k < len && bytes[k] != quote {
                        k += 1;
                    }
                    if k < len {
                        k += 1; // past closing quote
                    }
                } else {
                    // Unquoted value.
                    while k < len && !b" \t\n\x0C\r>".contains(&bytes[k]) {
                        k += 1;
                    }
                }
                out.push_str(&html_text[i..k]);
                i = k;
            }
        }
        // `i` is now on `>` or at end of input; leave `>` for the tail copy.
        pos = i;
    }
    out.push_str(&html_text[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unname_tags() {
        let input = "<img name=\"foo\" src=\"bar.png\"><object id=\"obj\" name=\"objname\">";
        let expected =
            "<img data-mdv-name=\"foo\" src=\"bar.png\"><object data-mdv-id=\"obj\" data-mdv-name=\"objname\">";
        assert_eq!(_unname_tags(input), expected);
    }

    // Finding 4 regression: nested tags in attribute values must not panic or corrupt output.
    #[test]
    fn test_unname_tags_nested() {
        let input = r#"<img alt="<img>" src="x.png">"#;
        let result = _unname_tags(input);
        // The outer <img> is processed; the inner `<img>` inside quotes is data, not a tag.
        assert!(result.contains("src=\"x.png\""));
        // Must not panic and must produce valid output containing the closing `>`.
        assert!(result.ends_with('>'));
    }

    // Finding 1 regression: O(N) instead of O(N^2) -- no explicit timing, but the old code
    // used chars().count() per match; this exercises a long input to verify correctness.
    #[test]
    fn test_unname_tags_many_matches() {
        let input: String = (0..100)
            .map(|i| format!("<img name=\"n{}\" src=\"s{}.png\">", i, i))
            .collect();
        let result = _unname_tags(&input);
        assert_eq!(
            result.matches("data-mdv-name").count(),
            100,
            "all 100 name attrs should be renamed"
        );
    }

    // Finding 3 regression: multiline script tags must be removed.
    #[test]
    fn test_script_tag_multiline() {
        let input =
            "<p>hello</p><script type=\"text/javascript\">\nalert('xss');\n</script><p>world</p>";
        let result = post_process(input);
        assert!(
            !result.contains("<script"),
            "multiline script tag should be removed"
        );
        assert!(result.contains("<p>hello</p>"));
        assert!(result.contains("<p>world</p>"));
    }

    // Finding 5 regression: generated IDs must not collide with explicit ones.
    #[test]
    fn test_heading_ids_no_collision() {
        // The first heading has an explicit ID "heading-1"; the second has no explicit ID.
        // The generated ID for the second must skip "heading-1".
        let md = "## First {#heading-1}\n\n## Second\n";
        let (body, _toc) = render_markdown(md);
        assert!(body.contains("id=\"heading-1\""), "explicit ID preserved");
        // The generated ID must not also be "heading-1".
        let count = body.matches("id=\"heading-1\"").count();
        assert_eq!(count, 1, "heading-1 must appear exactly once");
        assert!(
            body.contains("id=\"heading-2\""),
            "generated ID should be heading-2 (skipping heading-1)"
        );
    }

    // Finding 2 regression: explicit heading IDs outside TOC levels are preserved.
    #[test]
    fn test_explicit_id_outside_toc_levels() {
        let md = "# Title {#my-title}\n\n## Section\n";
        let (body, _toc) = render_markdown(md);
        assert!(
            body.contains("id=\"my-title\""),
            "explicit ID on h1 (outside TOC levels 2-4) must be preserved"
        );
    }

    #[test]
    fn test_task_lists() {
        let input = "<li> [ ] task</li><li> [x] done</li>";
        let expected = "<li class=\"task-item\"> <input type=\"checkbox\" disabled> task</li><li class=\"task-item\"> <input type=\"checkbox\" disabled checked> done</li>";
        assert_eq!(_task_lists(input), expected);
    }

    #[test]
    fn test_render_markdown_basic() {
        let (body, _) = render_markdown("# Hello\n\nworld");
        assert!(body.contains("<h1") && body.contains("Hello"));
        assert!(body.contains("<p>world</p>") || body.contains("<p>\nworld\n</p>"));
    }

    #[test]
    fn test_mermaid() {
        let (body, _) = render_markdown("```mermaid\ngraph TD; A-->B;\n```");
        assert!(body.contains("class=\"mermaid\""));
        assert!(body.contains("graph TD; A--&gt;B;"));
    }
}
