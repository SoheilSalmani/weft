//! Per-command completion forms: when required options are missing in a
//! terminal, these open a prefilled full-screen form instead of erroring,
//! then hand back the completed option structs. Every submitted form echoes
//! the equivalent flag invocation so the TUI teaches the CLI.

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_engine::author::{HookAddOptions, PatchSetOptions};
use weft_engine::template::Template;

use crate::tui::form::{Choice, Field, FormState};
use crate::tui::{echo_command, shell_quote};

fn slug(value: &str) -> Option<String> {
    let ok = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'));
    (!ok).then(|| "lowercase alphanumerics, - and _".to_owned())
}

fn hook_input(value: &str) -> Option<String> {
    let ok = value.strip_prefix("glob:").is_some()
        || value.strip_prefix("answer:").is_some()
        || value.strip_prefix("hook:").is_some();
    (!ok).then(|| "must start with glob:, answer: or hook:".to_owned())
}

fn patch_choices(template: &Template) -> Vec<Choice> {
    template
        .patches
        .iter()
        .map(|p| {
            let name = template.id_to_name[&p.id].clone();
            let mut choice = Choice::new(name);
            if let Some(title) = &p.meta.title {
                choice = choice.help(title.clone());
            }
            choice
        })
        .collect()
}

fn existing_hook_ids(template: &Template) -> Vec<Choice> {
    template
        .patches
        .iter()
        .flat_map(|p| p.meta.hooks.iter())
        .map(|h| Choice::new(h.id.0.clone()).help(h.label.clone()))
        .collect()
}

/// `weft hook add` with missing flags → form → completed options.
#[allow(clippy::too_many_arguments)]
pub fn hook_add(
    root: &Utf8Path,
    patch: Option<String>,
    id: Option<String>,
    phase: Option<String>,
    effect: Option<String>,
    label: Option<String>,
    action: Option<String>,
    description: Option<String>,
    when: Option<String>,
    after: Vec<String>,
    inputs: Vec<String>,
) -> Result<HookAddOptions> {
    let template = crate::source::load_template(root)?;
    if template.patches.is_empty() {
        bail!("template has no patches yet; record one first (`weft session new`)");
    }
    let fields = vec![
        Field::select("patch", "Patch", patch_choices(&template), patch.as_deref())
            .required()
            .help("The patch that owns this hook — it runs when that patch applies."),
        Field::text("id", "Hook id", id)
            .required()
            .validate(slug)
            .placeholder("verify-go")
            .help("Unique across the template; referenced by --after and hook: inputs."),
        Field::select(
            "phase",
            "Phase",
            vec![
                Choice::new("pre").help("guard — runs before any file is written"),
                Choice::new("post").help("runs after the tree is written"),
            ],
            phase.as_deref(),
        )
        .required()
        .help("Pre hooks abort the scaffold on failure; post hooks finish setup."),
        Field::select(
            "effect",
            "Effect",
            vec![
                Choice::new("check").help("read-only verification"),
                Choice::new("setup").help("idempotent local mutation"),
                Choice::new("deploy").help("external / irreversible"),
            ],
            effect.as_deref(),
        )
        .required()
        .help("The blast radius, surfaced to agents and CI."),
        Field::text("label", "Label", label)
            .required()
            .placeholder("Verify the Go toolchain is installed")
            .help("Short human sentence shown while the hook runs."),
        Field::text("action", "Action", action)
            .required()
            .placeholder("command -v go")
            .help("Shell command to execute."),
        Field::text("description", "Description", description)
            .help("Optional longer explanation (shown in describe)."),
        Field::text("when", "When (gate)", when)
            .placeholder("use_docker")
            .help("Optional Starlark gate; the hook only runs when truthy."),
        Field::multi_select("after", "Run after", existing_hook_ids(&template), &after)
            .help("Hooks that must run before this one (ordering edges)."),
        Field::string_list("inputs", "Inputs", inputs)
            .validate(hook_input)
            .help("Post-only re-fire triggers on update: glob:PATTERN, answer:ID, hook:ID."),
    ];
    let state = crate::tui::form::run(
        FormState::new(
            "weft hook add",
            &format!("template {}", template.manifest.template.name),
            fields,
        )
        .submit_label("Add hook"),
    )?;

    let opts = HookAddOptions {
        patch: state.select_value("patch").context("patch is required")?,
        id: state.text_value("id").context("id is required")?,
        phase: state.select_value("phase").context("phase is required")?,
        effect: state.select_value("effect").context("effect is required")?,
        label: state.text_value("label").context("label is required")?,
        action: state.text_value("action").context("action is required")?,
        description: state.text_value("description"),
        when: state.text_value("when"),
        after: state.multi_values("after"),
        inputs: state.list_values("inputs"),
    };

    let mut cmd = vec![
        "weft".into(),
        "hook".into(),
        "add".into(),
        opts.patch.clone(),
        "--id".into(),
        opts.id.clone(),
        "--phase".into(),
        opts.phase.clone(),
        "--effect".into(),
        opts.effect.clone(),
        "--label".into(),
        shell_quote(&opts.label),
        "--action".into(),
        shell_quote(&opts.action),
    ];
    if let Some(d) = &opts.description {
        cmd.extend(["--description".into(), shell_quote(d)]);
    }
    if let Some(w) = &opts.when {
        cmd.extend(["--when".into(), shell_quote(w)]);
    }
    for a in &opts.after {
        cmd.extend(["--after".into(), a.clone()]);
    }
    for i in &opts.inputs {
        cmd.extend(["--input".into(), shell_quote(i)]);
    }
    echo_command(&cmd);
    Ok(opts)
}

/// `weft patch set` with nothing to change (or no patch) → form.
pub fn patch_set(
    root: &Utf8Path,
    name: Option<String>,
    title: Option<String>,
    describe: Option<String>,
    tags: Vec<String>,
    clear_tags: bool,
) -> Result<PatchSetOptions> {
    let template = crate::source::load_template(root)?;
    if template.patches.is_empty() {
        bail!("template has no patches yet; record one first (`weft session new`)");
    }
    // Prefill the current metadata of the chosen patch when it's known.
    let (title, describe, tags) = match &name {
        Some(n) => {
            let file = template.patch_file(n)?;
            (
                title.or(file.title),
                describe.or(file.description),
                if tags.is_empty() { file.tags } else { tags },
            )
        }
        None => (title, describe, tags),
    };
    let fields = vec![
        Field::select("name", "Patch", patch_choices(&template), name.as_deref())
            .required()
            .help("The patch whose display metadata to edit (ids never change)."),
        Field::text("title", "Title", title)
            .placeholder("Add Docker support")
            .help("Display title shown by UIs instead of the kebab-case name."),
        Field::text("describe", "Description", describe)
            .help("What this patch does — metadata for humans and agents."),
        Field::string_list("tags", "Tags", tags).help("Free-form labels."),
        Field::toggle("clear_tags", "Replace tags", clear_tags)
            .help("On: the tags above replace the existing set. Off: they append."),
    ];
    let state = crate::tui::form::run(
        FormState::new(
            "weft patch set",
            &format!("template {}", template.manifest.template.name),
            fields,
        )
        .submit_label("Save metadata"),
    )?;

    let opts = PatchSetOptions {
        name: state.select_value("name").context("patch is required")?,
        // Empty text clears nothing here; only explicit values are applied.
        title: state.text_value("title"),
        describe: state.text_value("describe"),
        tags: state.list_values("tags"),
        clear_tags: state.toggle_value("clear_tags"),
    };
    let mut cmd = vec![
        "weft".into(),
        "patch".into(),
        "set".into(),
        opts.name.clone(),
    ];
    if let Some(t) = &opts.title {
        cmd.extend(["--title".into(), shell_quote(t)]);
    }
    if let Some(d) = &opts.describe {
        cmd.extend(["--describe".into(), shell_quote(d)]);
    }
    for t in &opts.tags {
        cmd.extend(["--tag".into(), shell_quote(t)]);
    }
    if opts.clear_tags {
        cmd.push("--clear-tags".into());
    }
    echo_command(&cmd);
    Ok(opts)
}

/// The completed inputs of a `weft commit` form.
pub struct CommitForm {
    pub name: String,
    pub title: Option<String>,
    pub describe: Option<String>,
    pub when: Option<String>,
    pub tags: Vec<String>,
}

/// `weft commit` without `--name` → form, prefilled with the default name.
pub fn commit(
    root: &Utf8Path,
    title: Option<String>,
    describe: Option<String>,
    when: Option<String>,
    tags: Vec<String>,
) -> Result<CommitForm> {
    let template = crate::source::load_template(root)?;
    let default_name = format!("patch-{:03}", template.patches.len() + 1);
    let fields = vec![
        Field::text("name", "Patch name", Some(default_name))
            .required()
            .validate(slug)
            .help("The patch's file name (patches/<name>.json)."),
        Field::text("title", "Title", title)
            .placeholder("Add Docker support")
            .help("Display title — say what the patch does, imperatively."),
        Field::text("describe", "Description", describe)
            .help("Optional longer context for humans and agents."),
        Field::text("when", "When (gate)", when)
            .placeholder("use_docker")
            .help("Optional Starlark gate making the whole patch conditional."),
        Field::string_list("tags", "Tags", tags).help("Free-form labels."),
    ];
    let state = crate::tui::form::run(
        FormState::new(
            "weft commit",
            &format!("template {}", template.manifest.template.name),
            fields,
        )
        .submit_label("Commit patch"),
    )?;

    let form = CommitForm {
        name: state.text_value("name").context("name is required")?,
        title: state.text_value("title"),
        describe: state.text_value("describe"),
        when: state.text_value("when"),
        tags: state.list_values("tags"),
    };
    let mut cmd = vec![
        "weft".into(),
        "commit".into(),
        "--name".into(),
        form.name.clone(),
    ];
    if let Some(t) = &form.title {
        cmd.extend(["--title".into(), shell_quote(t)]);
    }
    if let Some(d) = &form.describe {
        cmd.extend(["--describe".into(), shell_quote(d)]);
    }
    if let Some(w) = &form.when {
        cmd.extend(["--when".into(), shell_quote(w)]);
    }
    for t in &form.tags {
        cmd.extend(["--tag".into(), shell_quote(t)]);
    }
    echo_command(&cmd);
    Ok(form)
}

/// Templates under `dir` (itself, or immediate subdirectories).
pub fn discover_templates(dir: &Utf8Path) -> Vec<(String, Utf8PathBuf)> {
    let mut found = Vec::new();
    if dir.join("weft.toml").is_file() {
        found.push((".".to_owned(), dir.to_owned()));
    }
    if let Ok(entries) = dir.read_dir_utf8() {
        let mut dirs: Vec<_> = entries
            .flatten()
            .filter(|e| e.path().join("weft.toml").is_file())
            .collect();
        dirs.sort_by_key(|e| e.file_name().to_owned());
        for entry in dirs {
            found.push((entry.file_name().to_owned(), entry.path().to_owned()));
        }
    }
    found
}

/// The completed inputs of a `weft new` picker.
pub struct NewForm {
    pub template: Utf8PathBuf,
    pub dest: Utf8PathBuf,
    pub presets: Vec<String>,
}

/// `weft new` without a template → picker (template, destination, presets).
/// The answers wizard runs afterwards as usual.
pub fn new_picker(dest: &Utf8Path, presets: &[String]) -> Result<NewForm> {
    let cwd = Utf8PathBuf::from(".");
    let templates = discover_templates(&cwd);
    if templates.is_empty() {
        bail!(
            "no weft templates found here (or in immediate subdirectories); \
             pass a template path: weft new <template> <dest>"
        );
    }
    let options: Vec<Choice> = templates
        .iter()
        .map(|(name, path)| {
            let mut choice = Choice::labeled(path.as_str(), name.clone());
            // Only the template's own manifest: no graph load, so listing
            // never fetches a remote `extends`/include.
            let manifest = std::fs::read_to_string(path.join(weft_engine::template::MANIFEST_FILE))
                .ok()
                .and_then(|src| toml::from_str::<weft_engine::manifest::Manifest>(&src).ok());
            if let Some(manifest) = manifest {
                if let Some(d) = &manifest.template.description {
                    choice = choice.help(d.lines().next().unwrap_or_default().to_owned());
                }
            }
            choice
        })
        .collect();

    let fields = vec![
        Field::select("template", "Template", options, None)
            .required()
            .help("Templates found in the current directory."),
        Field::text("dest", "Destination", Some(dest.to_string()))
            .required()
            .help("Directory to scaffold into (must be empty or absent)."),
        Field::text("presets", "Presets", Some(presets.join(",")))
            .placeholder("comma-separated preset names")
            .help("Optional presets to layer under your answers."),
    ];
    let state = crate::tui::form::run(
        FormState::new("weft new", "pick a template to scaffold", fields)
            .submit_label("Continue to answers"),
    )?;

    let form = NewForm {
        template: Utf8PathBuf::from(
            state
                .select_value("template")
                .context("template is required")?,
        ),
        dest: Utf8PathBuf::from(state.text_value("dest").context("dest is required")?),
        presets: state
            .text_value("presets")
            .map(|p| {
                p.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
    };
    let mut cmd = vec![
        "weft".into(),
        "new".into(),
        shell_quote(form.template.as_str()),
        shell_quote(form.dest.as_str()),
    ];
    for p in &form.presets {
        cmd.extend(["--preset".into(), p.clone()]);
    }
    echo_command(&cmd);
    Ok(form)
}

/// The completed inputs of an `weft instance add` form.
pub struct InstanceForm {
    pub include: String,
    pub key: String,
}

/// `weft instance add` without include/key → form over the project's
/// repeatable includes.
pub fn instance_add(
    dest: &Utf8Path,
    include: Option<String>,
    key: Option<String>,
) -> Result<InstanceForm> {
    let state_file = weft_engine::state::State::load(dest)
        .with_context(|| format!("`{dest}` is not a weft-scaffolded project"))?;
    let located = crate::source::locate_project(&state_file, None, false)?;
    let mut resolver =
        crate::source::RemoteResolver::new(crate::hub::registry_url(None).ok(), false);
    let template = Template::load_with(&located.dir, &mut resolver)?;
    let repeatable: Vec<Choice> = template
        .includes
        .iter()
        .filter(|inc| inc.decl.repeat)
        .map(|inc| {
            Choice::new(inc.decl.name.clone()).help(format!(
                "{} mounted at {}",
                inc.template.manifest.template.name, inc.decl.path
            ))
        })
        .collect();
    if repeatable.is_empty() {
        bail!(
            "template `{}` declares no repeatable includes",
            template.manifest.template.name
        );
    }

    let fields = vec![
        Field::select("include", "Include", repeatable, include.as_deref())
            .required()
            .help("The repeatable slot this instance fills."),
        Field::text("key", "Instance key", key)
            .required()
            .validate(slug)
            .placeholder("billing")
            .help("Names the instance and its mount directory."),
    ];
    let form_state = crate::tui::form::run(
        FormState::new("weft instance add", &format!("project {dest}"), fields)
            .submit_label("Add instance"),
    )?;

    let form = InstanceForm {
        include: form_state
            .select_value("include")
            .context("include is required")?,
        key: form_state.text_value("key").context("key is required")?,
    };
    echo_command(&[
        "weft".into(),
        "instance".into(),
        "add".into(),
        form.include.clone(),
        form.key.clone(),
        "--dest".into(),
        shell_quote(dest.as_str()),
    ]);
    Ok(form)
}
