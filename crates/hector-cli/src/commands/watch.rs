//! `hector watch` — a read-only live TUI over `.hector/log.jsonl`.
//!
//! All decision logic (aggregation in core, plus `handle_key`/`stream_lines`/
//! `explorer_lines`/`ui` here) is pure and tested; the only uncovered code is
//! the terminal setup (`run_tui`) and event loop (`event_loop`), kept minimal.
use anyhow::Result;
use std::io::IsTerminal;
use std::path::Path;

/// Entry point. Requires an interactive terminal; otherwise exits 1 with a hint.
pub fn run(dir: &Path) -> Result<i32> {
    if !std::io::stdout().is_terminal() {
        eprintln!("hector watch: requires an interactive terminal (no TTY detected).");
        eprintln!(
            "A non-interactive `--once` snapshot is planned; for now inspect {}/.hector/log.jsonl directly.",
            dir.display()
        );
        return Ok(1);
    }
    // Phase 4 replaces this stub with the live loop.
    Ok(0)
}
