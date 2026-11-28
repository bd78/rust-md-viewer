//! Standalone Mermaid diagram file rendering.

use crate::html;

/// Renders raw Mermaid diagram file content into an HTML figure element, matching
/// how fenced mermaid blocks are rendered in Markdown documents.
pub fn render_mermaid(source: &str) -> String {
    let escaped = html::escape(source.trim());
    format!(
        r#"<div class="mermaid-figure"><pre class="mermaid" data-src="{}">{}</pre></div>"#,
        escaped, escaped
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_mermaid() {
        let input = "graph TD;\n    A --> B;";
        let html = render_mermaid(input);
        assert!(html.contains("class=\"mermaid\""));
        assert!(html.contains("graph TD;"));
        assert!(html.contains("A --&gt; B;"));
    }

    #[test]
    fn test_render_mermaid_empty() {
        let input = "   ";
        let html = render_mermaid(input);
        assert!(html.contains("class=\"mermaid\""));
    }
}
