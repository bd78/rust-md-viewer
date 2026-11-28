# TODO — markdown_render module

## Immediate

1. **Licence check** — verify `pulldown-cmark`, `syntect`, `sha2`, `ureq` and all transitive deps
2. **Local checks** — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
3. **Review the code** — check it matches py-md-view's `render_markdown` behaviour
4. **Branch hygiene** — the links branch already shipped links; markdown_render is a new feature → should be its own branch

## Blocked / Waiting

- UI stack decision (AGENTS.md: no UI code until the person decides)
- PARITY.md update for Markdown rendering features once committed
