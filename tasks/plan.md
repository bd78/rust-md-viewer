# Current plan — feat/links branch

## Context

Branch `feat/links` is based on `main` (after PR #4 and #5 merged the links module and PARITY.md update).
The branch has two committed changes:
- `93ca4da` — Add links module
- `2cb3817` — Record the links pull request in PARITY.md

## Uncommitted work in progress

Four files changed (not yet committed):
- `Cargo.toml` — added `pulldown-cmark`, `syntect`, `sha2`, `ureq`
- `Cargo.lock` — updated lockfile
- `src/lib.rs` — added `pub mod markdown_render;`
- `src/markdown_render.rs` — new module: Markdown-to-HTML rendering with syntax highlighting, Mermaid blocks, TOC, task lists, and sanitisation

## Status

- [ ] Verify new dependencies' licences against the allowlist
- [ ] Run local checks (`cargo fmt --check`, `cargo clippy`, `cargo test`)
- [ ] Confirm the module matches py-md-view behaviour
- [ ] Decide: does this belong on `feat/links` or a new branch?
- [ ] Commit when checks pass and review is clean
