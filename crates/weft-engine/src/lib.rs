//! Weft engine: record/commit/apply/update orchestration and worktree
//! management.
//!
//! All I/O (filesystem, prompts, subprocesses) lives here or in `weft-cli`;
//! `weft-core` stays pure.

#![forbid(unsafe_code)]

pub mod abstraction;
pub mod answers;
pub mod commit;
pub mod diff;
pub mod fsio;
pub mod interact;
pub mod manifest;
pub mod new;
pub mod record;
pub mod secrets;
pub mod session;
pub mod state;
pub mod tasks;
pub mod template;
