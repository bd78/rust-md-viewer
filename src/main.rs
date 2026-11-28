// Documents, links and paths are hostile input: the viewer returns errors, it never panics (AGENTS.md, Rust rule 1).
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::io::Write;
use std::process::ExitCode;

// A placeholder until the UI stack is chosen (AGENTS.md): it only says what it is.
fn main() -> ExitCode {
    // println! panics when stdout is closed or full; report a failed write instead.
    let (name, version) = (md_view::config::APP_NAME, env!("CARGO_PKG_VERSION"));
    match writeln!(std::io::stdout(), "{name} {version}") {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
