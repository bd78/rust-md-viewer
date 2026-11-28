use crate::html;
use pulldown_cmark::{html as cmark_html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use regex::{Captures, Regex};
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
    Regex::new(r"(?i)<script[^>]*>.*?</script>").unwrap_or_else(|e| panic!("invalid regex: {}", e))
});

pub fn render_markdown(text: &str) -> (String, String) {
    let options = Options::all();
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
                if (2..=4).contains(&lvl) {
                    current_heading_level = lvl;
                    current_heading_id = id.as_ref().map(|s| s.to_string()).unwrap_or_else(|| {
                        heading_counter += 1;
                        format!("heading-{}", heading_counter)
                    });
                    current_heading_text = String::new();
                    in_heading = true;
                }
                events.push(Event::Start(Tag::Heading {
                    level,
                    id: if current_heading_id.is_empty() {
                        None
                    } else {
                        Some(current_heading_id.clone().into())
                    },
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
                // Finding 1: Collect inline code
                current_heading_text.push_str(t);
                events.push(event);
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang))) => {
                let lang_str = lang.as_ref();
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
            html::escape(id), // Finding 2: Sanitization
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
    let html = SCRIPT_TAG.replace_all(&html, "").into_owned(); // Finding 4: Remove scripts

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

fn _unname_tags(html_text: &str) -> String {
    let chars: Vec<char> = html_text.chars().collect();
    let mut out = String::new();
    let mut pos = 0;
    let n = chars.len();

    // Re-check lookahead logic: <tag followed by a boundary
    for caps in NAMED_TAG.captures_iter(html_text) {
        let Some(m) = caps.get(0) else { continue };

        let end_char = html_text[..m.end()].chars().count();

        // Check boundary
        let is_boundary = if end_char < n {
            is_tag_boundary(chars[end_char])
        } else {
            true
        };
        if !is_boundary {
            continue;
        }

        out.push_str(&chars[pos..end_char].iter().collect::<String>());

        let tag_name = caps
            .get(1)
            .map(|v| v.as_str().to_lowercase())
            .unwrap_or_default();
        let renamed = if tag_name == "object" {
            vec!["name", "id"]
        } else {
            vec!["name"]
        };

        let mut i = end_char;

        while i < n && chars[i] != '>' {
            let c = chars[i];
            if " \t\n\x0C\r/".contains(c) {
                out.push(c);
                i += 1;
                continue;
            }

            let mut j = i + 1;
            while j < n && !" \t\n\x0C\r/>=".contains(chars[j]) {
                j += 1;
            }
            let attr_name = chars[i..j].iter().collect::<String>();
            if renamed.contains(&attr_name.to_lowercase().as_str()) {
                out.push_str("data-mdv-");
                out.push_str(&attr_name);
            } else {
                out.push_str(&attr_name);
            }
            i = j;

            let mut k = i;
            while k < n && " \t\n\x0C\r".contains(chars[k]) {
                k += 1;
            }
            if k < n && chars[k] == '=' {
                k += 1;
                while k < n && " \t\n\x0C\r".contains(chars[k]) {
                    k += 1;
                }
                if k < n && ("\"'".contains(chars[k])) {
                    let quote = chars[k];
                    k += 1;
                    let mut found = false;
                    while k < n {
                        if chars[k] == quote {
                            k += 1;
                            found = true;
                            break;
                        }
                        k += 1;
                    }
                    if !found {
                        k = n;
                    }
                } else {
                    while k < n && !" \t\n\x0C\r>".contains(chars[k]) {
                        k += 1;
                    }
                }
                out.push_str(&chars[i..k].iter().collect::<String>());
                i = k;
            }
        }
        pos = i;
    }
    out.push_str(&chars[pos..].iter().collect::<String>());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unname_tags() {
        let input = "<img name=\"foo\" src=\"bar.png\"><object id=\"obj\" name=\"objname\">";
        let expected = "<img data-mdv-name=\"foo\" src=\"bar.png\"><object data-mdv-id=\"obj\" data-mdv-name=\"objname\">";
        assert_eq!(_unname_tags(input), expected);
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
        println!("BODY: {}", body);
        assert!(body.contains("<h1>Hello</h1>") || body.contains("<h1>\nHello\n</h1>"));
        assert!(body.contains("<p>world</p>") || body.contains("<p>\nworld\n</p>"));
    }

    #[test]
    fn test_mermaid() {
        let (body, _) = render_markdown("```mermaid\ngraph TD; A-->B;\n```");
        println!("BODY: {}", body);
        assert!(body.contains("class=\"mermaid\""));
        assert!(body.contains("graph TD; A--&gt;B;"));
    }
}
