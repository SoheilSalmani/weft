//! `weft patch squash <names…> --into <name>`: collapse several patches into
//! one. The combined patch's ops are the members' ops concatenated in
//! dependency order (already-abstracted, so no answers or re-abstraction are
//! needed); its dependencies are the members' external deps; every external
//! dependent is repointed at the combined patch.
//!
//! Constraints keep the result sound:
//! - the set must be **convex** — no patch outside it may sit on a dependency
//!   path between two members;
//! - all members must share the same gate (`when`) — squashing different
//!   gates would silently change which answers activate the code;
//! - no member may be a **generator** patch (its ops must stay reproducible
//!   from its command) or a **foreach** patch (those must be graph leaves).

use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use weft_core::PatchId;

use crate::template::{PatchFile, Template, PATCHES_DIR};

pub struct SquashOptions {
    /// Members to combine (≥ 2).
    pub names: Vec<String>,
    /// Name of the resulting patch (may reuse a member's name).
    pub into: String,
    /// Optional title for the combined patch (defaults to the tip member's).
    pub title: Option<String>,
}

pub fn squash(
    root: &Utf8Path,
    opts: &SquashOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
) -> Result<()> {
    let template = Template::load_with(root, resolver)?;
    if opts.names.len() < 2 {
        bail!("squash needs at least two patches");
    }
    let name_ok = !opts.into.is_empty()
        && opts
            .into
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if !name_ok {
        bail!("`--into {}` is not a valid patch name", opts.into);
    }

    // Resolve members and their ids.
    let mut member_ids: BTreeSet<PatchId> = BTreeSet::new();
    for name in &opts.names {
        let id = *template
            .name_to_id
            .get(name)
            .with_context(|| format!("no patch `{name}` in this template"))?;
        if template.is_inherited(name) {
            bail!(
                "patch `{name}` is inherited from `{}` — squash it there",
                template.extends.as_ref().expect("inherited").root
            );
        }
        if !template.patches.iter().any(|p| p.id == id) {
            bail!("patch `{name}` belongs to an include — squash it in that template");
        }
        member_ids.insert(id);
    }
    let members: Vec<&weft_core::Patch> = template
        .patches
        .iter()
        .filter(|p| member_ids.contains(&p.id))
        .collect(); // already in topological order

    // Guards.
    for p in &members {
        let name = &template.id_to_name[&p.id];
        if p.meta.generator.is_some() {
            bail!(
                "patch `{name}` is a generator patch and can't be squashed — its ops must \
                 stay reproducible from its command (`weft patch detach {name}` first to \
                 take ownership)"
            );
        }
        if p.foreach.is_some() {
            bail!("patch `{name}` is a foreach patch and must stay a graph leaf — can't squash it");
        }
    }
    // Same gate across all members.
    let gate = members[0].when.clone();
    let gate_key = |p: &&weft_core::Patch| p.when.as_ref().map(|w| w.as_str().to_owned());
    if members
        .iter()
        .any(|p| gate_key(p) != gate.as_ref().map(|w| w.as_str().to_owned()))
    {
        bail!(
            "the patches have different `when` gates; squashing them would change which \
             answers activate the code. Squash only patches that share a gate."
        );
    }

    // Convexity: no patch outside the set may lie on a dependency path
    // between two members (be a descendant of one member and an ancestor of
    // another).
    for p in &template.patches {
        if member_ids.contains(&p.id) {
            continue;
        }
        let anc = template.ancestor_closure(p.id); // includes p
        let below_a_member = member_ids.iter().any(|m| anc.contains(m)); // some member is p's ancestor
        let above_a_member = members
            .iter()
            .any(|m| template.ancestor_closure(m.id).contains(&p.id)); // p is some member's ancestor
        if below_a_member && above_a_member {
            bail!(
                "patch `{}` sits between the squash members in the graph — the selection \
                 must be convex (include everything on the paths between members)",
                template.id_to_name[&p.id]
            );
        }
    }

    // Combined ops = members' ops concatenated in dependency order.
    let ops: Vec<weft_core::Op> = members.iter().flat_map(|p| p.ops.iter().cloned()).collect();

    // Combined deps = members' deps that point outside the set (by name).
    let mut dep_names: Vec<String> = Vec::new();
    for p in &members {
        for dep in &p.depends_on {
            if !member_ids.contains(dep) {
                let dep_name = template.id_to_name[dep].clone();
                if !dep_names.contains(&dep_name) {
                    dep_names.push(dep_name);
                }
            }
        }
    }

    // Combined hooks = union across members (ids must be unique).
    let mut hooks: Vec<weft_core::Hook> = Vec::new();
    for p in &members {
        for h in &p.meta.hooks {
            if hooks.iter().any(|e| e.id == h.id) {
                bail!("hook id `{}` appears in more than one squashed patch", h.id);
            }
            hooks.push(h.clone());
        }
    }

    // The tip (last member in topo order) donates display metadata.
    let tip = members.last().expect("≥2 members");
    let member_names: BTreeSet<&String> = opts.names.iter().collect();
    // `--into` may reuse a member name, but must not clobber an unrelated patch.
    if template.name_to_id.contains_key(&opts.into) && !member_names.contains(&opts.into) {
        bail!(
            "`--into {}` names an existing patch that isn't being squashed",
            opts.into
        );
    }

    // Build the combined patch file.
    let combined = PatchFile {
        title: opts.title.clone().or_else(|| tip.meta.title.clone()),
        description: tip.meta.description.clone(),
        tags: tip.meta.tags.clone(),
        depends_on: dep_names,
        when: gate,
        foreach: None,
        ops,
        hooks,
        generator: None,
    };

    // External dependents of any member get repointed at `into`.
    let mut edits: Vec<(String, PatchFile)> = Vec::new();
    for p in &template.patches {
        if member_ids.contains(&p.id) {
            continue;
        }
        if p.depends_on.iter().any(|d| member_ids.contains(d)) {
            let name = template.id_to_name[&p.id].clone();
            let mut file = template.patch_file(&name)?;
            let mut new_deps: Vec<String> = Vec::new();
            for d in &file.depends_on {
                let repointed = if member_names.contains(d) {
                    opts.into.clone()
                } else {
                    d.clone()
                };
                if !new_deps.contains(&repointed) {
                    new_deps.push(repointed);
                }
            }
            file.depends_on = new_deps;
            edits.push((name, file));
        }
    }

    // Apply as a transaction: snapshot, write, reload-validate, roll back on
    // failure. (Patch dirs live in git, but a squash touches several files at
    // once, so we make it atomic ourselves.)
    let dir = root.join(PATCHES_DIR);
    let snapshot: Vec<(std::path::PathBuf, Option<String>)> = opts
        .names
        .iter()
        .map(|n| dir.join(format!("{n}.json")))
        .chain(edits.iter().map(|(n, _)| dir.join(format!("{n}.json"))))
        .chain(std::iter::once(dir.join(format!("{}.json", opts.into))))
        .map(|p| {
            let p = p.into_std_path_buf();
            let content = std::fs::read_to_string(&p).ok();
            (p, content)
        })
        .collect();
    let restore = || {
        for (p, content) in &snapshot {
            match content {
                Some(c) => {
                    let _ = std::fs::write(p, c);
                }
                None => {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
    };

    let apply = || -> Result<()> {
        // Delete members (except the one whose name `into` reuses).
        for name in &opts.names {
            if name != &opts.into {
                let _ = std::fs::remove_file(dir.join(format!("{name}.json")));
            }
        }
        write_patch_file(&dir, &opts.into, &combined)?;
        for (name, file) in &edits {
            write_patch_file(&dir, name, file)?;
        }
        Ok(())
    };

    apply().inspect_err(|_| restore())?;
    match Template::load_with(root, resolver) {
        Ok(_) => {
            eprintln!(
                "squashed {} into `{}` ({} op(s)); run `weft check` with representative \
                 answers to re-verify",
                opts.names.join(", "),
                opts.into,
                combined.ops.len()
            );
            Ok(())
        }
        Err(e) => {
            restore();
            bail!("squash produced an invalid template ({e:#}); rolled back")
        }
    }
}

fn write_patch_file(dir: &Utf8Path, name: &str, file: &PatchFile) -> Result<()> {
    let path = dir.join(format!("{name}.json"));
    let mut json = serde_json::to_string_pretty(file)?;
    json.push('\n');
    std::fs::write(&path, json).with_context(|| format!("writing {path}"))?;
    Ok(())
}
