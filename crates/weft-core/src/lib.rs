//! Weft core: data model, patch algebra, and pure rendering.
//!
//! This crate is intentionally I/O-free: no filesystem access, no prompts,
//! no process spawning. All policy lives in `weft-engine`.

#![forbid(unsafe_code)]

pub mod id;
pub mod patch;
pub mod question;
pub mod segment;
pub mod task;
pub mod tree;
pub mod value;

pub use id::{AnswerId, PatchId, TaskId};
pub use patch::{Hunk, Op, Patch, DEFAULT_FILE_MODE};
pub use question::{AnswerKind, Question, SecretSpec, StarlarkExpr};
pub use segment::{join_lines, Content, Line, Segment, TemplatePath};
pub use task::{Task, TaskAction, TaskInput};
pub use tree::{FileEntry, Tree};
pub use value::{AnswerSet, SecretValue, Value};
