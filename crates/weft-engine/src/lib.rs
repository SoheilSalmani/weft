//! Weft engine: record/commit/apply/update orchestration and worktree
//! management.
//!
//! All I/O (filesystem, prompts, subprocesses) lives here or in `weft-cli`;
//! `weft-core` stays pure.

#![forbid(unsafe_code)]

pub mod abstraction;
pub mod adopt;
pub mod amend;
pub mod answers;
pub mod author;
pub mod check;
pub mod commit;
pub mod compose;
pub mod describe;
pub mod diff;
pub mod discover;
pub mod fsio;
pub mod generate;
pub mod graph;
pub mod hooks;
pub mod init;
pub mod instance;
pub mod interact;
pub mod lock;
pub mod manifest;
pub mod new;
pub mod preset;
pub mod refresh;
pub mod secrets;
pub mod session;
pub mod squash;
pub mod stage;
pub mod start;
pub mod state;
pub mod template;
pub mod update;
pub mod weftignore;

/// The default Starlark-backed expression evaluator, re-exported so binary
/// crates don't need a direct `weft-lang` dependency.
pub fn eval() -> weft_lang::StarlarkEval {
    weft_lang::StarlarkEval
}
