//! Authoring edits that previously required hand-editing patch JSON:
//! hook add/remove/list and patch metadata (title/description/tags).
//!
//! All edits here touch only patch *metadata* (hooks, title, description,
//! tags), which is excluded from content addressing — patch ids never
//! change. Every write is validated by reloading the template (and, for
//! hooks, re-running the same static validation `weft check` uses); on
//! failure the original file is restored.

use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use weft_core::{Hook, HookEffect, HookId, HookInput, HookPhase, StarlarkExpr};

use crate::hooks;
use crate::template::Template;

pub struct HookAddOptions {
    pub patch: String,
    pub id: String,
    /// `pre` | `post`
    pub phase: String,
    /// `check` | `setup` | `deploy`
    pub effect: String,
    pub label: String,
    pub action: String,
    pub description: Option<String>,
    pub when: Option<String>,
    pub after: Vec<String>,
    /// `glob:…`, `answer:…`, `hook:…`
    pub inputs: Vec<String>,
}

fn parse_phase(s: &str) -> Result<HookPhase> {
    match s {
        "pre" => Ok(HookPhase::Pre),
        "post" => Ok(HookPhase::Post),
        _ => bail!("invalid phase {s:?} (pre | post)"),
    }
}

fn parse_effect(s: &str) -> Result<HookEffect> {
    match s {
        "check" => Ok(HookEffect::Check),
        "setup" => Ok(HookEffect::Setup),
        "deploy" => Ok(HookEffect::Deploy),
        _ => bail!("invalid effect {s:?} (check | setup | deploy)"),
    }
}

/// Apply a metadata-only edit to one patch file, then verify the template
/// still loads and (optionally) that hook validation stays clean. Restores
/// the original file on any failure.
fn edit_patch_file(
    root: &Utf8Path,
    patch: &str,
    edit: impl FnOnce(&mut crate::template::PatchFile) -> Result<()>,
) -> Result<()> {
    let template = Template::load(root)?;
    let mut file = template.patch_file(patch)?;
    let original = std::fs::read_to_string(template.patch_path(patch))?;
    edit(&mut file)?;
    template.save_patch_file(patch, &file)?;

    let restore = |reason: String| -> Result<()> {
        std::fs::write(template.patch_path(patch), &original)
            .context("restoring the original patch file")?;
        bail!("{reason}")
    };
    let reloaded = match Template::load(root) {
        Ok(t) => t,
        Err(e) => return restore(format!("edit produced an invalid template: {e:#}")),
    };
    let issues = hooks::validate_all(&reloaded);
    if !issues.is_empty() {
        return restore(format!(
            "edit failed hook validation:\n  {}",
            issues.join("\n  ")
        ));
    }
    Ok(())
}

/// `weft hook add`: append a hook to a patch's metadata.
pub fn hook_add(root: &Utf8Path, opts: &HookAddOptions) -> Result<()> {
    let hook = Hook {
        id: HookId(opts.id.clone()),
        phase: parse_phase(&opts.phase)?,
        effect: parse_effect(&opts.effect)?,
        label: opts.label.clone(),
        description: opts.description.clone(),
        action: weft_core::Command::literal(&opts.action),
        when: opts.when.clone().map(StarlarkExpr),
        after: opts.after.iter().cloned().map(HookId).collect(),
        inputs: opts
            .inputs
            .iter()
            .map(|s| s.parse::<HookInput>())
            .collect::<Result<_, _>>()
            .map_err(|e| anyhow::anyhow!("invalid --input: {e}"))?,
    };
    edit_patch_file(root, &opts.patch, |file| {
        if file.hooks.iter().any(|h| h.id == hook.id) {
            bail!("patch `{}` already has a hook `{}`", opts.patch, hook.id);
        }
        file.hooks.push(hook);
        Ok(())
    })?;
    eprintln!("added hook `{}` to patch `{}`", opts.id, opts.patch);
    Ok(())
}

/// `weft hook rm`: remove a hook from a patch by id.
pub fn hook_rm(root: &Utf8Path, patch: &str, id: &str) -> Result<()> {
    edit_patch_file(root, patch, |file| {
        let before = file.hooks.len();
        file.hooks.retain(|h| h.id.0 != id);
        if file.hooks.len() == before {
            bail!("patch `{patch}` has no hook `{id}`");
        }
        Ok(())
    })?;
    eprintln!("removed hook `{id}` from patch `{patch}`");
    Ok(())
}

/// `weft hook ls`: every hook across the template, in execution order.
pub fn hook_ls(root: &Utf8Path) -> Result<()> {
    let template = Template::load(root)?;
    let mut count = 0;
    for phase in [HookPhase::Pre, HookPhase::Post] {
        for patch in &template.patches {
            for hook in patch.meta.hooks.iter().filter(|h| h.phase == phase) {
                let phase_s = match phase {
                    HookPhase::Pre => "pre",
                    HookPhase::Post => "post",
                };
                let effect = match hook.effect {
                    HookEffect::Check => "check",
                    HookEffect::Setup => "setup",
                    HookEffect::Deploy => "deploy",
                };
                let gate = hook
                    .when
                    .as_ref()
                    .map(|w| format!(" when={}", w.as_str()))
                    .unwrap_or_default();
                println!(
                    "{phase_s:4} {effect:6} {id:20} [{patch}] {label}{gate}",
                    id = hook.id.0,
                    patch = template.id_to_name[&patch.id],
                    label = hook.label,
                );
                count += 1;
            }
        }
    }
    if count == 0 {
        eprintln!("no hooks declared in this template");
    }
    Ok(())
}

pub struct PatchSetOptions {
    pub name: String,
    pub title: Option<String>,
    pub describe: Option<String>,
    pub tags: Vec<String>,
    pub clear_tags: bool,
}

/// `weft patch set`: edit a patch's display metadata (never its id).
pub fn patch_set(root: &Utf8Path, opts: &PatchSetOptions) -> Result<()> {
    if opts.title.is_none() && opts.describe.is_none() && opts.tags.is_empty() && !opts.clear_tags {
        bail!("nothing to change; pass --title, --describe, --tag, or --clear-tags");
    }
    edit_patch_file(root, &opts.name, |file| {
        if let Some(title) = &opts.title {
            file.title = (!title.is_empty()).then(|| title.clone());
        }
        if let Some(describe) = &opts.describe {
            file.description = (!describe.is_empty()).then(|| describe.clone());
        }
        if opts.clear_tags {
            file.tags.clear();
        }
        for tag in &opts.tags {
            if !file.tags.contains(tag) {
                file.tags.push(tag.clone());
            }
        }
        Ok(())
    })?;
    eprintln!("updated metadata of patch `{}`", opts.name);
    Ok(())
}

/// `weft patch ls`: one line per patch, in dependency order.
pub fn patch_ls(root: &Utf8Path) -> Result<()> {
    let template = Template::load(root)?;
    for patch in &template.patches {
        let name = &template.id_to_name[&patch.id];
        let title = patch.meta.title.as_deref().unwrap_or("");
        let mut flags = Vec::new();
        if let Some(w) = &patch.when {
            flags.push(format!("when={}", w.as_str()));
        }
        if let Some(f) = &patch.foreach {
            flags.push(format!("foreach={f}"));
        }
        let flags = if flags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", flags.join(", "))
        };
        println!(
            "{name:20} {ops} op(s), {hooks} hook(s){flags}  {title}",
            ops = patch.ops.len(),
            hooks = patch.meta.hooks.len(),
        );
    }
    Ok(())
}
