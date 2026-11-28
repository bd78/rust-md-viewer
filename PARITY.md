# Parity with py-md-view

One line per feature in py-md-view's [README](https://github.com/bd78/py-md-view/blob/main/README.md). States:
not started, partial, done.

| Feature | State | Pull request | Notes |
|---|---|---|---|
| Markdown: tables, footnotes, task lists, admonitions, definition lists, highlighting | not started | | |
| Markdown: ` ```mermaid ` blocks as diagrams | not started | | |
| Mermaid files (`.mmd`, `.mermaid`) | not started | | |
| Delimited files: colour per column, sniffed delimiter, quoted fields, 5,000-row cut | partial | #4 | parsing and HTML, matching Python's `csv` Sniffer and reader; no window yet. Differs on purpose: alignment that would add more than 8 Mi padding entities shows the file unaligned |
| Delimited files: align button, remembered | not started | | |
| SVG: vector zoom, natural size from the viewBox | partial | #4 | size detection |
| Raster images: zoom and drag to pan | not started | | |
| Text files: verbatim, UTF-16/UTF-8/cp1252, cut at 5 MB | partial | #4 | reading and decoding |
| Mermaid script: pinned, SHA-256 checked, `--fetch-assets`, `./assets/` copy | not started | | |
| Command line: files, `--style`/`--light`/`--dark`/`--github`/`--github-ext`, `--no-watch` | not started | | |
| Tabs and single instance | not started | | |
| Sidebar | not started | | |
| Refresh (F5) and file watching | not started | | |
| Styles: four looks, Ctrl+D, 1 to 4, remembered | partial | #4 | the style choice rules |
| Fonts | not started | | |
| Find | not started | | |
| Zoom | not started | | |
| All shortcuts | not started | | |
| Links: local documents open in the tab with back and forward, `http(s)` in the browser, the rest handed to the OS after asking | partial | #5 | href to path, network-path check and OS hand-off; no window, tabs or asking yet. Differs on purpose: hrefs over 32 KiB and a bare `file:` give no path; a local symlink to a network share comes back unresolved so the caller asks before it is touched |
| File associations (Windows, `--register`) | not started | | |
| Preferences saved safely (`prefs.json`) | done | #4 | |
| Read-only; documents cannot run script, reach the app or navigate away | not started | | |
