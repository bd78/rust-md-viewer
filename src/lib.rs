//! md-view: a read-only desktop viewer for Markdown, Mermaid, CSV, SVG and images.
//! Rust port of py-md-view: `config`, `documents` and `links` mirror the Python modules of the
//! same name; `html` holds the two Python standard-library string helpers they lean on.

// Documents, links and paths are hostile input: the viewer returns errors, it never panics (AGENTS.md, Rust rule 1).
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::todo,
        clippy::unreachable
    )
)]

pub mod cli;
pub mod config;
pub mod documents;
pub mod html;
pub mod links;
pub mod markdown_render;
pub mod mermaid_render;
