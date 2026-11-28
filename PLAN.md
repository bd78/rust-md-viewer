# Plan

How the port proceeds. Each step is one small pull request, merged before the next starts (house rule 18).
[PARITY.md](PARITY.md) tracks what is done.

1. Crate, `config` (file types, data folder, preferences, style choice) and the window-free half of `documents`
   (file kinds, delimited text, text decoding, SVG size).
2. `links`: a link in a document to a local path, network paths, handing a path to the operating system.
3. `markdown_render`: Markdown to HTML, mermaid blocks, syntax highlighting.
4. `mermaid_assets`: the pinned, checksum-verified download and the offline copy.
5. `styles` and `viewer_js`: the page's CSS and script, ported from py-md-view.
6. `page` and the parsed-document cache: whole HTML pages for every file kind.
7. `trees`: the sidebar's open-documents, folder and file-system trees.
8. UI stack: options and trade-offs put to the person; the choice goes in `AGENTS.md` (its own pull request).
9. The window: tabs, toolbar, sidebar, find, zoom and shortcuts, with the page talking to the app.
10. Single instance: a second launch hands its files to the running window.
11. File watching and refresh.
12. Command line and Windows file associations (`--register`).
13. Windows release: `cargo build --release` produces a self-contained `md-view.exe` (static C runtime, icon and
    version resource), checked on a clean Windows machine. CI that builds it on every tag is a separate pull request
    (house rule 12); publishing a release asks the person first (house rule 10).
