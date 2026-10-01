//! `weft share`: patches that each create one file become a patch that owns
//! the lines they share, with a slot each of them fills.
//!
//! Nobody writes the slot. Weft compares the patches' versions of the file
//! for the lines they have in common before and after their own, and takes
//! the separator between contributions from a combined file the author
//! confirms: the file as a project with every one of the patches on reads
//! it. The owner leaves the file out while no patch fills its slot, so each
//! patch alone still renders exactly what it rendered before; only the
//! combination that used to clash is new.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use weft_core::{Content, Line, Op, PatchMeta, Segment, SlotDecl, TemplatePath};

use crate::graph::{display_path, display_segments};
use crate::interact::Interaction;
use crate::session::Session;
use crate::template::{Node, NodeKind, PatchFile, Template};

/// The slot a shared file's contributions go in.
pub const SLOT: &str = "entries";

/// The longest separator a combined file can show between contributions.
const MAX_SEPARATOR: usize = 16;

// ---- working out the shared lines ----------------------------------------

/// One patch's version of a shared file: the patch's name, which becomes
/// the key its lines go under, and the lines it creates the file with.
#[derive(Clone, Debug)]
pub struct Version {
    pub name: String,
    pub lines: Vec<Line>,
}

/// A shared file worked out from its versions: the lines around the slot,
/// the separator, and each patch's own lines by name (key order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split {
    pub before: Vec<Line>,
    pub after: Vec<Line>,
    pub separator: String,
    pub fills: BTreeMap<String, Vec<Line>>,
}

impl Split {
    /// The file with every patch on, as the author reviews it: answer
    /// references read `{id}`, expressions `{=source}`.
    pub fn combined(&self) -> String {
        let order: Vec<&str> = self.fills.keys().map(String::as_str).collect();
        self.combined_in(&order)
    }

    fn combined_in(&self, order: &[&str]) -> String {
        let mut out: Vec<String> = self.before.iter().map(display_line).collect();
        for (k, name) in order.iter().enumerate() {
            let lines = &self.fills[*name];
            for (j, line) in lines.iter().enumerate() {
                let mut text = display_line(line);
                if j + 1 == lines.len() && k + 1 < order.len() {
                    text.push_str(&self.separator);
                }
                out.push(text);
            }
        }
        out.extend(self.after.iter().map(display_line));
        normalized(&out.join("\n"))
    }

    /// The owner's content: the shared lines around the slot.
    fn frame(&self) -> Vec<Line> {
        let mut lines = self.before.clone();
        lines.push(Line::slot(SlotDecl {
            slot: SLOT.to_owned(),
            separator: self.separator.clone(),
        }));
        lines.extend(self.after.iter().cloned());
        lines
    }
}

fn display_line(line: &Line) -> String {
    display_segments(&line.0)
}

/// Text compared by content: `\r\n` read as `\n`, trailing blank space
/// dropped, one final newline.
fn normalized(text: &str) -> String {
    let mut out = text.replace("\r\n", "\n").trim_end().to_owned();
    out.push('\n');
    out
}

/// A line's segments with adjacent literals merged and empty ones dropped,
/// so two spellings of one line compare equal.
fn canonical(line: &Line) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    for seg in &line.0 {
        match (seg, out.last_mut()) {
            (Segment::Literal(s), _) if s.is_empty() => {}
            (Segment::Literal(s), Some(Segment::Literal(prev))) => prev.push_str(s),
            _ => out.push(seg.clone()),
        }
    }
    out
}

fn same(a: &Line, b: &Line) -> bool {
    canonical(a) == canonical(b)
}

fn check_versions(path: &str, versions: &[Version]) -> Result<()> {
    match versions {
        [] => bail!("no patch creates `{path}`"),
        [only] => bail!(
            "only `{}` creates `{path}`; weft shares a file that two or more patches create",
            only.name
        ),
        _ => {}
    }
    for v in versions {
        if v.lines.is_empty() {
            bail!(
                "`{}` creates `{path}` empty, so it has no lines to add",
                v.name
            );
        }
        if v.lines.iter().any(Line::has_slot) {
            bail!(
                "`{}` already has a slot in `{path}`; record the other patches' lines inside it \
                 instead",
                v.name
            );
        }
    }
    Ok(())
}

/// Every way to cut the versions into lines they share before and after one
/// slot, as (lines before, lines after), each version keeping at least one
/// line of its own: the most shared lines first, then the most before.
fn cuts(versions: &[Version]) -> Vec<(usize, usize)> {
    let first = &versions[0].lines;
    let before = (0..first.len())
        .take_while(|&i| {
            versions[1..]
                .iter()
                .all(|v| v.lines.get(i).is_some_and(|l| same(l, &first[i])))
        })
        .count();
    let after = (1..=first.len())
        .take_while(|&k| {
            versions[1..].iter().all(|v| {
                v.lines.len() >= k && same(&v.lines[v.lines.len() - k], &first[first.len() - k])
            })
        })
        .count();
    let shortest = versions.iter().map(|v| v.lines.len()).min().unwrap_or(0);
    let mut out: Vec<(usize, usize)> = (0..=before)
        .flat_map(|b| (0..=after).map(move |a| (b, a)))
        .filter(|(b, a)| b + a < shortest)
        .collect();
    out.sort_by(|x, y| (y.0 + y.1).cmp(&(x.0 + x.1)).then(y.0.cmp(&x.0)));
    out
}

fn cut(versions: &[Version], (before, after): (usize, usize), separator: &str) -> Split {
    let first = &versions[0].lines;
    Split {
        before: first[..before].to_vec(),
        after: first[first.len() - after..].to_vec(),
        separator: separator.to_owned(),
        fills: versions
            .iter()
            .map(|v| {
                (
                    v.name.clone(),
                    v.lines[before..v.lines.len() - after].to_vec(),
                )
            })
            .collect(),
    }
}

/// Whether a contribution closes every bracket it opens, and opens every
/// one it closes: an entry of a list or map, not a piece of the frame.
fn balanced(lines: &[Line]) -> bool {
    let mut depth = [0i32; 3];
    for c in lines
        .iter()
        .flat_map(|l| display_line(l).chars().collect::<Vec<_>>())
    {
        let (i, step) = match c {
            '{' => (0, 1),
            '}' => (0, -1),
            '[' => (1, 1),
            ']' => (1, -1),
            '(' => (2, 1),
            ')' => (2, -1),
            _ => continue,
        };
        depth[i] += step;
        if depth[i] < 0 {
            return false;
        }
    }
    depth == [0; 3]
}

/// Weft's first guess: the most shared lines that leave every patch's own
/// lines balanced, and a `,` between contributions in a JSON file.
pub fn propose(path: &str, versions: &[Version]) -> Result<Split> {
    check_versions(path, versions)?;
    let separator = match Utf8Path::new(path).extension() {
        Some("json" | "jsonc" | "json5") => ",",
        _ => "",
    };
    let cuts = cuts(versions);
    let pick = cuts
        .iter()
        .find(|&&c| cut(versions, c, "").fills.values().all(|l| balanced(l)))
        .or_else(|| cuts.first())
        .copied()
        .with_context(|| format!("the versions of `{path}` leave no line to share"))?;
    Ok(cut(versions, pick, separator))
}

/// The split a combined file shows: a cut and separator that reproduce it
/// exactly, with the patches' lines in key order. Several can, since a
/// separator may span lines (a `},` that closes one entry reads as part of
/// it or of the separator): balanced entries win, then a separator on one
/// line, which later patches' lines are recorded against, then the most
/// shared lines.
pub fn from_example(path: &str, versions: &[Version], example: &str) -> Result<Split> {
    check_versions(path, versions)?;
    let want = normalized(example);
    let cuts = cuts(versions);
    let mut names: Vec<&str> = versions.iter().map(|v| v.name.as_str()).collect();
    names.sort_unstable();
    let best = cuts
        .iter()
        .filter_map(|&c| {
            Some(cut(
                versions,
                c,
                &separator_for(versions, c, &names, &want)?,
            ))
        })
        .min_by_key(|split| {
            (
                !split.fills.values().all(|l| balanced(l)),
                split.separator.contains('\n') && !split.separator.trim().is_empty(),
            )
        });
    if let Some(split) = best {
        return Ok(split);
    }
    if names.len() <= 4 {
        let reordered = permutations(&names)
            .into_iter()
            .filter(|order| *order != names)
            .any(|order| {
                cuts.iter()
                    .any(|&c| separator_for(versions, c, &order, &want).is_some())
            });
        if reordered {
            bail!(
                "in `{path}` the patches' lines go in the order of the patches' names ({}), \
                 in every project; put them in that order",
                names
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    let proposal = propose(path, versions)?.combined();
    let (line, found, expected) = first_difference(&want, &proposal);
    bail!(
        "`{path}` as saved is not the shared lines with each patch's own lines between them: \
         line {line} reads `{found}` where weft proposed `{expected}`. Move where the shared \
         lines end or change what separates the patches' lines; to change a patch's own lines, \
         amend that patch first"
    )
}

/// The separator between contributions, if cutting at `c` and putting the
/// contributions in `order` reproduces `want` with one.
fn separator_for(
    versions: &[Version],
    c: (usize, usize),
    order: &[&str],
    want: &str,
) -> Option<String> {
    let mut split = cut(versions, c, "");
    // Everything up to the end of the first patch's lines, which the
    // separator follows.
    let head: Vec<String> = split
        .before
        .iter()
        .chain(&split.fills[order[0]])
        .map(display_line)
        .collect();
    let rest = want.strip_prefix(head.join("\n").as_str())?;
    let ends = rest
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(rest.len()))
        .take(MAX_SEPARATOR + 1);
    for end in ends {
        split.separator = rest[..end].to_owned();
        if split.combined_in(order) == want {
            return Some(split.separator);
        }
    }
    None
}

fn permutations<'a>(items: &[&'a str]) -> Vec<Vec<&'a str>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, first);
            out.push(tail);
        }
    }
    out
}

/// The first line where two texts differ, 1-based, with both readings.
fn first_difference(found: &str, expected: &str) -> (usize, String, String) {
    let found: Vec<&str> = found.lines().collect();
    let expected: Vec<&str> = expected.lines().collect();
    for i in 0..found.len().max(expected.len()) {
        let (f, e) = (
            found.get(i).copied().unwrap_or_default(),
            expected.get(i).copied().unwrap_or_default(),
        );
        if f != e {
            return (i + 1, f.to_owned(), e.to_owned());
        }
    }
    (found.len(), String::new(), String::new())
}

// ---- the files and patches involved --------------------------------------

/// A file a patch creates that patches of the template create too.
#[derive(Clone, Debug)]
pub(crate) struct Clash {
    pub path: String,
    /// The template's patches that create it, with their gates.
    pub others: Vec<(String, Option<String>)>,
}

/// The files `ops` (a patch not yet in `template`) creates that patches of
/// the template create too.
pub(crate) fn clashes(template: &Template, ops: &[Op]) -> Vec<Clash> {
    let mut out = Vec::new();
    for op in ops {
        let Op::CreateFile { path, .. } = op else {
            continue;
        };
        let path = display_path(path);
        let others: Vec<(String, Option<String>)> = template
            .nodes
            .iter()
            .filter_map(|n| Some((n, template.node_patch(n)?)))
            .filter(|(n, p)| {
                p.ops.iter().any(|op| {
                    matches!(op, Op::CreateFile { .. } | Op::CreateBinaryFile { .. })
                        && op_paths(op).iter().any(|tp| placed(n, tp) == path)
                })
            })
            .map(|(n, p)| (n.name.clone(), p.when.as_ref().map(|w| w.0.clone())))
            .collect();
        if !others.is_empty() {
            out.push(Clash { path, others });
        }
    }
    out
}

fn op_paths(op: &Op) -> Vec<&TemplatePath> {
    match op {
        Op::CreateFile { path, .. }
        | Op::CreateBinaryFile { path, .. }
        | Op::ModifyFile { path, .. }
        | Op::DeleteFile { path }
        | Op::SetMode { path, .. }
        | Op::FillSlot { path, .. } => vec![path],
        Op::RenamePath { from, to } => vec![from, to],
    }
}

fn verb(op: &Op) -> &'static str {
    match op {
        Op::CreateFile { .. } => "creates",
        Op::CreateBinaryFile { .. } => "creates as a binary file",
        Op::ModifyFile { .. } => "changes",
        Op::DeleteFile { .. } => "deletes",
        Op::SetMode { .. } => "sets the mode of",
        Op::FillSlot { .. } => "adds lines to a slot of",
        Op::RenamePath { .. } => "renames",
    }
}

/// Where a node's op path lands in the tree, as displayed.
fn placed(node: &Node, path: &TemplatePath) -> String {
    let path = display_path(path);
    match &node.kind {
        NodeKind::Child { mount, .. } if !mount.as_str().is_empty() => {
            mount.join(&path).into_string()
        }
        _ => path,
    }
}

/// A file to share: where it is, its mode, the patches of the template that
/// create it, and every version of it (theirs and a pending patch's).
pub(crate) struct Shared {
    pub path: String,
    template_path: TemplatePath,
    mode: u32,
    creators: Vec<String>,
    pub versions: Vec<Version>,
}

/// A patch being committed that creates some of the shared files.
pub(crate) struct Pending<'a> {
    pub name: &'a str,
    pub ops: &'a [Op],
    pub depends_on: &'a [String],
}

/// Check that each file can be shared, and collect its versions.
pub(crate) fn prepare(
    template: &Template,
    paths: &[String],
    pending: Option<&Pending<'_>>,
) -> Result<Vec<Shared>> {
    let mut out = Vec::new();
    for path in paths {
        let path = path.trim_start_matches("./").to_owned();
        let mut creators = Vec::new();
        let mut versions = Vec::new();
        let mut shape: Option<(TemplatePath, u32)> = None;
        let mut take = |name: &str, tp: &TemplatePath, mode: u32, lines: &[Line]| -> Result<()> {
            match &shape {
                Some((_, m)) if *m != mode => bail!(
                    "the patches create `{path}` with different modes ({m:o} and {mode:o}); \
                     weft shares a file they create alike"
                ),
                Some(_) => {}
                None => shape = Some((tp.clone(), mode)),
            }
            versions.push(Version {
                name: name.to_owned(),
                lines: lines.to_vec(),
            });
            Ok(())
        };
        for node in &template.nodes {
            let Some(patch) = template.node_patch(node) else {
                continue;
            };
            for op in &patch.ops {
                if !op_paths(op).iter().any(|tp| placed(node, tp) == path) {
                    continue;
                }
                let Op::CreateFile {
                    path: tp,
                    content,
                    mode,
                    ..
                } = op
                else {
                    bail!(
                        "patch `{}` {} `{path}`; weft shares a file that patches only create",
                        node.name,
                        verb(op)
                    );
                };
                match &node.kind {
                    NodeKind::Root(_) if template.is_inherited(&node.name) => bail!(
                        "patch `{}` comes from `{}`: share `{path}` there",
                        node.name,
                        template.extends.as_ref().expect("inherited").root
                    ),
                    NodeKind::Root(_) => {}
                    _ => bail!(
                        "patch `{}` belongs to an include: share `{path}` in that template",
                        node.name
                    ),
                }
                if patch.foreach.is_some() {
                    bail!(
                        "patch `{}` renders once per instance (foreach); weft does not share \
                         the files it creates",
                        node.name
                    );
                }
                if patch.meta.generator.is_some() {
                    bail!(
                        "patch `{}` is generated by a command that writes the whole file; detach \
                         it first with `weft patch detach {0}`",
                        node.name
                    );
                }
                take(&node.name, tp, *mode, &content.0)?;
                creators.push(node.name.clone());
            }
        }
        for op in pending.map(|p| p.ops).unwrap_or_default() {
            if let Op::CreateFile {
                path: tp,
                content,
                mode,
                ..
            } = op
            {
                if display_path(tp) == path {
                    take(pending.expect("pending").name, tp, *mode, &content.0)?;
                }
            }
        }
        check_versions(&path, &versions)?;
        for (i, a) in creators.iter().enumerate() {
            for b in &creators[i + 1..] {
                let (ia, ib) = (template.name_to_id[a], template.name_to_id[b]);
                let (upper, lower) = if template.ancestor_closure(ia).contains(&ib) {
                    (a, b)
                } else if template.ancestor_closure(ib).contains(&ia) {
                    (b, a)
                } else {
                    continue;
                };
                bail!(
                    "`{upper}` depends on `{lower}`, so they never create `{path}` side by side; \
                     weft shares a file that independent patches create"
                );
            }
        }
        let (template_path, mode) = shape.expect("two or more versions");
        out.push(Shared {
            path,
            template_path,
            mode,
            creators,
            versions,
        });
    }
    Ok(out)
}

/// Each file's split: from its combined file when one is given, from the
/// author's editor when a person is there, else weft's proposal.
pub(crate) fn confirm(
    shared: &[Shared],
    examples: &BTreeMap<String, String>,
    interaction: &mut dyn Interaction,
) -> Result<Vec<Split>> {
    let mut out = Vec::new();
    for s in shared {
        let split = match examples.get(&s.path) {
            Some(example) => from_example(&s.path, &s.versions, example)?,
            None if interaction.interactive() => edit(s, interaction)?,
            None => propose(&s.path, &s.versions)?,
        };
        out.push(split);
    }
    Ok(out)
}

/// Open the proposed combined file in the author's editor until it splits.
fn edit(shared: &Shared, interaction: &mut dyn Interaction) -> Result<Split> {
    let mut names: Vec<String> = shared
        .versions
        .iter()
        .map(|v| format!("`{}`", v.name))
        .collect();
    names.sort();
    eprintln!(
        "{}: weft opens it in your editor as a project with {} on gets it. Fix it if it is \
         wrong, then save and close.",
        shared.path,
        names.join(" and ")
    );
    let extension = Utf8Path::new(&shared.path)
        .extension()
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let mut text = propose(&shared.path, &shared.versions)?.combined();
    loop {
        let edited = interaction.edit(&text, &extension)?;
        if edited.trim().is_empty() {
            bail!("aborted: `{}` was saved empty", shared.path);
        }
        match from_example(&shared.path, &shared.versions, &edited) {
            Ok(split) => return Ok(split),
            Err(e) => {
                eprintln!("error: {e:#}");
                if !interaction.confirm("open it again?", true)? {
                    bail!("aborted: nothing was shared");
                }
                text = edited;
            }
        }
    }
}

/// The new patch that owns the shared files.
pub(crate) struct Owner {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

/// The owner's name and metadata: as given, else asked when `ask` and a
/// person is there, else a title and description weft writes.
pub(crate) fn owner(
    shared: &[Shared],
    name: Option<&str>,
    title: Option<&str>,
    describe: Option<&str>,
    ask: bool,
    interaction: &mut dyn Interaction,
) -> Result<Owner> {
    let files = shared
        .iter()
        .map(|s| s.path.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let ask = ask && interaction.interactive();
    let name = match name {
        Some(n) => n.to_owned(),
        None if ask => interaction.text(
            &format!("Name of the patch that owns {files}"),
            &default_name(&shared[0].path),
        )?,
        None => bail!("--name is required: the name of the new patch that owns {files}"),
    };
    let default_title = format!("Shared {files}");
    let title = match title {
        Some(t) => t.to_owned(),
        None if ask => interaction.text("Title", &default_title)?,
        None => default_title,
    };
    let description = match describe {
        Some(d) => d.to_owned(),
        None => format!(
            "Holds the lines of {files} that the patches adding to it share. A file is left \
             out while no patch adds to it."
        ),
    };
    Ok(Owner {
        name,
        title: Some(title).filter(|t| !t.is_empty()),
        description: Some(description).filter(|d| !d.is_empty()),
    })
}

/// A patch name from a file's name: `.mcp.json` → `mcp`.
fn default_name(path: &str) -> String {
    let file = Utf8Path::new(path).file_name().unwrap_or(path);
    let stem = file.trim_start_matches('.').split('.').next().unwrap_or("");
    let name: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    if name.is_empty() {
        "shared".to_owned()
    } else {
        name
    }
}

/// Everything sharing writes.
pub(crate) struct Plan {
    pub owner: Owner,
    pub owner_depends_on: Vec<String>,
    pub owner_ops: Vec<Op>,
    /// Patches of the template rewritten to fill the owner's slots, with the
    /// shared files each one fills.
    pub rewritten: Vec<(String, PatchFile, Vec<String>)>,
    /// The pending patch's ops, filling the slots instead of creating files.
    pub pending_ops: Option<Vec<Op>>,
    /// The pending patch's dependencies: the owner, in place of what the
    /// owner already depends on.
    pub pending_depends_on: Option<Vec<String>>,
}

/// Build the owner and the rewritten patches.
pub(crate) fn build(
    template: &Template,
    shared: &[Shared],
    splits: &[Split],
    owner: Owner,
    pending: Option<&Pending<'_>>,
) -> Result<Plan> {
    let valid = !owner.name.is_empty()
        && owner
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if !valid {
        bail!(
            "`{}` is not a valid patch name (ASCII letters, digits, `-` and `_`)",
            owner.name
        );
    }
    if template.name_to_id.contains_key(&owner.name)
        || pending.is_some_and(|p| p.name == owner.name)
    {
        bail!(
            "patch `{}` already exists; name the patch that owns the shared lines differently",
            owner.name
        );
    }
    let owner_ops = shared
        .iter()
        .zip(splits)
        .map(|(s, split)| Op::CreateFile {
            path: s.template_path.clone(),
            omit_when_empty: vec![SLOT.to_owned()],
            content: Content(split.frame()),
            mode: s.mode,
        })
        .collect();
    let to_fill = |name: &str, ops: &[Op]| -> (Vec<Op>, Vec<String>) {
        let mut files = Vec::new();
        let ops = ops
            .iter()
            .map(|op| {
                let Op::CreateFile { path, .. } = op else {
                    return op.clone();
                };
                let display = display_path(path);
                let Some((_, split)) = shared.iter().zip(splits).find(|(s, _)| s.path == display)
                else {
                    return op.clone();
                };
                files.push(display);
                Op::FillSlot {
                    path: path.clone(),
                    slot: SLOT.to_owned(),
                    key: Line::literal(name),
                    lines: split.fills[name].clone(),
                }
            })
            .collect();
        (ops, files)
    };

    // The owner depends on what every patch that created the files did.
    let mut common: Option<Vec<String>> = None;
    let mut intersect = |deps: &[String]| {
        common = Some(match common.take() {
            None => deps.to_vec(),
            Some(prev) => prev.into_iter().filter(|d| deps.contains(d)).collect(),
        });
    };
    let names: BTreeSet<&str> = shared
        .iter()
        .flat_map(|s| s.creators.iter().map(String::as_str))
        .collect();
    let mut rewritten = Vec::new();
    for name in names {
        let mut file = template.patch_file(name)?;
        intersect(&file.depends_on);
        let (ops, files) = to_fill(name, &file.ops);
        file.ops = ops;
        rewritten.push((name.to_owned(), file, files));
    }
    let pending_ops = pending.map(|p| {
        intersect(p.depends_on);
        to_fill(p.name, p.ops).0
    });
    let owner_depends_on = common.unwrap_or_default();
    // Each patch depends on the owner in place of what the owner depends on.
    let rehome = |deps: &[String]| -> Vec<String> {
        let mut out: Vec<String> = deps
            .iter()
            .filter(|d| !owner_depends_on.contains(d) && **d != owner.name)
            .cloned()
            .collect();
        out.push(owner.name.clone());
        out
    };
    for (_, file, _) in &mut rewritten {
        file.depends_on = rehome(&file.depends_on);
    }
    let pending_depends_on = pending.map(|p| rehome(p.depends_on));
    Ok(Plan {
        owner,
        owner_depends_on,
        owner_ops,
        rewritten,
        pending_ops,
        pending_depends_on,
    })
}

/// What sharing wrote, to put back if what follows fails.
pub(crate) struct Written {
    restore: Vec<(Utf8PathBuf, String)>,
    created: Vec<Utf8PathBuf>,
}

impl Written {
    pub(crate) fn undo(self) {
        for (path, text) in self.restore {
            let _ = std::fs::write(&path, text);
        }
        for path in self.created {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Write the owner and rewrite the patches that created the files; on an
/// error, nothing stays written.
pub(crate) fn write(template: &Template, plan: &Plan) -> Result<Written> {
    let mut written = Written {
        restore: Vec::new(),
        created: Vec::new(),
    };
    let result = (|| -> Result<()> {
        template.write_patch_full(
            &plan.owner.name,
            plan.owner_depends_on.clone(),
            None,
            None,
            plan.owner_ops.clone(),
            PatchMeta {
                title: plan.owner.title.clone(),
                description: plan.owner.description.clone(),
                ..PatchMeta::default()
            },
        )?;
        written.created.push(template.patch_path(&plan.owner.name));
        for (name, file, _) in &plan.rewritten {
            let path = template.patch_path(name);
            let before =
                std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
            template.save_patch_file(name, file)?;
            written.restore.push((path, before));
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(written),
        Err(e) => {
            written.undo();
            Err(e)
        }
    }
}

/// What to print about a plan that was written.
pub(crate) fn report(plan: &Plan) -> Vec<String> {
    let owned: Vec<String> = plan
        .owner_ops
        .iter()
        .filter_map(|op| match op {
            Op::CreateFile { path, .. } => Some(display_path(path)),
            _ => None,
        })
        .collect();
    let mut out = vec![format!(
        "created patch `{}`: creates {}, left out while no patch adds to it",
        plan.owner.name,
        owned.join(", ")
    )];
    for (name, _, files) in &plan.rewritten {
        out.push(format!(
            "rewrote patch `{name}`: adds its lines to {}, depends on `{}`",
            files.join(", "),
            plan.owner.name
        ));
    }
    out
}

// ---- at commit -------------------------------------------------------------

/// The question `weft commit` asks a person when the patch creates files
/// other patches create: true to share them.
pub(crate) fn ask(clashes: &[Clash], interaction: &mut dyn Interaction) -> Result<bool> {
    let groups = grouped(clashes);
    let lines: Vec<String> = groups.iter().map(Group::line).collect();
    let together = if groups.len() == 1 && groups[0].others.len() == 1 {
        "Can both be on in the same project?"
    } else {
        "Can they all be on in the same project?"
    };
    let prompt = format!("{}\n{together}", lines.join("\n"));
    let files = if clashes.len() == 1 { "file" } else { "files" };
    let choice = interaction.choose(
        &prompt,
        &[
            &format!("Yes: share the {files}"),
            "No: they are alternatives",
        ],
        0,
    )?;
    Ok(choice == 0)
}

/// The files a patch clashes on with the same other patches.
struct Group<'c> {
    others: &'c [(String, Option<String>)],
    paths: Vec<&'c str>,
}

impl Group<'_> {
    /// `patch `linear` (when use_linear) also creates .mcp.json and x.toml.`
    fn line(&self) -> String {
        let names: Vec<String> = self
            .others
            .iter()
            .map(|(name, when)| match when {
                Some(when) => format!("`{name}` (when {when})"),
                None => format!("`{name}`"),
            })
            .collect();
        let (noun, verb) = if names.len() == 1 {
            ("patch", "creates")
        } else {
            ("patches", "create")
        };
        let paths: Vec<String> = self.paths.iter().map(|p| (*p).to_owned()).collect();
        format!(
            "{noun} {} also {verb} {}.",
            and_list(&names),
            and_list(&paths)
        )
    }
}

/// The clashes grouped by the patches they are with.
fn grouped(clashes: &[Clash]) -> Vec<Group<'_>> {
    let mut out: Vec<Group<'_>> = Vec::new();
    for clash in clashes {
        match out.iter_mut().find(|g| g.others == clash.others.as_slice()) {
            Some(group) => group.paths.push(&clash.path),
            None => out.push(Group {
                others: &clash.others,
                paths: vec![&clash.path],
            }),
        }
    }
    out
}

/// `a`, `a and b`, `a, b and c`.
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What `weft commit` says after committing a patch whose files clash
/// with other patches' files.
pub(crate) fn hint(clashes: &[Clash], why_not: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = grouped(clashes)
        .into_iter()
        .map(|group| {
            let clash = if group.others.len() == 1 {
                "The two clash in a project that has both on"
            } else {
                "They clash in a project that has them all on"
            };
            format!(
                "note: {} {clash}; if they belong together, run `weft share {} --name NAME`",
                group.line(),
                group.paths.join(" ")
            )
        })
        .collect();
    if let Some(reason) = why_not {
        let them = if clashes.len() == 1 { "it" } else { "them" };
        out.push(format!(
            "note: weft cannot share {them} from here: {reason}"
        ));
    }
    out
}

// ---- `weft share` ------------------------------------------------------------

pub struct ShareOptions {
    pub template: Utf8PathBuf,
    /// Files to share, as tree paths (`.mcp.json`).
    pub paths: Vec<String>,
    /// The new patch that owns the shared lines (asked in a terminal).
    pub name: Option<String>,
    pub title: Option<String>,
    pub describe: Option<String>,
    /// Combined files to take the split from, by path, instead of weft's
    /// proposal.
    pub examples: BTreeMap<String, String>,
    /// Report what would be written without writing it.
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
pub struct ShareReport {
    pub owner: String,
    /// The patches that now fill the owner's slots.
    pub rewritten: Vec<String>,
    /// Each shared file as a project with every one of the patches on gets it.
    pub files: BTreeMap<String, String>,
    /// What was written, as `weft share` prints it (empty on a dry run).
    pub lines: Vec<String>,
}

pub fn run(
    opts: &ShareOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<ShareReport> {
    if opts.paths.is_empty() {
        bail!("name the file(s) to share, as paths in the project (`.mcp.json`)");
    }
    // Sharing rewrites patches under any open session's pinned base.
    if !opts.dry_run {
        let open = Session::list(&opts.template)?;
        if !open.is_empty() {
            bail!(
                "session(s) {} are open in `{}`; commit or end them before sharing a file",
                open.iter()
                    .map(|(n, _)| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                opts.template
            );
        }
    }
    let template = Template::load_with(&opts.template, resolver)?;
    let shared = prepare(&template, &opts.paths, None)?;
    let splits = confirm(&shared, &opts.examples, interaction)?;
    let owner = owner(
        &shared,
        opts.name.as_deref(),
        opts.title.as_deref(),
        opts.describe.as_deref(),
        true,
        interaction,
    )?;
    let plan = build(&template, &shared, &splits, owner, None)?;
    let files = shared
        .iter()
        .zip(&splits)
        .map(|(s, split)| (s.path.clone(), split.combined()))
        .collect();
    let rewritten = plan.rewritten.iter().map(|(n, ..)| n.clone()).collect();
    if opts.dry_run {
        return Ok(ShareReport {
            owner: plan.owner.name,
            rewritten,
            files,
            lines: Vec::new(),
        });
    }
    let written = write(&template, &plan)?;
    if let Err(e) = Template::load_with(&opts.template, resolver) {
        written.undo();
        return Err(e.context("the shared patches do not load; nothing was changed"));
    }
    Ok(ShareReport {
        lines: report(&plan),
        owner: plan.owner.name,
        rewritten,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use weft_core::{AnswerId, AnswerSet, Draft, Patch};

    fn lines(text: &[&str]) -> Vec<Line> {
        text.iter().map(|t| Line::literal(t)).collect()
    }

    fn version(name: &str, text: &[&str]) -> Version {
        Version {
            name: name.to_owned(),
            lines: lines(text),
        }
    }

    fn one_line_servers() -> Vec<Version> {
        vec![
            version(
                "linear",
                &[
                    "{",
                    "  \"mcpServers\": {",
                    "    \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" }",
                    "  }",
                    "}",
                ],
            ),
            version(
                "jira",
                &[
                    "{",
                    "  \"mcpServers\": {",
                    "    \"atlassian\": { \"url\": \"https://mcp.atlassian.com/v1/sse\" }",
                    "  }",
                    "}",
                ],
            ),
        ]
    }

    fn multi_line_servers() -> Vec<Version> {
        let server = |name: &str, key: &str, url: &str| {
            version(
                name,
                &[
                    "{",
                    "  \"mcpServers\": {",
                    &format!("    \"{key}\": {{"),
                    "      \"type\": \"http\",",
                    &format!("      \"url\": \"{url}\""),
                    "    }",
                    "  }",
                    "}",
                ],
            )
        };
        vec![
            server("linear", "linear", "https://mcp.linear.app/mcp"),
            server("jira", "atlassian", "https://mcp.atlassian.com/v1/sse"),
        ]
    }

    /// The owner and fills a split writes, rendered with `on` filling.
    fn render(split: &Split, path: &str, on: &[&str], answers: &AnswerSet) -> Option<String> {
        let eval = weft_lang::StarlarkEval;
        let owner = Patch::new(
            vec![],
            None,
            vec![Op::CreateFile {
                path: TemplatePath(vec![Segment::Literal(path.to_owned())]),
                omit_when_empty: vec![SLOT.to_owned()],
                content: Content(split.frame()),
                mode: weft_core::DEFAULT_FILE_MODE,
            }],
        );
        let mut draft = Draft::new();
        draft
            .apply(owner.id, &owner, answers, &eval, Utf8Path::new(""))
            .unwrap();
        for name in on {
            let fill = Patch::new(
                vec![owner.id],
                None,
                vec![Op::FillSlot {
                    path: TemplatePath(vec![Segment::Literal(path.to_owned())]),
                    slot: SLOT.to_owned(),
                    key: Line::literal(name),
                    lines: split.fills[*name].clone(),
                }],
            );
            draft
                .apply(fill.id, &fill, answers, &eval, Utf8Path::new(""))
                .unwrap();
        }
        let tree = draft.finish().unwrap();
        tree.get(Utf8Path::new(path))
            .map(|e| e.content.text().unwrap().to_owned())
    }

    #[test]
    fn one_line_entries_share_the_braces_and_join_with_commas() {
        let versions = one_line_servers();
        let split = propose(".mcp.json", &versions).unwrap();
        assert_eq!(split.before.len(), 2);
        assert_eq!(split.after.len(), 2);
        assert_eq!(split.separator, ",");
        assert_eq!(
            split.combined(),
            "{\n  \"mcpServers\": {\n    \
             \"atlassian\": { \"url\": \"https://mcp.atlassian.com/v1/sse\" },\n    \
             \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" }\n  }\n}\n"
        );
    }

    #[test]
    fn the_proposal_keeps_an_entrys_closing_brace_with_the_entry() {
        // Both files end with `    }`, `  }`, `}`; the longest shared end
        // would take the entry's own brace, leaving invalid JSON.
        let versions = multi_line_servers();
        let split = propose(".mcp.json", &versions).unwrap();
        assert_eq!(split.after.len(), 2, "{:?}", split.after);
        assert_eq!(split.fills["linear"].len(), 4);
        let combined = split.combined();
        assert!(
            combined.contains("\n    },\n    \"linear\": {\n"),
            "{combined}"
        );
        assert_eq!(
            from_example(".mcp.json", &versions, &combined).unwrap(),
            split
        );
    }

    #[test]
    fn the_example_decides_where_the_shared_lines_end() {
        // Without the brackets the proposal can't tell a TOML table's
        // blank line apart: the example says it separates contributions.
        let versions = vec![
            version(
                "linear",
                &[
                    "[mcp_servers.linear]",
                    "url = \"https://mcp.linear.app/mcp\"",
                ],
            ),
            version(
                "jira",
                &[
                    "[mcp_servers.atlassian]",
                    "url = \"https://mcp.atlassian.com/v1/sse\"",
                ],
            ),
        ];
        let example = "[mcp_servers.atlassian]\nurl = \"https://mcp.atlassian.com/v1/sse\"\n\n\
                       [mcp_servers.linear]\nurl = \"https://mcp.linear.app/mcp\"\n";
        let split = from_example(".codex/config.toml", &versions, example).unwrap();
        assert!(split.before.is_empty() && split.after.is_empty());
        assert_eq!(split.separator, "\n");
        assert_eq!(split.combined(), example);
    }

    #[test]
    fn an_example_out_of_name_order_is_refused_with_the_order() {
        let versions = one_line_servers();
        let example = "{\n  \"mcpServers\": {\n    \
                       \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" },\n    \
                       \"atlassian\": { \"url\": \"https://mcp.atlassian.com/v1/sse\" }\n  }\n}\n";
        let err = from_example(".mcp.json", &versions, example).unwrap_err();
        assert!(err.to_string().contains("(`jira`, `linear`)"), "{err}");
    }

    #[test]
    fn an_example_that_changes_a_patchs_lines_is_refused_at_that_line() {
        let versions = one_line_servers();
        let example = "{\n  \"mcpServers\": {\n    \
                       \"atlassian\": { \"url\": \"https://example.com\" },\n    \
                       \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" }\n  }\n}\n";
        let err = from_example(".mcp.json", &versions, example).unwrap_err();
        assert!(err.to_string().contains("line 3 reads"), "{err}");
    }

    #[test]
    fn answer_references_stay_in_the_patches_lines() {
        let entry = |key: &str, id: &str| {
            Line(vec![
                Segment::Literal(format!("    \"{key}\": {{ \"url\": \"")),
                Segment::Answer(AnswerId::from(id)),
                Segment::Literal("\" }".to_owned()),
            ])
        };
        let frame = |entry: Line| vec![Line::literal("{"), entry, Line::literal("}")];
        let versions = vec![
            Version {
                name: "lightdash".into(),
                lines: frame(entry("lightdash", "lightdash_url")),
            },
            Version {
                name: "sentry".into(),
                lines: frame(entry("sentry", "sentry_url")),
            },
        ];
        let split = propose("mcp.json", &versions).unwrap();
        assert!(split.combined().contains("{lightdash_url}"));
        assert_eq!(
            split.fills["lightdash"],
            vec![entry("lightdash", "lightdash_url")]
        );
    }

    #[test]
    fn each_patch_alone_renders_its_own_file_and_together_the_combined_one() {
        let answers = AnswerSet::new();
        for versions in [one_line_servers(), multi_line_servers()] {
            let split = propose(".mcp.json", &versions).unwrap();
            for v in &versions {
                let own: String = v
                    .lines
                    .iter()
                    .map(|l| format!("{}\n", l.as_literal().unwrap()))
                    .collect();
                assert_eq!(
                    render(&split, ".mcp.json", &[&v.name], &answers).unwrap(),
                    own
                );
            }
            let names: Vec<&str> = versions.iter().map(|v| v.name.as_str()).collect();
            assert_eq!(
                render(&split, ".mcp.json", &names, &answers).unwrap(),
                split.combined()
            );
            assert_eq!(render(&split, ".mcp.json", &[], &answers), None, "left out");
        }
    }

    #[test]
    fn default_names_come_from_the_file_name() {
        assert_eq!(default_name(".mcp.json"), "mcp");
        assert_eq!(default_name(".codex/config.toml"), "config");
        assert_eq!(default_name(".gitignore"), "gitignore");
    }
}
