//! Weft core: data model, patch algebra, and pure rendering.
//!
//! This crate is intentionally I/O-free: no filesystem access, no prompts,
//! no process spawning. All policy lives in `weft-engine`.

#![forbid(unsafe_code)]

pub mod generator;
pub mod hook;
pub mod id;
pub mod merge;
pub mod patch;
pub mod question;
pub mod render;
pub mod segment;
pub mod tree;
pub mod value;

pub use generator::Generator;
pub use hook::{Command, Hook, HookEffect, HookInput, HookPhase};
pub use id::{AnswerId, HookId, PatchId};
pub use patch::{Hunk, Op, Patch, PatchMeta, DEFAULT_FILE_MODE};
pub use question::{AnswerKind, Question, SecretSpec, StarlarkExpr};
pub use segment::{join_lines, Content, Line, Segment, TemplatePath};
pub use tree::{FileEntry, Tree};
pub use value::{AnswerSet, SecretValue, Value};
