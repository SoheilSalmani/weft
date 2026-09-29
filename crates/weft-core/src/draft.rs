//! A tree under construction. Rendering applies each active patch's ops to
//! a [`Draft`] and then finishes it into a [`Tree`].
//!
//! **Slots.** A patch may declare a slot, a named place in a file, as a line
//! of the lines it adds (see [`SlotDecl`]); other patches add lines to it
//! with `fill_slot`. While the draft is open a slot is a single marker line
//! that no hunk can match, and each fill is kept aside under its key.
//! [`Draft::finish`] replaces every marker with the slot's contributions in
//! key order, the separator ending every one but the last, and leaves out a
//! file whose `omit_when_empty` slots all stayed empty. Since no op sees what
//! is in a slot, patches that only fill slots commute however they are
//! ordered.
//!
//! **Tracing.** A traced draft also records where every line of every text
//! file came from ([`Trace`]): the node whose op wrote it, and whether it was
//! rendered from an `expr` segment or is slot content. `weft commit` uses
//! that to keep answer- and graph-dependent lines out of hunk context and to
//! record lines added inside a slot as a fill, and `weft check` to say which
//! segment a failing hunk's context came from.

use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};

use crate::id::PatchId;
use crate::patch::{Hunk, Op, Patch};
use crate::question::StarlarkExpr;
use crate::render::{render_line, render_path, ExprEval, RenderError, SlotClash};
use crate::segment::{join_lines, Line, Segment, SlotDecl, TemplatePath};
use crate::tree::{FileData, FileEntry, Tree};
use crate::value::AnswerSet;

/// Where one line of a rendered text file came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineOrigin {
    /// The graph node whose op wrote the line: a root patch's id, or an
    /// include patch's keyed node id.
    pub node: PatchId,
    pub source: LineSource,
}

/// What a rendered line was rendered from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineSource {
    /// Literal text and answer references: abstraction turns the rendered
    /// text back into a line that renders the same under other answers.
    Plain,
    /// A line holding an `expr` segment (the first one's source): under
    /// other answers it can read differently, or be empty.
    Expr(StarlarkExpr),
    /// Slot content, which is there only while the patch adding it is
    /// active: the contribution under `key` to slot `slot`. In a draft, the
    /// slot's own marker line has no key; a finished trace never shows one.
    Slot { slot: String, key: Option<String> },
}

impl LineOrigin {
    /// Whether a hunk recorded against this line may take it as context,
    /// i.e. whether the line reads the same wherever the hunk applies.
    pub fn anchors(&self) -> bool {
        matches!(self.source, LineSource::Plain)
    }
}

/// Provenance of a rendered tree: for every text file, one [`LineOrigin`]
/// per line of its text (`str::lines`), and where its slots sit. A file
/// `omit_when_empty` left out keeps its entry, with the text it would have
/// had ([`Self::omitted`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace {
    files: BTreeMap<Utf8PathBuf, Vec<LineOrigin>>,
    slots: BTreeMap<Utf8PathBuf, Vec<SlotSpan>>,
    omitted: BTreeMap<Utf8PathBuf, String>,
}

impl Trace {
    /// The origins of `path`'s lines, if it is a traced text file.
    pub fn lines(&self, path: &Utf8Path) -> Option<&[LineOrigin]> {
        self.files.get(path).map(Vec::as_slice)
    }

    /// The slots of `path`, in file order.
    pub fn slots(&self, path: &Utf8Path) -> &[SlotSpan] {
        self.slots.get(path).map(Vec::as_slice).unwrap_or_default()
    }

    /// The text of a file the render left out because its `omit_when_empty`
    /// slots were all empty, as it reads with those slots empty. A worktree
    /// that has the file adds to its slots; recording diffs against this.
    pub fn omitted(&self, path: &Utf8Path) -> Option<&str> {
        self.omitted.get(path).map(String::as_str)
    }
}

/// Where a slot sits in a rendered file, and what fills it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotSpan {
    pub name: String,
    pub separator: String,
    /// The node whose op declared the slot.
    pub declared_by: PatchId,
    /// The 0-based line where the slot's content starts: where an empty
    /// slot sits.
    pub start: usize,
    /// Its contributions, in key order (the order they appear in).
    pub fills: Vec<SlotFill>,
}

impl SlotSpan {
    /// One past the slot's last line.
    pub fn end(&self) -> usize {
        self.start + self.fills.iter().map(|f| f.lines).sum::<usize>()
    }
}

/// One contribution to a slot, as rendered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotFill {
    pub key: String,
    /// The node whose `fill_slot` added it.
    pub node: PatchId,
    /// How many lines of the file it takes.
    pub lines: usize,
}

/// Which part of a hunk's pattern a line belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HunkPart {
    ContextBefore,
    Removed,
    ContextAfter,
}

/// How close a hunk that matched nowhere came to matching, from a traced
/// render: the best partial alignment of its pattern against the file, and
/// the first line where the two disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NearMiss {
    pub part: HunkPart,
    /// The line the hunk expects there.
    pub expected: String,
    /// The file's line at that position (1-based), what it reads, and where
    /// it came from; `None` when the pattern runs off the start or end of
    /// the file.
    pub found: Option<(usize, String, LineOrigin)>,
}

/// A tree being rendered.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    tree: Tree,
    /// One origin per line of every text file, when tracing.
    origins: Option<BTreeMap<Utf8PathBuf, Vec<LineOrigin>>>,
    /// The slots declared in each file, until `finish` renders them.
    slots: BTreeMap<Utf8PathBuf, FileSlots>,
}

#[derive(Clone, Debug, Default)]
struct FileSlots {
    /// In declaration order; names are unique within the file.
    slots: Vec<SlotState>,
    /// The `omit_when_empty` of the `create_file` that made the file.
    omit: Option<Omit>,
}

#[derive(Clone, Debug)]
struct Omit {
    owner: PatchId,
    slots: Vec<String>,
    /// The first other node that changed the file by anything but a fill.
    edited_by: Option<PatchId>,
}

#[derive(Clone, Debug)]
struct SlotState {
    name: String,
    separator: String,
    declared_by: PatchId,
    /// Contributions by key: who added them and their rendered lines.
    fills: BTreeMap<String, (PatchId, Vec<String>)>,
}

impl FileSlots {
    fn slot(&self, name: &str) -> Option<&SlotState> {
        self.slots.iter().find(|s| s.name == name)
    }

    fn declare(
        &mut self,
        node: PatchId,
        path: &Utf8Path,
        decl: &SlotDecl,
    ) -> Result<(), RenderError> {
        if self.slot(&decl.slot).is_some() {
            return Err(RenderError::DuplicateSlot {
                patch: node,
                path: path.to_owned(),
                slot: decl.slot.clone(),
            });
        }
        self.slots.push(SlotState {
            name: decl.slot.clone(),
            separator: decl.separator.clone(),
            declared_by: node,
            fills: BTreeMap::new(),
        });
        Ok(())
    }
}

/// The line a slot occupies in a draft's text: nothing a patch writes can
/// read like it, so no hunk anchors on it and a slot name is all it holds.
const MARKER: &str = "\u{0}weft-slot\u{0}";

fn marker(name: &str) -> String {
    format!("{MARKER}{name}")
}

fn marker_name(line: &str) -> Option<&str> {
    line.strip_prefix(MARKER)
}

impl Draft {
    pub fn new() -> Self {
        Self::default()
    }

    /// A draft that records where every line came from.
    pub fn traced() -> Self {
        Draft {
            origins: Some(BTreeMap::new()),
            ..Self::default()
        }
    }

    /// Apply one patch's ops, as graph node `node` (errors and origins
    /// name it), with every op path placed under `mount`. The patch's
    /// `when` gate is **not** evaluated here: the caller decides whether the
    /// patch applies.
    pub fn apply(
        &mut self,
        node: PatchId,
        patch: &Patch,
        answers: &AnswerSet,
        eval: &dyn ExprEval,
        mount: &Utf8Path,
    ) -> Result<(), RenderError> {
        let place = |path: &TemplatePath| -> Result<Utf8PathBuf, RenderError> {
            let path = render_path(path, answers, eval)?;
            Ok(if mount.as_str().is_empty() {
                path
            } else {
                mount.join(path)
            })
        };
        for op in &patch.ops {
            match op {
                Op::CreateFile {
                    path,
                    omit_when_empty,
                    content,
                    mode,
                } => {
                    let path = place(path)?;
                    if self.tree.get(&path).is_some() {
                        return Err(RenderError::CreateExists { patch: node, path });
                    }
                    let declared = declared_slots(&content.0, node, &path)?;
                    let lines = render_added(&content.0, answers, eval)?;
                    let mut slots = FileSlots::default();
                    for decl in declared {
                        slots.declare(node, &path, decl)?;
                    }
                    if let Some(name) = omit_when_empty.iter().find(|s| slots.slot(s).is_none()) {
                        return Err(RenderError::UnknownSlot {
                            patch: node,
                            path,
                            slot: name.clone(),
                        });
                    }
                    if !omit_when_empty.is_empty() {
                        slots.omit = Some(Omit {
                            owner: node,
                            slots: omit_when_empty.clone(),
                            edited_by: None,
                        });
                    }
                    if !slots.slots.is_empty() {
                        self.slots.insert(path.clone(), slots);
                    }
                    if let Some(origins) = &mut self.origins {
                        origins.insert(path.clone(), created_origins(node, &content.0, &lines));
                    }
                    self.tree.insert(
                        path,
                        FileEntry {
                            content: join_lines(&lines).into(),
                            mode: *mode,
                        },
                    );
                }
                Op::CreateBinaryFile { path, data, mode } => {
                    let path = place(path)?;
                    if self.tree.get(&path).is_some() {
                        return Err(RenderError::CreateExists { patch: node, path });
                    }
                    use base64::Engine as _;
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|_| RenderError::InvalidBinaryData {
                            patch: node,
                            path: path.clone(),
                        })?;
                    self.tree.insert(
                        path,
                        FileEntry {
                            content: FileData::from_bytes(bytes),
                            mode: *mode,
                        },
                    );
                }
                Op::ModifyFile { path, hunks } => {
                    let path = place(path)?;
                    let entry = self
                        .tree
                        .get(&path)
                        .ok_or_else(|| RenderError::MissingFile {
                            patch: node,
                            path: path.clone(),
                        })?;
                    let Some(text) = entry.content.text() else {
                        return Err(RenderError::BinaryModify { patch: node, path });
                    };
                    let mut declared = Vec::new();
                    for hunk in hunks {
                        declared.extend(declared_slots(&hunk.added, node, &path)?);
                    }
                    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
                    // While the hunks apply, one origin per element of
                    // `lines`; an added line whose value spans several lines
                    // is split up once the op is done.
                    let mut origins = self
                        .origins
                        .as_mut()
                        .and_then(|o| o.remove(&path))
                        .filter(|o| o.len() == lines.len());
                    for (i, hunk) in hunks.iter().enumerate() {
                        apply_hunk(&mut lines, origins.as_mut(), node, hunk, answers, eval)
                            .map_err(|e| match e {
                                HunkApplyError::NoMatch(near) => RenderError::HunkNoMatch {
                                    patch: node,
                                    path: path.clone(),
                                    hunk: i,
                                    near,
                                },
                                HunkApplyError::Ambiguous(count) => RenderError::HunkAmbiguous {
                                    patch: node,
                                    path: path.clone(),
                                    hunk: i,
                                    count,
                                },
                                HunkApplyError::Render(e) => e,
                            })?;
                    }
                    let mode = entry.mode;
                    if !declared.is_empty() {
                        let slots = self.slots.entry(path.clone()).or_default();
                        for decl in declared {
                            slots.declare(node, &path, decl)?;
                        }
                    }
                    self.touch(&path, node);
                    if let (Some(all), Some(origins)) = (&mut self.origins, origins) {
                        all.insert(path.clone(), split_origins(&lines, origins));
                    }
                    self.tree.insert(
                        path,
                        FileEntry {
                            content: join_lines(&lines).into(),
                            mode,
                        },
                    );
                }
                Op::DeleteFile { path } => {
                    let path = place(path)?;
                    if self.tree.remove(&path).is_none() {
                        return Err(RenderError::MissingFile { patch: node, path });
                    }
                    if let Some(origins) = &mut self.origins {
                        origins.remove(&path);
                    }
                    self.slots.remove(&path);
                }
                Op::RenamePath { from, to } => {
                    let from = place(from)?;
                    let to = place(to)?;
                    let entry =
                        self.tree
                            .remove(&from)
                            .ok_or_else(|| RenderError::MissingFile {
                                patch: node,
                                path: from.clone(),
                            })?;
                    if self.tree.get(&to).is_some() {
                        return Err(RenderError::RenameExists {
                            patch: node,
                            path: to,
                        });
                    }
                    if let Some(origins) = &mut self.origins {
                        if let Some(lines) = origins.remove(&from) {
                            origins.insert(to.clone(), lines);
                        }
                    }
                    if let Some(slots) = self.slots.remove(&from) {
                        self.slots.insert(to.clone(), slots);
                    }
                    self.touch(&to, node);
                    self.tree.insert(to, entry);
                }
                Op::SetMode { path, mode } => {
                    let path = place(path)?;
                    let entry = self
                        .tree
                        .get(&path)
                        .ok_or_else(|| RenderError::MissingFile {
                            patch: node,
                            path: path.clone(),
                        })?;
                    let content = entry.content.clone();
                    self.touch(&path, node);
                    self.tree.insert(
                        path,
                        FileEntry {
                            content,
                            mode: *mode,
                        },
                    );
                }
                Op::FillSlot {
                    path,
                    slot,
                    key,
                    lines,
                } => {
                    let path = place(path)?;
                    if self.tree.get(&path).is_none() {
                        return Err(RenderError::MissingFile { patch: node, path });
                    }
                    let key = render_line(key, answers, eval)?;
                    if key.is_empty() || key.contains('\n') {
                        return Err(RenderError::InvalidSlotKey {
                            patch: node,
                            path,
                            slot: slot.clone(),
                            key,
                        });
                    }
                    if lines.is_empty() {
                        return Err(RenderError::EmptySlotFill {
                            patch: node,
                            path,
                            slot: slot.clone(),
                        });
                    }
                    // A slot line in a fill is refused here: slots do not nest.
                    let rendered = render_lines(lines, answers, eval)?;
                    let Some(state) = self
                        .slots
                        .get_mut(&path)
                        .and_then(|f| f.slots.iter_mut().find(|s| s.name == *slot))
                    else {
                        return Err(RenderError::UnknownSlot {
                            patch: node,
                            path,
                            slot: slot.clone(),
                        });
                    };
                    if let Some((first, _)) = state.fills.get(&key) {
                        return Err(RenderError::DuplicateSlotKey(Box::new(SlotClash {
                            path,
                            slot: slot.clone(),
                            key,
                            first: *first,
                            second: node,
                        })));
                    }
                    state.fills.insert(key, (node, rendered));
                }
            }
        }
        Ok(())
    }

    /// Note that `node` changed `path` by something other than a fill, for
    /// the `omit_when_empty` rule.
    fn touch(&mut self, path: &Utf8Path, node: PatchId) {
        let omit = self.slots.get_mut(path).and_then(|f| f.omit.as_mut());
        if let Some(omit) = omit {
            if omit.owner != node && omit.edited_by.is_none() {
                omit.edited_by = Some(node);
            }
        }
    }

    /// The rendered tree.
    pub fn finish(self) -> Result<Tree, RenderError> {
        Ok(self.finish_traced()?.0)
    }

    /// The rendered tree and where its lines came from (no lines unless the
    /// draft was [traced](Self::traced)). Renders every slot; a file whose
    /// `omit_when_empty` slots are all empty is left out, which is an error
    /// when another patch changed the file otherwise.
    pub fn finish_traced(self) -> Result<(Tree, Trace), RenderError> {
        let Draft {
            mut tree,
            origins,
            slots,
        } = self;
        let tracing = origins.is_some();
        let mut files = origins.unwrap_or_default();
        let mut spans_by_file = BTreeMap::new();
        let mut omitted = BTreeMap::new();
        for (path, file) in slots {
            let omit = match &file.omit {
                Some(omit)
                    if omit
                        .slots
                        .iter()
                        .all(|name| file.slot(name).is_none_or(|s| s.fills.is_empty())) =>
                {
                    if let Some(editor) = omit.edited_by {
                        return Err(RenderError::OmittedFileEdited {
                            path,
                            owner: omit.owner,
                            editor,
                        });
                    }
                    true
                }
                _ => false,
            };
            let Some(entry) = tree.get(&path) else {
                continue;
            };
            let Some(text) = entry.content.text() else {
                continue;
            };
            let mode = entry.mode;
            let draft_lines: Vec<&str> = text.lines().collect();
            let old_origins = files.remove(&path).filter(|o| o.len() == draft_lines.len());
            let mut out: Vec<String> = Vec::with_capacity(draft_lines.len());
            let mut out_origins: Vec<LineOrigin> = Vec::with_capacity(draft_lines.len());
            let mut slot_spans = Vec::new();
            let mut placed: BTreeSet<&str> = BTreeSet::new();
            // Lines of the finished text so far (a filled value may span
            // several).
            let mut at = 0usize;
            for (i, line) in draft_lines.iter().enumerate() {
                let Some(name) = marker_name(line) else {
                    out.push((*line).to_owned());
                    if let Some(o) = &old_origins {
                        out_origins.push(o[i].clone());
                    }
                    at += 1;
                    continue;
                };
                let state = file
                    .slot(name)
                    .filter(|_| placed.insert(name))
                    .ok_or_else(|| RenderError::SlotLost {
                        path: path.clone(),
                        slot: name.to_owned(),
                    })?;
                let start = at;
                let mut fills = Vec::with_capacity(state.fills.len());
                for (k, (key, (by, lines))) in state.fills.iter().enumerate() {
                    let first = at;
                    let final_fill = k + 1 == state.fills.len();
                    for (j, text) in lines.iter().enumerate() {
                        let mut text = text.clone();
                        if j + 1 == lines.len() && !final_fill {
                            text.push_str(&state.separator);
                        }
                        let n = spans(&text);
                        out_origins.extend(std::iter::repeat_n(
                            LineOrigin {
                                node: *by,
                                source: LineSource::Slot {
                                    slot: name.to_owned(),
                                    key: Some(key.clone()),
                                },
                            },
                            n,
                        ));
                        at += n;
                        out.push(text);
                    }
                    fills.push(SlotFill {
                        key: key.clone(),
                        node: *by,
                        lines: at - first,
                    });
                }
                slot_spans.push(SlotSpan {
                    name: name.to_owned(),
                    separator: state.separator.clone(),
                    declared_by: state.declared_by,
                    start,
                    fills,
                });
            }
            if let Some(lost) = file
                .slots
                .iter()
                .find(|s| !placed.contains(s.name.as_str()))
            {
                return Err(RenderError::SlotLost {
                    path,
                    slot: lost.name.clone(),
                });
            }
            if tracing && old_origins.is_some() {
                files.insert(path.clone(), out_origins);
            }
            spans_by_file.insert(path.clone(), slot_spans);
            if omit {
                tree.remove(&path);
                if tracing {
                    omitted.insert(path, join_lines(&out));
                }
            } else {
                tree.insert(
                    path,
                    FileEntry {
                        content: join_lines(&out).into(),
                        mode,
                    },
                );
            }
        }
        Ok((
            tree,
            Trace {
                files,
                slots: spans_by_file,
                omitted,
            },
        ))
    }
}

fn render_lines(
    lines: &[Line],
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Vec<String>, RenderError> {
    lines
        .iter()
        .map(|l| render_line(l, answers, eval))
        .collect()
}

/// Render the lines a patch adds (`create_file` content, a hunk's `added`
/// lines), where a slot line becomes the slot's marker.
fn render_added(
    lines: &[Line],
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Vec<String>, RenderError> {
    lines
        .iter()
        .map(|line| match line.as_slot() {
            Some(decl) => Ok(marker(&decl.slot)),
            None => render_line(line, answers, eval),
        })
        .collect()
}

/// The slots `lines` declare, each with a valid name.
fn declared_slots<'l>(
    lines: &'l [Line],
    node: PatchId,
    path: &Utf8Path,
) -> Result<Vec<&'l SlotDecl>, RenderError> {
    lines
        .iter()
        .filter_map(Line::as_slot)
        .map(|decl| {
            if SlotDecl::valid_name(&decl.slot) {
                Ok(decl)
            } else {
                Err(RenderError::InvalidSlotName {
                    patch: node,
                    path: path.to_owned(),
                    slot: decl.slot.clone(),
                })
            }
        })
        .collect()
}

/// What `line` renders from, for its origin.
fn source_of(line: &Line) -> LineSource {
    if let Some(decl) = line.as_slot() {
        return LineSource::Slot {
            slot: decl.slot.clone(),
            key: None,
        };
    }
    line.0
        .iter()
        .find_map(|seg| match seg {
            Segment::Expr(expr) => Some(LineSource::Expr(expr.clone())),
            _ => None,
        })
        .unwrap_or(LineSource::Plain)
}

/// How many lines of file text a rendered line becomes: a value holding
/// newlines spans several.
fn spans(rendered: &str) -> usize {
    1 + rendered.matches('\n').count()
}

/// Origins of a created file's lines, one per line of its text.
fn created_origins(node: PatchId, lines: &[Line], rendered: &[String]) -> Vec<LineOrigin> {
    let mut out = Vec::with_capacity(rendered.len());
    for (line, text) in lines.iter().zip(rendered) {
        let origin = LineOrigin {
            node,
            source: source_of(line),
        };
        out.extend(std::iter::repeat_n(origin, spans(text)));
    }
    out
}

/// Expand origins kept one per element of `lines` to one per line of the
/// text `lines` joins into.
fn split_origins(lines: &[String], origins: Vec<LineOrigin>) -> Vec<LineOrigin> {
    if lines.iter().all(|l| !l.contains('\n')) {
        return origins;
    }
    let mut out = Vec::with_capacity(lines.len());
    for (line, origin) in lines.iter().zip(origins) {
        out.extend(std::iter::repeat_n(origin, spans(line)));
    }
    out
}

enum HunkApplyError {
    NoMatch(Option<Box<NearMiss>>),
    Ambiguous(usize),
    Render(RenderError),
}

/// Apply one context-anchored hunk. The pattern
/// `context_before + removed + context_after` must match exactly one position
/// in `lines`; `removed` is replaced with `added`. An entirely empty pattern
/// appends `added` at end of file. `origins`, when tracing, has one entry per
/// element of `lines` and is kept in step.
fn apply_hunk(
    lines: &mut Vec<String>,
    origins: Option<&mut Vec<LineOrigin>>,
    node: PatchId,
    hunk: &Hunk,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<(), HunkApplyError> {
    let rl = |ls: &[Line]| render_lines(ls, answers, eval).map_err(HunkApplyError::Render);
    let before = rl(&hunk.context_before)?;
    let removed = rl(&hunk.removed)?;
    let added = render_added(&hunk.added, answers, eval).map_err(HunkApplyError::Render)?;
    let after = rl(&hunk.context_after)?;
    let added_origins = hunk.added.iter().map(|line| LineOrigin {
        node,
        source: source_of(line),
    });

    let mut pattern: Vec<&str> = Vec::new();
    pattern.extend(before.iter().map(String::as_str));
    pattern.extend(removed.iter().map(String::as_str));
    pattern.extend(after.iter().map(String::as_str));

    if pattern.is_empty() {
        if let Some(origins) = origins {
            origins.extend(added_origins);
        }
        lines.extend(added);
        return Ok(());
    }

    let matches: Vec<usize> = (0..=lines.len().saturating_sub(pattern.len()))
        .filter(|&i| {
            lines.len() - i >= pattern.len()
                && pattern.iter().enumerate().all(|(j, p)| lines[i + j] == *p)
        })
        .collect();

    match matches.as_slice() {
        [] => Err(HunkApplyError::NoMatch(origins.and_then(|origins| {
            near_miss(lines, origins, [&before, &removed, &after]).map(Box::new)
        }))),
        [start] => {
            let at = start + before.len();
            if let Some(origins) = origins {
                origins.splice(at..at + removed.len(), added_origins);
            }
            lines.splice(at..at + removed.len(), added);
            Ok(())
        }
        many => Err(HunkApplyError::Ambiguous(many.len())),
    }
}

/// How a hunk's pattern missed `lines`, and its first disagreeing line.
/// First, a pattern that matches once the slots' marker lines are skipped
/// anchors across a slot: the marker it straddles is the line. Otherwise
/// the best partial alignment (partial overlaps with the file's ends
/// allowed), which is `None` unless one alignment matches strictly more
/// lines than any other, at least half the pattern, including a non-blank
/// line. Diagnostic only.
fn near_miss(
    lines: &[String],
    origins: &[LineOrigin],
    [before, removed, after]: [&Vec<String>; 3],
) -> Option<NearMiss> {
    let pattern: Vec<(HunkPart, &str)> = before
        .iter()
        .map(|l| (HunkPart::ContextBefore, l.as_str()))
        .chain(removed.iter().map(|l| (HunkPart::Removed, l.as_str())))
        .chain(after.iter().map(|l| (HunkPart::ContextAfter, l.as_str())))
        .collect();
    let unmarked: Vec<usize> = (0..lines.len())
        .filter(|&i| marker_name(&lines[i]).is_none())
        .collect();
    for window in unmarked.windows(pattern.len().max(1)) {
        let matches = window.len() == pattern.len()
            && window
                .iter()
                .zip(&pattern)
                .all(|(&i, (_, p))| lines[i] == *p);
        let straddled = window.windows(2).position(|w| w[1] != w[0] + 1);
        if let (true, Some(j)) = (matches, straddled) {
            let (part, expected) = pattern[j + 1];
            let i = window[j] + 1;
            return Some(NearMiss {
                part,
                expected: expected.to_owned(),
                found: Some((i + 1, lines[i].clone(), origins[i].clone())),
            });
        }
    }
    let (m, n) = (pattern.len() as isize, lines.len() as isize);
    let at = |start: isize, j: usize| -> Option<usize> {
        let i = start + j as isize;
        (0..n).contains(&i).then_some(i as usize)
    };
    let mut best: Option<(usize, isize)> = None;
    let mut tied = false;
    for start in (1 - m)..n {
        let hits: Vec<&str> = pattern
            .iter()
            .enumerate()
            .filter(|(j, (_, p))| at(start, *j).is_some_and(|i| lines[i] == *p))
            .map(|(_, (_, p))| *p)
            .collect();
        if hits.is_empty() || hits.iter().all(|p| p.trim().is_empty()) {
            continue;
        }
        match best {
            Some((count, _)) if hits.len() < count => {}
            Some((count, _)) if hits.len() == count => tied = true,
            _ => {
                best = Some((hits.len(), start));
                tied = false;
            }
        }
    }
    let (count, start) = best?;
    if tied || count * 2 < pattern.len() {
        return None;
    }
    let (j, (part, expected)) = pattern
        .iter()
        .enumerate()
        .find(|(j, (_, p))| at(start, *j).is_none_or(|i| lines[i] != *p))?;
    Some(NearMiss {
        part: *part,
        expected: (*expected).to_owned(),
        found: at(start, j).map(|i| (i + 1, lines[i].clone(), origins[i].clone())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::test_support::StubEval;
    use crate::segment::Content;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn node() -> PatchId {
        Patch::new(vec![], None, vec![]).id
    }

    fn root() -> &'static Utf8Path {
        Utf8Path::new("")
    }

    fn create(path: &str, lines: Vec<Line>) -> Op {
        Op::CreateFile {
            path: TemplatePath::literal(path),
            omit_when_empty: vec![],
            content: Content(lines),
            mode: crate::DEFAULT_FILE_MODE,
        }
    }

    #[test]
    fn hunk_applies_at_unique_context() {
        let mut lines = strings(&["a", "b", "c", "d"]);
        let hunk = Hunk {
            context_before: vec![Line::literal("b")],
            removed: vec![Line::literal("c")],
            added: vec![Line::literal("C1"), Line::literal("C2")],
            context_after: vec![Line::literal("d")],
        };
        apply_hunk(
            &mut lines,
            None,
            node(),
            &hunk,
            &AnswerSet::new(),
            &StubEval,
        )
        .ok()
        .unwrap();
        assert_eq!(lines, vec!["a", "b", "C1", "C2", "d"]);
    }

    #[test]
    fn hunk_fails_cleanly_on_missing_context() {
        let mut lines = strings(&["a"]);
        let hunk = Hunk {
            context_before: vec![Line::literal("nope")],
            ..Default::default()
        };
        assert!(matches!(
            apply_hunk(
                &mut lines,
                None,
                node(),
                &hunk,
                &AnswerSet::new(),
                &StubEval
            ),
            Err(HunkApplyError::NoMatch(None))
        ));
    }

    #[test]
    fn hunk_fails_on_ambiguous_context() {
        let mut lines = strings(&["x", "x"]);
        let hunk = Hunk {
            context_before: vec![Line::literal("x")],
            added: vec![Line::literal("y")],
            ..Default::default()
        };
        assert!(matches!(
            apply_hunk(
                &mut lines,
                None,
                node(),
                &hunk,
                &AnswerSet::new(),
                &StubEval
            ),
            Err(HunkApplyError::Ambiguous(2))
        ));
    }

    /// `[section]`, then a line rendered from an expression.
    fn owner() -> Patch {
        Patch::new(
            vec![],
            None,
            vec![create(
                "config.toml",
                vec![
                    Line::literal("[section]"),
                    Line(vec![Segment::Expr(StarlarkExpr::from("flag"))]),
                    Line::literal("tail"),
                ],
            )],
        )
    }

    fn with_flag(on: bool) -> AnswerSet {
        [(
            crate::AnswerId::from("flag"),
            crate::Value::String(if on { "on" } else { "off" }.into()),
        )]
        .into_iter()
        .collect()
    }

    #[test]
    fn a_traced_draft_marks_expression_lines_and_follows_hunks() {
        let owner = owner();
        let editor = Patch::new(
            vec![owner.id],
            None,
            vec![Op::ModifyFile {
                path: TemplatePath::literal("config.toml"),
                hunks: vec![Hunk {
                    context_before: vec![Line::literal("[section]")],
                    added: vec![Line::literal("one\ntwo")],
                    ..Default::default()
                }],
            }],
        );
        let answers = with_flag(true);
        let mut draft = Draft::traced();
        draft
            .apply(owner.id, &owner, &answers, &StubEval, root())
            .unwrap();
        draft
            .apply(editor.id, &editor, &answers, &StubEval, root())
            .unwrap();
        let (tree, trace) = draft.finish_traced().unwrap();
        let text = tree
            .get("config.toml".into())
            .unwrap()
            .content
            .text()
            .unwrap();
        assert_eq!(text, "[section]\none\ntwo\non\ntail\n");
        let sources: Vec<(PatchId, bool)> = trace
            .lines("config.toml".into())
            .unwrap()
            .iter()
            .map(|o| (o.node, o.anchors()))
            .collect();
        assert_eq!(
            sources,
            [
                (owner.id, true),
                (editor.id, true),
                (editor.id, true),
                (owner.id, false),
                (owner.id, true),
            ]
        );
    }

    #[test]
    fn a_failing_hunk_reports_the_line_it_missed_and_its_origin() {
        let owner = owner();
        // Recorded while `flag` rendered "on"; replayed where it renders "off".
        let editor = Patch::new(
            vec![owner.id],
            None,
            vec![Op::ModifyFile {
                path: TemplatePath::literal("config.toml"),
                hunks: vec![Hunk {
                    context_before: vec![Line::literal("[section]")],
                    added: vec![Line::literal("new")],
                    context_after: vec![Line::literal("on"), Line::literal("tail")],
                    ..Default::default()
                }],
            }],
        );
        let answers = with_flag(false);
        let mut draft = Draft::traced();
        draft
            .apply(owner.id, &owner, &answers, &StubEval, root())
            .unwrap();
        let err = draft
            .apply(editor.id, &editor, &answers, &StubEval, root())
            .unwrap_err();
        let RenderError::HunkNoMatch {
            near: Some(near), ..
        } = err
        else {
            panic!("expected a near miss, got {err:?}");
        };
        assert_eq!(near.part, HunkPart::ContextAfter);
        assert_eq!(near.expected, "on");
        let (line, found, origin) = near.found.unwrap();
        assert_eq!((line, found.as_str()), (2, "off"));
        assert_eq!(origin.node, owner.id);
        assert_eq!(origin.source, LineSource::Expr(StarlarkExpr::from("flag")));

        // Untraced renders skip the diagnosis.
        let mut plain = Draft::new();
        plain
            .apply(owner.id, &owner, &answers, &StubEval, root())
            .unwrap();
        let err = plain
            .apply(editor.id, &editor, &answers, &StubEval, root())
            .unwrap_err();
        assert!(matches!(err, RenderError::HunkNoMatch { near: None, .. }));
    }

    // ---- slots ----------------------------------------------------------

    fn slot(name: &str, separator: &str) -> Line {
        Line::slot(SlotDecl {
            slot: name.into(),
            separator: separator.into(),
        })
    }

    /// `.mcp.json`, declaring a `servers` slot separated by commas; omitted
    /// while nothing fills it.
    fn mcp() -> Patch {
        Patch::new(
            vec![],
            None,
            vec![Op::CreateFile {
                path: TemplatePath::literal(".mcp.json"),
                omit_when_empty: vec!["servers".into()],
                content: Content(vec![
                    Line::literal("{"),
                    Line::literal("  \"mcpServers\": {"),
                    slot("servers", ","),
                    Line::literal("  }"),
                    Line::literal("}"),
                ]),
                mode: crate::DEFAULT_FILE_MODE,
            }],
        )
    }

    fn fill(owner: &Patch, key: &str, lines: &[&str]) -> Patch {
        Patch::new(
            vec![owner.id],
            None,
            vec![Op::FillSlot {
                path: TemplatePath::literal(".mcp.json"),
                slot: "servers".into(),
                key: Line::literal(key),
                lines: lines.iter().map(|l| Line::literal(l)).collect(),
            }],
        )
    }

    fn render(order: &[&Patch]) -> Result<(Tree, Trace), RenderError> {
        let mut draft = Draft::traced();
        for patch in order {
            draft.apply(patch.id, patch, &AnswerSet::new(), &StubEval, root())?;
        }
        draft.finish_traced()
    }

    fn text(tree: &Tree, path: &str) -> String {
        tree.get(path.into())
            .unwrap()
            .content
            .text()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn slot_fills_render_in_key_order_with_separators_whatever_the_order() {
        let owner = mcp();
        let linear = fill(&owner, "linear", &["    \"linear\": {}"]);
        let lightdash = fill(&owner, "lightdash", &["    \"lightdash\": {", "    }"]);
        let (one, trace) = render(&[&owner, &linear, &lightdash]).unwrap();
        let (two, _) = render(&[&owner, &lightdash, &linear]).unwrap();
        assert_eq!(one, two);
        assert_eq!(
            text(&one, ".mcp.json"),
            "{\n  \"mcpServers\": {\n    \"lightdash\": {\n    },\n    \"linear\": {}\n  }\n}\n"
        );
        let span = &trace.slots(".mcp.json".into())[0];
        assert_eq!(
            (span.name.as_str(), span.start, span.end()),
            ("servers", 2, 5)
        );
        assert_eq!(span.declared_by, owner.id);
        let fills: Vec<(&str, PatchId, usize)> = span
            .fills
            .iter()
            .map(|f| (f.key.as_str(), f.node, f.lines))
            .collect();
        assert_eq!(
            fills,
            [("lightdash", lightdash.id, 2), ("linear", linear.id, 1)]
        );
        let origins = trace.lines(".mcp.json".into()).unwrap();
        assert!(origins[1].anchors() && origins[5].anchors());
        assert!(origins[2..5].iter().all(|o| !o.anchors()), "{origins:?}");
    }

    #[test]
    fn an_empty_slot_renders_nothing_and_omit_when_empty_drops_the_file() {
        let owner = mcp();
        let (tree, trace) = render(&[&owner]).unwrap();
        assert!(tree.get(".mcp.json".into()).is_none());
        // What recording diffs a worktree that creates the file against.
        assert_eq!(
            trace.omitted(".mcp.json".into()),
            Some("{\n  \"mcpServers\": {\n  }\n}\n")
        );
        assert_eq!(trace.slots(".mcp.json".into())[0].start, 2);

        let mut kept = owner.clone();
        let Op::CreateFile {
            omit_when_empty, ..
        } = &mut kept.ops[0]
        else {
            unreachable!()
        };
        omit_when_empty.clear();
        let (tree, trace) = render(&[&kept]).unwrap();
        assert_eq!(text(&tree, ".mcp.json"), "{\n  \"mcpServers\": {\n  }\n}\n");
        assert_eq!(trace.slots(".mcp.json".into())[0].start, 2);
    }

    #[test]
    fn hunks_never_see_slot_content() {
        let owner = mcp();
        let linear = fill(&owner, "linear", &["    \"linear\": {}"]);
        // Anchored on the lines around the slot, recorded while it was empty.
        let across = Patch::new(
            vec![owner.id],
            None,
            vec![Op::ModifyFile {
                path: TemplatePath::literal(".mcp.json"),
                hunks: vec![Hunk {
                    context_before: vec![Line::literal("  \"mcpServers\": {")],
                    added: vec![Line::literal("    \"x\": {}")],
                    context_after: vec![Line::literal("  }")],
                    ..Default::default()
                }],
            }],
        );
        let err = render(&[&owner, &across]).unwrap_err();
        let RenderError::HunkNoMatch {
            near: Some(near), ..
        } = err
        else {
            panic!("{err:?}")
        };
        let (_, _, origin) = near.found.unwrap();
        assert_eq!(
            origin.source,
            LineSource::Slot {
                slot: "servers".into(),
                key: None
            }
        );
        // A hunk next to the slot applies with or without its content.
        let header = Patch::new(
            vec![owner.id],
            None,
            vec![Op::ModifyFile {
                path: TemplatePath::literal(".mcp.json"),
                hunks: vec![Hunk {
                    context_before: vec![Line::literal("{")],
                    added: vec![Line::literal("  \"x\": 1,")],
                    ..Default::default()
                }],
            }],
        );
        let (one, _) = render(&[&owner, &linear, &header]).unwrap();
        let (two, _) = render(&[&owner, &header, &linear]).unwrap();
        assert_eq!(one, two);
    }

    #[test]
    fn slot_errors_name_what_went_wrong() {
        let owner = mcp();
        let a = fill(&owner, "same", &["a"]);
        let b = fill(&owner, "same", &["b"]);
        let err = render(&[&owner, &a, &b]).unwrap_err();
        assert!(
            matches!(&err, RenderError::DuplicateSlotKey(clash)
                if clash.first == a.id && clash.second == b.id),
            "{err:?}"
        );
        let mut unknown = fill(&owner, "x", &["x"]);
        let Op::FillSlot { slot: name, .. } = &mut unknown.ops[0] else {
            unreachable!()
        };
        *name = "nope".into();
        assert!(matches!(
            render(&[&owner, &unknown]),
            Err(RenderError::UnknownSlot { .. })
        ));
        let nested = Patch::new(
            vec![owner.id],
            None,
            vec![Op::FillSlot {
                path: TemplatePath::literal(".mcp.json"),
                slot: "servers".into(),
                key: Line::literal("k"),
                lines: vec![slot("inner", "")],
            }],
        );
        assert!(matches!(
            render(&[&owner, &nested]),
            Err(RenderError::MisplacedSlot(name)) if name == "inner"
        ));
        let twice = Patch::new(
            vec![],
            None,
            vec![create("x", vec![slot("s", ""), slot("s", "")])],
        );
        assert!(matches!(
            render(&[&twice]),
            Err(RenderError::DuplicateSlot { .. })
        ));
    }

    #[test]
    fn a_file_omitted_while_empty_may_not_be_edited_by_another_patch() {
        let owner = mcp();
        let editor = Patch::new(
            vec![owner.id],
            None,
            vec![Op::ModifyFile {
                path: TemplatePath::literal(".mcp.json"),
                hunks: vec![Hunk {
                    context_before: vec![Line::literal("{")],
                    added: vec![Line::literal("  \"x\": 1,")],
                    ..Default::default()
                }],
            }],
        );
        let err = render(&[&owner, &editor]).unwrap_err();
        assert!(
            matches!(&err, RenderError::OmittedFileEdited { owner: o, editor: e, .. }
                if *o == owner.id && *e == editor.id),
            "{err:?}"
        );
        // Filled, the file stays and the edit applies.
        let linear = fill(&owner, "linear", &["    \"linear\": {}"]);
        let (tree, _) = render(&[&owner, &editor, &linear]).unwrap();
        assert!(text(&tree, ".mcp.json").starts_with("{\n  \"x\": 1,\n"));
    }

    #[test]
    fn a_hunk_can_open_a_slot() {
        let base = Patch::new(
            vec![],
            None,
            vec![create(
                "mise.toml",
                vec![Line::literal("[tools]"), Line::literal("node = \"24\"")],
            )],
        );
        let opener = Patch::new(
            vec![base.id],
            None,
            vec![Op::ModifyFile {
                path: TemplatePath::literal("mise.toml"),
                hunks: vec![Hunk {
                    context_before: vec![Line::literal("node = \"24\"")],
                    added: vec![Line::literal(""), Line::literal("[env]"), slot("env", "")],
                    ..Default::default()
                }],
            }],
        );
        let filler = Patch::new(
            vec![opener.id],
            None,
            vec![Op::FillSlot {
                path: TemplatePath::literal("mise.toml"),
                slot: "env".into(),
                key: Line::literal("lightdash"),
                lines: vec![Line::literal("URL = \"x\"")],
            }],
        );
        let (tree, trace) = render(&[&base, &opener, &filler]).unwrap();
        assert_eq!(
            text(&tree, "mise.toml"),
            "[tools]\nnode = \"24\"\n\n[env]\nURL = \"x\"\n"
        );
        assert_eq!(trace.slots("mise.toml".into())[0].declared_by, opener.id);
    }
}
