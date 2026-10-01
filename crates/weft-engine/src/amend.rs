//! `weft patch amend <name>`: edit a recorded patch's *content* through the
//! record loop instead of hand-editing its JSON. The base is the patch's
//! ancestors; the worktree is seeded with the patch applied, so you edit its
//! result directly. `weft commit` re-derives the patch's ops in place —
//! keeping its name, dependencies, gate, and metadata — which changes its
//! content id (as any content edit does).
//!
//! Amending a patch that others depend on is a rebase: their
//! context-anchored hunks replay over the new content. If the edit doesn't
//! move what a dependent anchors on, it just works; if it does, `weft
//! commit` reports which dependent broke so you can re-record it. Generator
//! patches are refused — their content is owned by their command (`weft
//! patch resync` / `set-command`, or `detach` to take ownership first).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_core::render::ExprEval;
use weft_core::{
    AnswerKind, AnswerSet, Draft, FileData, FileEntry, Line, Op, Patch, PatchId, SlotDecl,
    TemplatePath, Tree,
};
use weft_lang::StarlarkEval;

use crate::interact::Interaction;
use crate::session::{self, Session, SessionMeta};
use crate::template::Template;
use crate::{answers, fsio};

pub struct AmendOptions {
    pub template: Utf8PathBuf,
    /// The patch to amend (file stem).
    pub name: String,
    /// The session to open for the edit; defaults to the patch's name.
    pub session: String,
    pub presets: Vec<String>,
    pub answers: Vec<String>,
    pub answers_file: Option<Utf8PathBuf>,
    /// Discard an existing session instead of erroring.
    pub force: bool,
}

/// Patches that directly depend on `id` (by name), anywhere in the graph.
pub(crate) fn direct_dependents(template: &Template, id: PatchId) -> Vec<String> {
    template.direct_dependents(id)
}

/// Start an amend session: render the target's ancestors, seed the worktree
/// with the target applied on top, and print the worktree path.
pub fn start(
    opts: &AmendOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<Utf8PathBuf> {
    let template = Template::load_with(&opts.template, resolver)?;
    let session_name = &opts.session;
    if Session::exists(&opts.template, session_name) {
        if opts.force {
            Session::load(&opts.template, session_name)?.end(&opts.template, session_name)?;
        } else {
            bail!(
                "session `{session_name}` already exists in `{}`; \
                 finish it with `weft commit`, or pass `--force` to discard it",
                opts.template
            );
        }
    }

    let id = *template
        .name_to_id
        .get(&opts.name)
        .with_context(|| format!("no patch `{}` in this template", opts.name))?;
    let target = match template.node(id).map(|n| &n.kind) {
        Some(crate::template::NodeKind::Root(i)) if !template.is_inherited(&opts.name) => {
            &template.patches[*i]
        }
        Some(crate::template::NodeKind::Root(_)) => bail!(
            "patch `{}` is inherited from `{}` — amend it there",
            opts.name,
            template.extends.as_ref().expect("inherited").root
        ),
        Some(crate::template::NodeKind::Child { path, .. }) => bail!(
            "patch `{}` belongs to include `{}` — amend it in that template",
            opts.name,
            path.join("/")
        ),
        _ => bail!("`{}` is not a patch that can be amended", opts.name),
    };
    if target.meta.generator.is_some() {
        bail!(
            "patch `{}` is a generator patch; its content is owned by its command — \
             use `weft patch resync {0}` to regenerate, `weft patch set-command {0} …` to \
             change the command, or `weft patch detach {0}` to take ownership first",
            opts.name
        );
    }
    if target.foreach.is_some() {
        bail!(
            "patch `{}` is a foreach (integration) patch; amend does not support these yet — \
             re-record it with `weft session new --foreach …`",
            opts.name
        );
    }
    // A worktree holds text, not slots: it shows the lines other patches
    // add to the target's slots (a marker where none does), and commit puts
    // the slot back where those lines are.
    let eval = StarlarkEval;
    // Amending a patch with dependents is a rebase: the dependents' hunks
    // replay over the new content and may conflict. That's handled at commit
    // (a full render surfaces any that break); warn up front.
    let dependents = direct_dependents(&template, id);
    if !dependents.is_empty() {
        eprintln!(
            "note: `{}` has dependents ({}); if your edit moves the content they anchor \
             on, `weft commit` will report which one to re-record.",
            opts.name,
            dependents.join(", ")
        );
    }

    let layered = answers::layered_with_json_full(
        &template,
        &opts.presets,
        opts.answers_file.as_deref(),
        &opts.answers,
        None,
    )?;
    let (parent_provided, child_provided) =
        crate::compose::split_provided(&template, &layered.answers)?;
    let include_provided: BTreeMap<String, weft_core::AnswerSet> = child_provided
        .into_iter()
        .filter(|((inc, _), _)| template.include(inc).is_some_and(|i| !i.decl.repeat))
        .map(|((inc, _), set)| (inc, set))
        .collect();
    let resolved = answers::gather_reviewed(
        &template,
        &parent_provided,
        &layered.constraints,
        &eval,
        interaction,
    )?
    .answers;

    // Base = the target's strict ancestors (its dependency closure minus
    // itself), across frames: a target depending on `web/next-config` needs
    // that include mounted. The worktree is that base with the target
    // applied on top, so the author edits the target's own contribution.
    let mut pinned = template.ancestor_closure(id);
    pinned.remove(&id);
    pinned.retain(|n| {
        template
            .node(*n)
            .and_then(|n| template.node_patch(n))
            .is_some_and(|p| p.foreach.is_none())
    });
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| pinned.contains(&p.id))
        .cloned()
        .collect();
    let mut parts = crate::compose::single_parts(
        &template,
        &resolved,
        &include_provided,
        &BTreeMap::new(),
        &eval,
        interaction,
    )?;
    crate::compose::filter_parts(&mut parts, &pinned);
    let instances: Vec<crate::session::SessionInstance> = parts
        .iter()
        .map(|p| crate::session::SessionInstance {
            include: p.instance.include.clone(),
            answers: crate::start::strip_secrets(&p.instance.answers),
            secrets: crate::new::secret_specs(&p.template.manifest.questions, &p.instance.answers),
        })
        .collect();
    let base = crate::compose::draft_composed(
        weft_core::Draft::new(),
        &base_patches,
        &resolved,
        &parts,
        &eval,
    )
    .context("rendering the patch's ancestors")?;
    let mut seed = base.clone();
    // Force the target on regardless of its gate (we're editing it).
    seed.apply(target.id, target, &resolved, &eval, Utf8Path::new(""))
        .with_context(|| format!("applying patch `{}` to seed the worktree", opts.name))?;
    let shown = contributions(&template, target, &resolved, &eval)?;
    for (node, ops) in &shown {
        seed.apply(
            *node,
            &weft_core::Patch::new(vec![], None, ops.clone()),
            &resolved,
            &eval,
            Utf8Path::new(""),
        )
        .with_context(|| format!("showing the lines other patches add to `{}`", opts.name))?;
    }
    let fillers: Vec<String> = shown
        .iter()
        .filter_map(|(node, _)| template.id_to_name.get(node))
        .map(|n| format!("`{n}`"))
        .collect();
    if !shown.is_empty() {
        eprintln!(
            "note: `{}` has slots; the worktree shows the lines {} add to them. Edit the lines \
             around them; the patches' own lines stay as they are.",
            opts.name,
            if fillers.is_empty() {
                "other patches".to_owned()
            } else {
                fillers.join(", ")
            }
        );
    }
    let base_tree = base.finish().context("rendering the patch's ancestors")?;
    let seed = seed
        .finish()
        .with_context(|| format!("applying patch `{}` to seed the worktree", opts.name))?;

    let worktree = session::default_worktree_dir(&opts.template, session_name);
    std::fs::create_dir_all(&worktree)?;
    fsio::write_tree(&worktree, &seed)?;
    crate::discover::WorktreeLink {
        template: crate::discover::absolute(&opts.template),
        session: session_name.clone(),
    }
    .save(&worktree)?;

    let secret_specs: BTreeMap<_, _> = template
        .manifest
        .questions
        .iter()
        .filter_map(|q| match &q.kind {
            AnswerKind::Secret { source } if resolved.contains(&q.id) => {
                Some((q.id.clone(), source.to_string()))
            }
            _ => None,
        })
        .collect();
    let sess = Session {
        session: SessionMeta {
            base: pinned.into_iter().collect(),
            // The diff target is the base *without* the patch, so commit
            // re-derives the patch's full contribution from the worktree.
            tree_hash: base_tree.hash(),
            worktree: None,
            scope: Vec::new(),
            adopted: false,
        },
        answers: crate::start::strip_secrets(&resolved),
        secrets: secret_specs,
        instances,
        foreach: None,
        generator: None,
        amend: Some(opts.name.clone()),
    };
    sess.save(&opts.template, session_name)?;
    Ok(worktree)
}

/// `weft commit --title/--describe/--tag` in an amend session, applied like
/// `weft patch set`: non-empty replaces, empty clears, tags are appended.
pub(crate) struct MetaEdits<'a> {
    pub title: Option<&'a str>,
    pub describe: Option<&'a str>,
    pub tags: &'a [String],
}

/// Finish an amend session (called by `weft commit`): re-derive the target's
/// ops from the worktree and write them back into its file in place. Keeps
/// the target's name, dependencies, gate and foreach; metadata changes only
/// through `meta`.
#[allow(clippy::too_many_arguments)] // one call site; all of it is the amend's state
pub(crate) fn finish(
    template: &Template,
    template_root: &Utf8Path,
    session: &crate::session::Session,
    session_name: &str,
    target: &str,
    base_patches: &[weft_core::Patch],
    parts: &[crate::compose::ComposedPart<'_>],
    answers: &weft_core::AnswerSet,
    work_tree: &weft_core::Tree,
    ops: Vec<weft_core::Op>,
    // What the worktree showed in the target's slots: replayed with it.
    shown: Vec<(PatchId, Vec<Op>)>,
    meta: &MetaEdits<'_>,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let eval = StarlarkEval;
    let has_dependents = template
        .name_to_id
        .get(target)
        .map(|id| !direct_dependents(template, *id).is_empty())
        .unwrap_or(false);

    // Preserve everything but the ops.
    let mut file = template.patch_file(target)?;
    let dep_ids: Vec<PatchId> = file
        .depends_on
        .iter()
        .map(|n| {
            template
                .name_to_id
                .get(n)
                .copied()
                .with_context(|| format!("unknown dependency `{n}`"))
        })
        .collect::<Result<_>>()?;

    // Replay guard: the amended patch on its base must reproduce the
    // worktree (skipped when its gate is closed under the answers).
    let candidate = weft_core::Patch::new_foreach(
        dep_ids,
        file.when.clone(),
        file.foreach.clone(),
        ops.clone(),
    );
    let candidate_id = candidate.id;
    let mut with_new = base_patches.to_vec();
    with_new.push(candidate);
    with_new.extend(
        shown
            .into_iter()
            .map(|(_, ops)| Patch::new(vec![candidate_id], None, ops)),
    );
    let replayed = crate::compose::render_composed(&with_new, answers, parts, &eval)
        .context("replaying the amended patch against its base")?;
    let gate_open = match &file.when {
        None => true,
        Some(expr) => eval.eval_bool(expr, answers).unwrap_or(true),
    };
    if gate_open && replayed.hash() != work_tree.hash() {
        bail!(
            "replaying the amended patch does not reproduce the worktree \
             (likely an ambiguous hunk context); refusing to write a broken patch"
        );
    }

    let op_count = ops.len();
    file.ops = ops;
    if let Some(title) = meta.title {
        file.title = (!title.is_empty()).then(|| title.to_owned());
    }
    if let Some(describe) = meta.describe {
        file.description = (!describe.is_empty()).then(|| describe.to_owned());
    }
    for tag in meta.tags {
        if !file.tags.contains(tag) {
            file.tags.push(tag.clone());
        }
    }
    template.save_patch_file(target, &file)?;
    session.end(template_root, session_name)?;
    eprintln!("amended patch `{target}` ({op_count} op(s)); its content id changed");

    // Rebase check: if anything depends on the amended patch, its
    // context-anchored hunks may have moved with the new content. A full
    // composed render under the amend answers surfaces a dependent that
    // broke.
    if has_dependents {
        let reloaded = Template::load_with(template_root, resolver)?;
        let full = crate::session::Session {
            session: SessionMeta {
                base: crate::start::pin_base(&reloaded, "latest")?
                    .into_iter()
                    .collect(),
                tree_hash: String::new(),
                worktree: None,
                scope: Vec::new(),
                adopted: false,
            },
            answers: session.answers.clone(),
            secrets: session.secrets.clone(),
            instances: session.instances.clone(),
            foreach: None,
            generator: None,
            amend: None,
        };
        let (root, parts) = crate::commit::session_base(&reloaded, &full, answers, interaction)?;
        if let Err(e) = crate::compose::render_composed(&root, answers, &parts, &eval) {
            bail!(
                "a dependent patch no longer applies after the amend: {e}\n\
                 its context anchored on content that moved — re-record it \
                 (`weft patch amend <name>` or `weft session new --base <name>`), then \
                 `weft check`. The amend to `{target}` is written; fix the dependent to \
                 finish the rebase."
            );
        }
        eprintln!("dependents still apply — no rebase conflicts");
    }
    Ok(())
}

/// The slots a patch declares, as `` `name` in `path` ``. Resync re-derives
/// ops from a command's output, which cannot carry them.
pub(crate) fn declared_slots(patch: &weft_core::Patch) -> Vec<String> {
    let mut out = Vec::new();
    for op in &patch.ops {
        let (path, lines): (_, Vec<&weft_core::Line>) = match op {
            weft_core::Op::CreateFile { path, content, .. } => (path, content.0.iter().collect()),
            weft_core::Op::ModifyFile { path, hunks } => {
                (path, hunks.iter().flat_map(|h| &h.added).collect())
            }
            _ => continue,
        };
        let path = crate::graph::display_path(path);
        out.extend(
            lines
                .into_iter()
                .filter_map(weft_core::Line::as_slot)
                .map(|decl| format!("`{}` in `{path}`", decl.slot)),
        );
    }
    out
}

/// The slots `patch` declares: where each renders, how its path is
/// written, and the declaration.
fn declared(
    patch: &Patch,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Vec<(Utf8PathBuf, TemplatePath, SlotDecl)>> {
    let mut out = Vec::new();
    for op in &patch.ops {
        let (path, lines): (_, Vec<&Line>) = match op {
            Op::CreateFile { path, content, .. } => (path, content.0.iter().collect()),
            Op::ModifyFile { path, hunks } => (path, hunks.iter().flat_map(|h| &h.added).collect()),
            _ => continue,
        };
        for decl in lines.into_iter().filter_map(Line::as_slot) {
            let rendered = weft_core::render::render_path(path, answers, eval)?;
            out.push((rendered, path.clone(), decl.clone()));
        }
    }
    Ok(out)
}

/// What an amend worktree shows in the slots `target` declares, as (node,
/// ops): each root patch's fills of them, and a marker line holding the
/// place of a slot nobody fills.
pub(crate) fn contributions(
    template: &Template,
    target: &Patch,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Vec<(PatchId, Vec<Op>)>> {
    let declared = declared(target, answers, eval)?;
    let mut out: Vec<(PatchId, Vec<Op>)> = Vec::new();
    let mut filled = BTreeSet::new();
    for patch in &template.patches {
        if patch.id == target.id || patch.foreach.is_some() {
            continue;
        }
        let mut ops = Vec::new();
        for op in &patch.ops {
            let Op::FillSlot { path, slot, .. } = op else {
                continue;
            };
            let Ok(rendered) = weft_core::render::render_path(path, answers, eval) else {
                continue;
            };
            if declared
                .iter()
                .any(|(p, _, d)| *p == rendered && d.slot == *slot)
            {
                filled.insert((rendered, slot.clone()));
                ops.push(op.clone());
            }
        }
        if !ops.is_empty() {
            out.push((patch.id, ops));
        }
    }
    for (rendered, path, decl) in &declared {
        if filled.contains(&(rendered.clone(), decl.slot.clone())) {
            continue;
        }
        let ops = vec![Op::FillSlot {
            path: path.clone(),
            slot: decl.slot.clone(),
            // No patch name has a `~`, so the marker's key clashes with none.
            key: Line::literal("~"),
            lines: vec![Line::literal(&format!(
                "⟪slot {}: other patches add their lines here⟫",
                decl.slot
            ))],
        }];
        out.push((Patch::new(vec![], None, ops.clone()).id, ops));
    }
    Ok(out)
}

/// An amend worktree of a patch with slots, with the lines each slot shows
/// swapped for a stand-in line: recording sees the patch's own lines only,
/// and [`Self::restore`] turns the stand-ins back into the slots.
pub(crate) struct StandIn {
    pub tree: Tree,
    /// (stand-in text, the slot line it holds the place of).
    slots: Vec<(String, Line)>,
    /// The target's `omit_when_empty` slot names.
    omit: Vec<String>,
    /// What the worktree shows in the slots, to replay with.
    pub shown: Vec<(PatchId, Vec<Op>)>,
    /// Where each slot's lines are in the worktree, for `weft diff`.
    pub notes: Vec<(Utf8PathBuf, String)>,
}

/// Find each slot's lines in the worktree as `weft patch amend` showed
/// them, and swap them for a stand-in. `None` when `target` has no slots.
pub(crate) fn stand_in(
    template: &Template,
    target: &Patch,
    base_patches: &[Patch],
    parts: &[crate::compose::ComposedPart<'_>],
    answers: &AnswerSet,
    work_tree: &Tree,
) -> Result<Option<StandIn>> {
    let eval = StarlarkEval;
    let declared = declared(target, answers, &eval)?;
    if declared.is_empty() {
        return Ok(None);
    }
    let shown = contributions(template, target, answers, &eval)?;
    // The seed as amend wrote it, traced: where each slot's lines sit.
    let mut seed =
        crate::compose::draft_composed(Draft::traced(), base_patches, answers, parts, &eval)?;
    seed.apply(target.id, target, answers, &eval, Utf8Path::new(""))?;
    for (node, ops) in &shown {
        let patch = Patch::new(vec![], None, ops.clone());
        seed.apply(*node, &patch, answers, &eval, Utf8Path::new(""))?;
    }
    let (seed_tree, trace) = seed.finish_traced()?;
    let mut tree = work_tree.clone();
    let mut slots = Vec::new();
    let mut notes = Vec::new();
    for (i, (path, _, decl)) in declared.iter().enumerate() {
        let span = trace
            .slots(path)
            .iter()
            .find(|s| s.name == decl.slot && s.declared_by == target.id)
            .with_context(|| format!("slot `{}` of `{path}` did not render", decl.slot))?;
        let seed_text = seed_tree
            .get(path)
            .and_then(|e| e.content.text())
            .unwrap_or_default();
        let seed_lines: Vec<&str> = seed_text.lines().collect();
        let block = &seed_lines[span.start..span.end()];
        let names: Vec<String> = span
            .fills
            .iter()
            .filter_map(|f| template.id_to_name.get(&f.node))
            .map(|n| format!("`{n}`"))
            .collect();
        let whose = if names.is_empty() {
            "the marker line".to_owned()
        } else {
            format!("the lines {} add", names.join(", "))
        };
        let Some(entry) = tree.get(path) else {
            bail!(
                "`{path}` is gone from the worktree, but its slot `{}` holds {whose}; remove \
                 those patches first",
                decl.slot
            );
        };
        let text = entry
            .content
            .text()
            .with_context(|| format!("`{path}` is no longer a text file"))?;
        let lines: Vec<&str> = text.lines().collect();
        let at: Vec<usize> = (0..(lines.len() + 1).saturating_sub(block.len()))
            .filter(|&k| lines[k..k + block.len()] == *block)
            .collect();
        let k = match at.as_slice() {
            [k] => *k,
            [] if names.is_empty() => bail!(
                "slot `{}` of `{path}` is marked by the line `{}`, which is no longer in the \
                 file; put it back where other patches' lines go",
                decl.slot,
                block.join("\n")
            ),
            [] => bail!(
                "slot `{}` of `{path}` held {whose}, and they are no longer in the file as they \
                 were:\n{}\nkeep them together and unchanged (to change a patch's lines, amend \
                 that patch)",
                decl.slot,
                block.join("\n")
            ),
            _ => bail!(
                "the lines slot `{}` of `{path}` holds appear more than once in the file; keep \
                 one copy",
                decl.slot
            ),
        };
        let stand = format!("{}\u{1}", "\u{0}".repeat(i + 1));
        let mut new_lines: Vec<String> = lines[..k].iter().map(|s| (*s).to_owned()).collect();
        new_lines.push(stand.clone());
        new_lines.extend(lines[k + block.len()..].iter().map(|s| (*s).to_owned()));
        let mode = entry.mode;
        tree.insert(
            path.clone(),
            FileEntry {
                content: FileData::Text(weft_core::join_lines(&new_lines)),
                mode,
            },
        );
        slots.push((stand, Line::slot(decl.clone())));
        let range = match block.len() {
            1 => format!("line {}", k + 1),
            n => format!("lines {}-{}", k + 1, k + n),
        };
        notes.push((
            path.clone(),
            if names.is_empty() {
                format!(
                    "{range}: the place of slot `{}`, where other patches add their lines; \
                     keep the line",
                    decl.slot
                )
            } else {
                format!(
                    "{range}: the lines {} add to slot `{}`, which stay theirs",
                    names.join(", "),
                    decl.slot
                )
            },
        ));
    }
    let omit = target
        .ops
        .iter()
        .flat_map(|op| match op {
            Op::CreateFile {
                omit_when_empty, ..
            } => omit_when_empty.clone(),
            _ => Vec::new(),
        })
        .collect();
    Ok(Some(StandIn {
        tree,
        slots,
        omit,
        shown,
        notes,
    }))
}

impl StandIn {
    /// Turn the stand-in lines in recorded `ops` back into the slots they
    /// hold the place of, and give each file its `omit_when_empty` back.
    pub(crate) fn restore(&self, mut ops: Vec<Op>) -> Result<Vec<Op>> {
        let mut placed = 0usize;
        let mut swap = |lines: &mut Vec<Line>| {
            let mut names = Vec::new();
            for line in lines.iter_mut() {
                let text = line.as_literal();
                if let Some((_, slot)) = self.slots.iter().find(|(s, _)| Some(s) == text.as_ref()) {
                    names.push(slot.as_slot().expect("a slot line").slot.clone());
                    *line = slot.clone();
                    placed += 1;
                }
            }
            names
        };
        for op in &mut ops {
            match op {
                Op::CreateFile {
                    content,
                    omit_when_empty,
                    ..
                } => {
                    let names = swap(&mut content.0);
                    *omit_when_empty = self
                        .omit
                        .iter()
                        .filter(|n| names.contains(n))
                        .cloned()
                        .collect();
                }
                Op::ModifyFile { hunks, .. } => {
                    for hunk in hunks {
                        swap(&mut hunk.added);
                    }
                }
                _ => {}
            }
        }
        if placed != self.slots.len() {
            bail!(
                "the recording lost a slot: its lines ended up where weft cannot keep a slot; \
                 keep each slot's lines on lines of their own"
            );
        }
        Ok(ops)
    }
}
