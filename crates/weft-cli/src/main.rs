mod diffcmd;
mod forms;
mod hub;
mod lsp;
mod mcp;
mod schema;
mod tui;
mod wizard;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use weft_engine::interact::auto_interaction;
use weft_engine::new::NewOptions;
use weft_engine::template::Template;

#[derive(Parser)]
#[command(name = "weft", version, about = "Record-based project scaffolding")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a template into a destination directory. Run without a
    /// template in a terminal to pick one interactively.
    New {
        /// Path to the template (a directory containing weft.toml). Omit it
        /// (in a terminal) to pick from templates found here.
        template: Option<Utf8PathBuf>,
        /// Destination directory (must be empty or absent). Defaults to `.`.
        #[arg(default_value = ".")]
        dest: Utf8PathBuf,
        /// Apply a named preset (repeatable; presets lock what they answer).
        #[arg(long = "preset")]
        presets: Vec<String>,
        /// Answer a question inline as KEY=VALUE (repeatable).
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// TOML file with answers.
        #[arg(long = "answers-file")]
        answers_file: Option<Utf8PathBuf>,
        /// Answers as a JSON object (inline, `@file`, or `-` for stdin);
        /// highest precedence before prompts. Designed for agents.
        #[arg(long = "answers-json")]
        answers_json: Option<String>,
        /// Declare an instance of a repeatable include as INCLUDE=KEY
        /// (repeatable). Providing any `include.key.answer=…` answer also
        /// declares the instance implicitly.
        #[arg(long = "instance")]
        instances: Vec<String>,
        /// Do not run template tasks after scaffolding.
        #[arg(long)]
        skip_tasks: bool,
        /// Never prompt; fail if answers are missing.
        #[arg(long)]
        non_interactive: bool,
        /// Use sequential prompts instead of the full-screen wizard.
        #[arg(long)]
        no_wizard: bool,
        /// Never open the interactive template picker.
        #[arg(long)]
        no_tui: bool,
        /// Fail if a hub include isn't already pinned in weft.lock (CI).
        #[arg(long)]
        frozen: bool,
    },
    /// Re-render against the current template state and 3-way merge the
    /// changes over local edits.
    Update {
        /// Scaffolded project directory (defaults to `.`).
        #[arg(default_value = ".")]
        dest: Utf8PathBuf,
        /// Print the plan without changing anything.
        #[arg(long)]
        dry_run: bool,
        /// Use this template path instead of the one stored in state.
        #[arg(long)]
        template: Option<Utf8PathBuf>,
        /// Do not run template tasks after merging.
        #[arg(long)]
        skip_tasks: bool,
        /// Never prompt; fail if new questions lack answers.
        #[arg(long)]
        non_interactive: bool,
        /// Fail if a hub include isn't already pinned in weft.lock (CI).
        #[arg(long)]
        frozen: bool,
    },
    /// Manage repeatable-include instances of a scaffolded project.
    Instance {
        #[command(subcommand)]
        cmd: InstanceCmd,
    },
    /// Create a blank template skeleton (weft.toml + patches/).
    Init {
        /// Directory to initialize (created if absent). Defaults to `.`.
        #[arg(default_value = ".")]
        dir: Utf8PathBuf,
        /// Template name (defaults to the directory name).
        #[arg(long)]
        name: Option<String>,
    },
    /// Manage patch hooks without editing patch JSON (hooks are metadata —
    /// patch ids never change).
    Hook {
        #[command(subcommand)]
        cmd: HookCmd,
    },
    /// Inspect and edit patch metadata (title, description, tags).
    Patch {
        #[command(subcommand)]
        cmd: PatchCmd,
    },
    /// Interact with a Weft Hub registry (WEFT_HUB_URL or --registry).
    Hub {
        #[command(subcommand)]
        cmd: HubCmd,
    },
    /// Resolve `hub:` includes and write/refresh weft.lock.
    Lock {
        /// Template directory (defaults to `.`).
        #[arg(default_value = ".")]
        template: Utf8PathBuf,
        /// Bump every include to the newest version satisfying its
        /// requirement (default only fills missing entries).
        #[arg(long)]
        upgrade: bool,
        /// Registry base URL (default: WEFT_HUB_URL).
        #[arg(long)]
        registry: Option<String>,
    },
    /// Start a recording session: materialize a base state into a scratch
    /// worktree and print its path.
    Record {
        /// Template directory (defaults to `.`).
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Base state: `latest` or a patch name (that patch + ancestors).
        #[arg(long, default_value = "latest")]
        base: String,
        /// Apply a named preset when rendering the base (repeatable).
        #[arg(long = "preset")]
        presets: Vec<String>,
        /// Answer a question inline as KEY=VALUE (repeatable).
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// TOML file with answers.
        #[arg(long = "answers-file")]
        answers_file: Option<Utf8PathBuf>,
        /// Record a foreach integration patch: mount one sample instance of
        /// a repeatable include as INCLUDE=KEY; commit abstracts the sample
        /// back out into a `foreach` patch.
        #[arg(long)]
        foreach: Option<String>,
        /// Run this command in the rendered worktree; its output becomes the
        /// patch and the command is stored so `weft patch resync` can re-run
        /// it (e.g. --exec "npx shadcn@latest add button"). `${answer}` /
        /// `${expr}` interpolate declared answers; pass `--exec` with no
        /// value to write the command in $VISUAL/$EDITOR.
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        exec: Option<String>,
        /// Discard an existing session instead of failing.
        #[arg(long)]
        force: bool,
        /// Never prompt; fail if answers are missing.
        #[arg(long)]
        non_interactive: bool,
        /// Use sequential prompts instead of the full-screen wizard.
        #[arg(long)]
        no_wizard: bool,
    },
    /// Diff the recording worktree against its base and append the result as
    /// a new patch (with value abstraction).
    Commit {
        /// Template directory (defaults to `.`).
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Name for the new patch (defaults to patch-NNN).
        #[arg(long)]
        name: Option<String>,
        /// Starlark condition gating the new patch.
        #[arg(long)]
        when: Option<String>,
        /// Display title, e.g. "Add Prisma support" (metadata, not hashed).
        #[arg(long)]
        title: Option<String>,
        /// Human/agent-facing description stored with the patch (not hashed).
        #[arg(long)]
        describe: Option<String>,
        /// Tag the patch (repeatable).
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Accept all abstraction proposals without prompting.
        #[arg(long)]
        yes: bool,
        /// Keep one occurrence literal: ANSWER@PATH:LINE[:NTH] (repeatable;
        /// see `weft diff` for the keys). Implies accepting the rest.
        #[arg(long = "keep-literal")]
        keep_literal: Vec<String>,
        /// Never open the interactive form; use the default patch name.
        #[arg(long)]
        no_tui: bool,
    },
    /// Show the recording session's changes before committing: the concrete
    /// diff with abstraction candidates highlighted.
    Diff {
        /// Template directory (defaults to `.`).
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Show the stored form ({answer} placeholders) instead of values.
        #[arg(long)]
        abstracted: bool,
        /// Emit the preview as JSON (files, hunks, candidates, occurrences).
        #[arg(long)]
        json: bool,
    },
    /// List or show template presets.
    Presets {
        #[command(subcommand)]
        command: PresetsCommand,
    },
    /// Show the template's patch graph (nodes, dependency edges, and — when
    /// answers are supplied — which patches are active).
    Graph {
        /// Template directory (defaults to `.`).
        #[arg(default_value = ".")]
        template: Utf8PathBuf,
        /// Apply a named preset (repeatable).
        #[arg(long = "preset")]
        presets: Vec<String>,
        /// Answer a question inline as KEY=VALUE (repeatable).
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// TOML file with answers.
        #[arg(long = "answers-file")]
        answers_file: Option<Utf8PathBuf>,
        /// Print the graph document as JSON instead of a text summary.
        #[arg(long)]
        json: bool,
        /// Show what one patch (by name) contributes under the answers.
        #[arg(long = "diff")]
        diff: Option<String>,
    },
    /// Write JSON Schemas for weft.toml and patches/*.json (editor
    /// validation + completion).
    Schema {
        /// Output directory.
        #[arg(long, default_value = "schemas")]
        out: Utf8PathBuf,
    },
    /// Run the weft language server (stdio) for template authoring:
    /// live check diagnostics, completion, hover, go-to-definition.
    Lsp,
    /// Serve weft to AI agents over the Model Context Protocol (stdio).
    Mcp {
        /// Directory containing templates (or itself a template). Defaults
        /// to the current directory.
        #[arg(long = "templates-dir", default_value = ".")]
        templates_dir: Utf8PathBuf,
    },
    /// Print the template's contract: questions with types/defaults/gates,
    /// presets, patches, tasks, and ready-to-run commands. Agents should use
    /// `--json`.
    Describe {
        /// Template directory (defaults to `.`).
        #[arg(default_value = ".")]
        template: Utf8PathBuf,
        /// Emit the full machine-readable contract as JSON.
        #[arg(long)]
        json: bool,
        /// Write an AGENTS.md guide (default path: <template>/AGENTS.md;
        /// pass `-` for stdout).
        #[arg(long = "agents-md", num_args = 0..=1, default_missing_value = "")]
        agents_md: Option<String>,
    },
    /// Validate the template: manifest, expressions, patch graph, and (when
    /// answers are available) a full render plus patch commutation.
    Check {
        /// Template directory (defaults to `.`).
        #[arg(default_value = ".")]
        template: Utf8PathBuf,
        /// Apply a named preset for the render/commutation checks (repeatable).
        #[arg(long = "preset")]
        presets: Vec<String>,
        /// Answer a question inline as KEY=VALUE (repeatable).
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// TOML file with answers.
        #[arg(long = "answers-file")]
        answers_file: Option<Utf8PathBuf>,
        /// Emit {ok, issues, notes} as JSON (exit code still reflects ok).
        #[arg(long)]
        json: bool,
        /// Fail if a hub include isn't already pinned in weft.lock (CI).
        #[arg(long)]
        frozen: bool,
    },
}

#[derive(Subcommand)]
enum InstanceCmd {
    /// Add an instance of a repeatable include (renders its files in
    /// place). Run bare in a terminal for an interactive form.
    Add {
        /// The [[include]] name in the template.
        include: Option<String>,
        /// The new instance's key (lowercase alphanumerics, `-`, `_`).
        key: Option<String>,
        /// Child answers as ID=VALUE (child-scoped; repeatable).
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// Scaffolded project directory (defaults to `.`).
        #[arg(long, default_value = ".")]
        dest: Utf8PathBuf,
        /// Do not run hooks.
        #[arg(long)]
        skip_tasks: bool,
        /// Never prompt; fail if child answers are missing.
        #[arg(long)]
        non_interactive: bool,
        /// Never open the interactive form; fail on missing arguments.
        #[arg(long)]
        no_tui: bool,
    },
    /// Remove an instance (untouched files deleted, modified ones kept).
    Remove {
        include: String,
        key: String,
        #[arg(long, default_value = ".")]
        dest: Utf8PathBuf,
        /// Do not run hooks.
        #[arg(long)]
        skip_tasks: bool,
    },
    /// List the project's include instances.
    List {
        #[arg(long, default_value = ".")]
        dest: Utf8PathBuf,
    },
}

#[derive(Subcommand)]
enum HookCmd {
    /// Add a hook to a patch. Missing options open an interactive form in
    /// a terminal; provided flags prefill it.
    Add {
        /// Patch name (file stem) that owns the hook.
        patch: Option<String>,
        /// Template directory (defaults to `.`).
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Unique hook id (referenced by --after / hook: inputs).
        #[arg(long)]
        id: Option<String>,
        /// `pre` (guard, before writing) or `post` (after writing).
        #[arg(long)]
        phase: Option<String>,
        /// `check` (read-only), `setup` (idempotent local), `deploy` (external).
        #[arg(long)]
        effect: Option<String>,
        /// Short human label, e.g. "Verify the Go toolchain is installed".
        #[arg(long)]
        label: Option<String>,
        /// Shell command to run.
        #[arg(long)]
        action: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// Starlark gate.
        #[arg(long)]
        when: Option<String>,
        /// Hook id this one must run after (repeatable).
        #[arg(long = "after")]
        after: Vec<String>,
        /// Post-only update re-fire input: `glob:P`, `answer:ID`, `hook:ID`
        /// (repeatable).
        #[arg(long = "input")]
        inputs: Vec<String>,
        /// Never open the interactive form; fail on missing options.
        #[arg(long)]
        no_tui: bool,
    },
    /// Remove a hook from a patch by id.
    Rm {
        patch: String,
        id: String,
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
    },
    /// List every hook in execution order.
    Ls {
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
    },
}

#[derive(Subcommand)]
enum PatchCmd {
    /// Edit a patch's display metadata (never changes its content id).
    /// Run bare in a terminal for an interactive form.
    Set {
        /// Patch name (file stem).
        name: Option<String>,
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Display title, e.g. "Add Prisma support" (empty string clears it).
        #[arg(long)]
        title: Option<String>,
        /// Description (empty string clears it).
        #[arg(long)]
        describe: Option<String>,
        /// Add a tag (repeatable).
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Remove all existing tags first.
        #[arg(long)]
        clear_tags: bool,
        /// Never open the interactive form; fail on missing options.
        #[arg(long)]
        no_tui: bool,
    },
    /// List patches with their metadata.
    Ls {
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
    },
    /// Re-run the stored generator command of patches recorded with
    /// `weft record --exec` and rewrite their ops from the fresh output.
    Resync {
        /// Patch names to resync (or pass --all).
        names: Vec<String>,
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Resync every generated patch, in dependency order.
        #[arg(long)]
        all: bool,
        /// Override a stored record-time answer as KEY=VALUE (repeatable;
        /// single named patch only; persisted into the metadata).
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// Replace the stored keep-literal specs, as ANSWER@PATH:LINE[:NTH]
        /// (repeatable; single named patch only; persisted).
        #[arg(long = "keep-literal")]
        keep_literal: Vec<String>,
        /// Report what would change without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Emit the report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Replace the stored generator command of a `--exec` patch. `${…}`
    /// interpolates declared answers. Omit CMD to edit in $EDITOR.
    SetCommand {
        /// Patch name (file stem).
        name: String,
        /// New command (omit to open $VISUAL/$EDITOR).
        command: Option<String>,
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Regenerate the patch from the new command immediately.
        #[arg(long)]
        resync: bool,
    },
    /// Drop a patch's generator metadata: it becomes a plain, freely
    /// editable patch (`weft patch amend`), but `resync` no longer applies.
    Detach {
        /// Patch name (file stem).
        name: String,
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
    },
    /// Edit a patch's recorded content: reopens its contribution in a
    /// scratch worktree; `weft commit` re-derives its ops in place (id
    /// changes, like any content edit). Leaf patches only for now.
    Amend {
        /// Patch name (file stem).
        name: String,
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Apply a named preset when rendering the base (repeatable).
        #[arg(long = "preset")]
        presets: Vec<String>,
        /// Answer a question inline as KEY=VALUE (repeatable).
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// TOML file with answers.
        #[arg(long = "answers-file")]
        answers_file: Option<Utf8PathBuf>,
        /// Discard an existing session instead of failing.
        #[arg(long)]
        force: bool,
        /// Never prompt; fail if answers are missing.
        #[arg(long)]
        non_interactive: bool,
        /// Use sequential prompts instead of the full-screen wizard.
        #[arg(long)]
        no_wizard: bool,
    },
}

#[derive(Subcommand)]
enum HubCmd {
    /// Publish the template directory as owner/name@version.
    Publish {
        /// `owner/name` on the registry.
        spec: String,
        /// The version to publish (semver, greater than the latest).
        #[arg(long)]
        version: String,
        /// Template directory (defaults to `.`).
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Registry base URL (default: WEFT_HUB_URL).
        #[arg(long)]
        registry: Option<String>,
        /// Bearer token (default: WEFT_HUB_TOKEN).
        #[arg(long)]
        token: Option<String>,
    },
    /// Search the registry.
    Search {
        query: String,
        #[arg(long)]
        registry: Option<String>,
    },
    /// Show a template's versions.
    Info {
        /// `owner/name`.
        spec: String,
        #[arg(long)]
        registry: Option<String>,
    },
}

#[derive(Subcommand)]
enum PresetsCommand {
    /// List presets declared by a template.
    List {
        #[arg(default_value = ".")]
        template: Utf8PathBuf,
    },
    /// Show the answers a preset provides.
    Show {
        name: String,
        #[arg(default_value = ".")]
        template: Utf8PathBuf,
    },
    /// Create or update a preset: a tri-state wizard in a terminal (answer =
    /// lock; multichoice options cycle free/fixed/blocked), or scripted via
    /// --answer/--fix/--block with --non-interactive.
    Save {
        name: String,
        #[arg(default_value = ".")]
        template: Utf8PathBuf,
        /// Lock an answer as KEY=VALUE (repeatable).
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// Always-select a multichoice option, as KEY=CHOICE (repeatable).
        #[arg(long = "fix")]
        fix: Vec<String>,
        /// Never allow a multichoice option, as KEY=CHOICE (repeatable).
        #[arg(long = "block")]
        block: Vec<String>,
        /// Skip the wizard; use only the flags.
        #[arg(long)]
        non_interactive: bool,
    },
    /// Remove a preset: its declaration in weft.toml and its file.
    Rm {
        name: String,
        #[arg(default_value = ".")]
        template: Utf8PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::New {
            template,
            dest,
            presets,
            answers,
            answers_file,
            answers_json,
            instances,
            skip_tasks,
            non_interactive,
            no_wizard,
            no_tui,
            frozen,
        } => {
            // No template given: pick one interactively (terminal only).
            let (template, dest, presets) = match template {
                Some(t) => (t, dest, presets),
                None if tui::interactive(no_tui || non_interactive) => {
                    let form = forms::new_picker(&dest, &presets)?;
                    (form.template, form.dest, form.presets)
                }
                None => anyhow::bail!(
                    "missing TEMPLATE (weft new <template> [dest]); \
                     run in a terminal to pick one interactively"
                ),
            };
            // Hub refs resolve through the verified cache; the state file
            // records the resolved `hub:…@version` instead of the path.
            let (template, stored_ref) = match hub::parse_ref(template.as_str()) {
                Some(parsed) => {
                    let r = parsed?;
                    let registry = hub::registry_url(None)?;
                    let (dir, resolved) = hub::fetch(&registry, &r)?;
                    (dir, Some(resolved))
                }
                None => (template, None),
            };
            let mut opts = NewOptions {
                template,
                dest,
                presets,
                answers,
                answers_file,
                answers_json,
                instances,
                skip_tasks,
                stored_ref,
            };
            let wizard_ran = maybe_wizard(
                &opts.template,
                &opts.presets,
                opts.answers_file.as_deref(),
                &opts.answers,
                &mut opts.answers_json,
                non_interactive,
                no_wizard,
            )?;
            let mut interaction = auto_interaction(non_interactive);
            let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), frozen);
            let result = weft_engine::new::run(&opts, &mut resolver, interaction.as_mut());
            resolver.flush()?;
            result?;
            // Offer to capture the answers as a preset — local templates
            // only (a hub/cache copy isn't the author's working tree).
            if wizard_ran && opts.stored_ref.is_none() {
                if let Err(e) = offer_preset_capture(
                    &opts.template,
                    opts.answers_file.as_deref(),
                    &opts.answers,
                    opts.answers_json.as_deref(),
                    interaction.as_mut(),
                ) {
                    eprintln!("preset capture skipped: {e:#}");
                }
            }
            Ok(())
        }
        Command::Update {
            dest,
            dry_run,
            template,
            skip_tasks,
            non_interactive,
            frozen,
        } => {
            // A hub-ref project resolves its pinned version from the cache
            // (offline once cached) and hints when the index moved on.
            let template = match template {
                Some(t) => Some(t),
                None => match weft_engine::state::State::load(&dest) {
                    Ok(state) => match hub::parse_ref(&state.state.template) {
                        Some(parsed) => {
                            let r = parsed?;
                            let registry = hub::registry_url(None)?;
                            let (dir, _) = hub::fetch(&registry, &r)?;
                            if let (Some(pinned), Ok((latest, _))) = (
                                &r.version,
                                hub::resolve(
                                    &registry,
                                    &hub::HubRef {
                                        owner: r.owner.clone(),
                                        name: r.name.clone(),
                                        version: None,
                                    },
                                ),
                            ) {
                                if *pinned != latest {
                                    eprintln!(
                                        "note: {}/{} has {latest} on the registry (project pins {pinned}); \
                                         re-scaffold or wait for `weft update --to-latest`",
                                        r.owner, r.name
                                    );
                                }
                            }
                            Some(dir)
                        }
                        None => None,
                    },
                    Err(_) => None,
                },
            };
            let opts = weft_engine::update::UpdateOptions {
                dest,
                dry_run,
                template_override: template,
                skip_tasks,
                drop_instances: vec![],
            };
            let mut interaction = auto_interaction(non_interactive);
            let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), frozen);
            let report = weft_engine::update::run(&opts, &mut resolver, interaction.as_mut())?;
            resolver.flush()?;
            if opts.dry_run {
                Ok(())
            } else {
                weft_engine::update::finish(&report)
            }
        }
        Command::Instance { cmd } => match cmd {
            InstanceCmd::Add {
                include,
                key,
                answers,
                dest,
                skip_tasks,
                non_interactive,
                no_tui,
            } => {
                let (include, key) = match (include, key) {
                    (Some(include), Some(key)) => (include, key),
                    (include, key) if tui::interactive(no_tui || non_interactive) => {
                        let form = forms::instance_add(&dest, include, key)?;
                        (form.include, form.key)
                    }
                    _ => anyhow::bail!(
                        "missing INCLUDE and KEY (weft instance add <include> <key>); \
                         run in a terminal for the interactive form"
                    ),
                };
                let mut interaction = auto_interaction(non_interactive);
                let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
                let report = weft_engine::instance::add(
                    &weft_engine::instance::InstanceAddOptions {
                        dest,
                        include,
                        key,
                        answers,
                        skip_tasks,
                    },
                    &mut resolver,
                    interaction.as_mut(),
                )?;
                resolver.flush()?;
                weft_engine::update::finish(&report)
            }
            InstanceCmd::Remove {
                include,
                key,
                dest,
                skip_tasks,
            } => {
                let mut interaction = auto_interaction(false);
                let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
                let report = weft_engine::instance::remove(
                    &dest,
                    &include,
                    &key,
                    skip_tasks,
                    &mut resolver,
                    interaction.as_mut(),
                )?;
                resolver.flush()?;
                weft_engine::update::finish(&report)
            }
            InstanceCmd::List { dest } => weft_engine::instance::list(&dest),
        },
        Command::Init { dir, name } => weft_engine::init::run(&dir, name.as_deref()),
        Command::Hook { cmd } => match cmd {
            HookCmd::Add {
                patch,
                template,
                id,
                phase,
                effect,
                label,
                action,
                description,
                when,
                after,
                inputs,
                no_tui,
            } => {
                // Everything but the command provided → write it in
                // $VISUAL/$EDITOR instead of the full TUI form.
                let action = match (&patch, &id, &phase, &effect, &label, action) {
                    (Some(_), Some(_), Some(_), Some(_), Some(_), None) => {
                        Some(weft_engine::interact::edit_command(
                            "Enter the hook's shell command.\n\
                             Lines starting with '#' are ignored. Multi-line commands run \
                             via sh -c.\n\
                             ${answer_or_expr} interpolates declared answers; unknown ${...} \
                             passes through to the shell.",
                        )?)
                    }
                    (_, _, _, _, _, action) => action,
                };
                let opts = match (patch, id, phase, effect, label, action) {
                    (
                        Some(patch),
                        Some(id),
                        Some(phase),
                        Some(effect),
                        Some(label),
                        Some(action),
                    ) => weft_engine::author::HookAddOptions {
                        patch,
                        id,
                        phase,
                        effect,
                        label,
                        action,
                        description,
                        when,
                        after,
                        inputs,
                    },
                    (patch, id, phase, effect, label, action) if tui::interactive(no_tui) => {
                        forms::hook_add(
                            &template,
                            patch,
                            id,
                            phase,
                            effect,
                            label,
                            action,
                            description,
                            when,
                            after,
                            inputs,
                        )?
                    }
                    _ => anyhow::bail!(
                        "missing required options (PATCH, --id, --phase, --effect, --label, \
                         --action); run in a terminal for the interactive form or pass the flags"
                    ),
                };
                weft_engine::author::hook_add(&template, &opts)
            }
            HookCmd::Rm {
                patch,
                id,
                template,
            } => weft_engine::author::hook_rm(&template, &patch, &id),
            HookCmd::Ls { template } => weft_engine::author::hook_ls(&template),
        },
        Command::Patch { cmd } => match cmd {
            PatchCmd::Set {
                name,
                template,
                title,
                describe,
                tags,
                clear_tags,
                no_tui,
            } => {
                let has_changes =
                    title.is_some() || describe.is_some() || !tags.is_empty() || clear_tags;
                let opts = match (&name, has_changes) {
                    (Some(_), true) => weft_engine::author::PatchSetOptions {
                        name: name.expect("matched Some"),
                        title,
                        describe,
                        tags,
                        clear_tags,
                    },
                    _ if tui::interactive(no_tui) => {
                        forms::patch_set(&template, name, title, describe, tags, clear_tags)?
                    }
                    _ => anyhow::bail!(
                        "nothing to change; pass NAME plus --title/--describe/--tag/--clear-tags, \
                         or run in a terminal for the interactive form"
                    ),
                };
                weft_engine::author::patch_set(&template, &opts)
            }
            PatchCmd::Ls { template } => weft_engine::author::patch_ls(&template),
            PatchCmd::Resync {
                names,
                template,
                all,
                answers,
                keep_literal,
                dry_run,
                json,
            } => {
                let opts = weft_engine::generate::ResyncOptions {
                    template,
                    names,
                    all,
                    answers,
                    keep_literal,
                    dry_run,
                };
                let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
                let mut interaction = auto_interaction(false);
                let report =
                    weft_engine::generate::resync(&opts, &mut resolver, interaction.as_mut())?;
                resolver.flush()?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    for entry in &report.entries {
                        use weft_engine::generate::Outcome;
                        match &entry.outcome {
                            Outcome::UpToDate => eprintln!("{}: up to date", entry.name),
                            Outcome::Rewritten { ops } => {
                                eprintln!("{}: rewritten ({ops} op(s))", entry.name)
                            }
                            Outcome::WouldRewrite { ops } => {
                                eprintln!("{}: would rewrite ({ops} op(s))", entry.name)
                            }
                            Outcome::Skipped { reason } => {
                                eprintln!("{}: skipped — {reason}", entry.name)
                            }
                        }
                    }
                    for note in &report.notes {
                        eprintln!("note: {note}");
                    }
                    for issue in &report.issues {
                        eprintln!("issue: {issue}");
                    }
                    if report.entries.iter().any(|e| {
                        matches!(e.outcome, weft_engine::generate::Outcome::Rewritten { .. })
                    }) {
                        eprintln!(
                            "review with `git diff patches/`; run `weft check` to re-verify \
                             the graph"
                        );
                    }
                }
                if report.issues.is_empty() {
                    Ok(())
                } else {
                    std::process::exit(1);
                }
            }
            PatchCmd::SetCommand {
                name,
                command,
                template,
                resync,
            } => {
                let command = match command {
                    Some(cmd) => cmd,
                    None => weft_engine::interact::edit_command(
                        "Enter the generator command; its output becomes the patch.\n\
                         Lines starting with '#' are ignored. ${answer_or_expr} interpolates\n\
                         declared answers, e.g. ${project_name} or ${' '.join(components)}.",
                    )?,
                };
                weft_engine::author::set_generator_command(&template, &name, &command)?;
                if resync {
                    let opts = weft_engine::generate::ResyncOptions {
                        template,
                        names: vec![name],
                        all: false,
                        answers: vec![],
                        keep_literal: vec![],
                        dry_run: false,
                    };
                    let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
                    let mut interaction = auto_interaction(false);
                    let report =
                        weft_engine::generate::resync(&opts, &mut resolver, interaction.as_mut())?;
                    resolver.flush()?;
                    for entry in &report.entries {
                        eprintln!("{}: {:?}", entry.name, entry.outcome);
                    }
                    if !report.issues.is_empty() {
                        for issue in &report.issues {
                            eprintln!("issue: {issue}");
                        }
                        std::process::exit(1);
                    }
                }
                Ok(())
            }
            PatchCmd::Detach { name, template } => {
                weft_engine::author::detach_generator(&template, &name)
            }
            PatchCmd::Amend {
                name,
                template,
                presets,
                answers,
                answers_file,
                force,
                non_interactive,
                no_wizard,
            } => {
                let mut opts = weft_engine::amend::AmendOptions {
                    template,
                    name,
                    presets,
                    answers,
                    answers_file,
                    answers_json: None,
                    force,
                };
                maybe_wizard(
                    &opts.template,
                    &opts.presets,
                    opts.answers_file.as_deref(),
                    &opts.answers,
                    &mut opts.answers_json,
                    non_interactive,
                    no_wizard,
                )?;
                let mut interaction = auto_interaction(non_interactive);
                let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
                let worktree =
                    weft_engine::amend::start(&opts, &mut resolver, interaction.as_mut())?;
                resolver.flush()?;
                weft_engine::amend::announce(&worktree, &opts.name);
                Ok(())
            }
        },
        Command::Hub { cmd } => match cmd {
            HubCmd::Publish {
                spec,
                version,
                template,
                registry,
                token,
            } => {
                let registry = hub::registry_url(registry.as_deref())?;
                let token = token
                    .or_else(|| std::env::var("WEFT_HUB_TOKEN").ok())
                    .ok_or_else(|| {
                        anyhow::anyhow!("no token; set WEFT_HUB_TOKEN or pass --token")
                    })?;
                let (owner, name) = spec
                    .split_once('/')
                    .ok_or_else(|| anyhow::anyhow!("expected owner/name, got `{spec}`"))?;
                // Ensure a complete, fresh lockfile before packing (so the
                // published tarball carries pinned child versions).
                let mut resolver = hub::HubResolver::new(Some(registry.clone()), false);
                Template::load_with(&template, &mut resolver)?;
                resolver.flush()?;
                hub::publish(&registry, &token, owner, name, &version, &template)
            }
            HubCmd::Search { query, registry } => {
                let registry = hub::registry_url(registry.as_deref())?;
                hub::search(&registry, &query)
            }
            HubCmd::Info { spec, registry } => {
                let registry = hub::registry_url(registry.as_deref())?;
                hub::info(&registry, &spec)
            }
        },
        Command::Lock {
            template,
            upgrade,
            registry,
        } => {
            let registry = hub::registry_url(registry.as_deref()).ok();
            let mut resolver = hub::HubResolver::new(registry, false);
            if upgrade {
                resolver.clear_lock(&template)?;
            }
            Template::load_with(&template, &mut resolver)?;
            resolver.flush()?;
            eprintln!("lock up to date");
            Ok(())
        }
        Command::Record {
            template,
            base,
            presets,
            answers,
            answers_file,
            foreach,
            exec,
            force,
            non_interactive,
            no_wizard,
        } => {
            // Bare `--exec` opens $VISUAL/$EDITOR to write the command.
            let exec = match exec {
                Some(cmd) if cmd.is_empty() => Some(weft_engine::interact::edit_command(
                    "Enter the generator command; its output becomes the patch.\n\
                     Lines starting with '#' are ignored. Multi-line commands run via sh -c.\n\
                     ${answer_or_expr} interpolates declared answers, e.g. ${project_name} or\n\
                     ${' '.join(components)}; unknown ${...} passes through to the shell.",
                )?),
                other => other,
            };
            let mut opts = weft_engine::record::RecordOptions {
                template,
                base,
                presets,
                answers,
                answers_file,
                answers_json: None,
                foreach,
                exec,
                force,
            };
            maybe_wizard(
                &opts.template,
                &opts.presets,
                opts.answers_file.as_deref(),
                &opts.answers,
                &mut opts.answers_json,
                non_interactive,
                no_wizard,
            )?;
            let mut interaction = auto_interaction(non_interactive);
            let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
            let worktree = weft_engine::record::run(&opts, &mut resolver, interaction.as_mut())?;
            resolver.flush()?;
            weft_engine::record::announce(&worktree);
            Ok(())
        }
        Command::Commit {
            template,
            name,
            when,
            title,
            describe,
            tags,
            yes,
            keep_literal,
            no_tui,
        } => {
            let (name, title, describe, when, tags) = match name {
                Some(name) => (Some(name), title, describe, when, tags),
                None if tui::interactive(no_tui) => {
                    let form = forms::commit(&template, title, describe, when, tags)?;
                    (
                        Some(form.name),
                        form.title,
                        form.describe,
                        form.when,
                        form.tags,
                    )
                }
                None => (None, title, describe, when, tags),
            };
            let opts = weft_engine::commit::CommitOptions {
                template,
                name,
                when,
                title,
                describe,
                tags,
                decisions: None,
                keep_literal,
            };
            let mut interaction = auto_interaction(yes);
            let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
            let result = weft_engine::commit::run(&opts, &mut resolver, interaction.as_mut());
            resolver.flush()?;
            result
        }
        Command::Diff {
            template,
            abstracted,
            json,
        } => {
            let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
            let tpl = Template::load_with(&template, &mut resolver)?;
            resolver.flush()?;
            let mut interaction = auto_interaction(false);
            let preview = weft_engine::commit::preview(&tpl, interaction.as_mut())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&preview)?);
                Ok(())
            } else {
                diffcmd::print(&preview, abstracted)
            }
        }
        Command::Presets { command } => match command {
            PresetsCommand::List { template } => {
                let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
                let template = Template::load_with(&template, &mut resolver)?;
                resolver.flush()?;
                for preset in &template.manifest.presets {
                    println!("{}\t{}", preset.name, preset.file);
                }
                Ok(())
            }
            PresetsCommand::Show { name, template } => {
                let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
                let template = Template::load_with(&template, &mut resolver)?;
                resolver.flush()?;
                let spec = template.preset_spec(&name)?;
                for (id, value) in spec.lock_answers().iter() {
                    let tv = toml::Value::try_from(value.clone())?;
                    println!("{id} = {tv} (locked)");
                }
                for (id, (fixed, blocked)) in spec.constraints() {
                    let mut parts = Vec::new();
                    if !fixed.is_empty() {
                        parts.push(format!("fixed: {}", fixed.join(", ")));
                    }
                    if !blocked.is_empty() {
                        parts.push(format!("blocked: {}", blocked.join(", ")));
                    }
                    println!("{id}: {}", parts.join(" · "));
                }
                Ok(())
            }
            PresetsCommand::Save {
                name,
                template: dir,
                answers,
                fix,
                block,
                non_interactive,
            } => {
                use weft_engine::preset::{PresetEntry, PresetSpec};
                let template = Template::load(&dir)?;

                // Flag prefills: --answer locks, --fix/--block constraints.
                let mut locks = std::collections::BTreeMap::new();
                for arg in &answers {
                    let (id, value) =
                        weft_engine::answers::parse_answer_arg(&template.manifest.questions, arg)?;
                    locks.insert(id, value);
                }
                let mut constraints: std::collections::BTreeMap<
                    weft_core::AnswerId,
                    (Vec<String>, Vec<String>),
                > = std::collections::BTreeMap::new();
                for (args, blocked_side) in [(&fix, false), (&block, true)] {
                    for arg in args.iter() {
                        let (key, choice) = arg.split_once('=').ok_or_else(|| {
                            anyhow::anyhow!("--fix/--block {arg:?} is not KEY=CHOICE")
                        })?;
                        let entry = constraints
                            .entry(weft_core::AnswerId(key.to_owned()))
                            .or_default();
                        let side = if blocked_side {
                            &mut entry.1
                        } else {
                            &mut entry.0
                        };
                        side.push(choice.to_owned());
                    }
                }

                let (locks, constraints) = if tui::interactive(non_interactive) {
                    wizard::run_author(
                        &template.manifest.template.name,
                        &template.manifest.questions,
                        locks,
                        constraints,
                        &weft_engine::eval(),
                    )?
                } else {
                    (locks, constraints)
                };

                let mut spec = PresetSpec::default();
                for (id, value) in locks {
                    spec.sources.insert(id.clone(), name.clone());
                    spec.entries.insert(id, PresetEntry::Lock(value));
                }
                for (id, (fixed, blocked)) in constraints {
                    spec.sources.insert(id.clone(), name.clone());
                    spec.entries
                        .insert(id, PresetEntry::Constraint { fixed, blocked });
                }
                weft_engine::preset::save(&dir, &name, &spec)?;
                println!("saved preset `{name}`");
                Ok(())
            }
            PresetsCommand::Rm {
                name,
                template: dir,
            } => {
                weft_engine::preset::remove(&dir, &name)?;
                println!("removed preset `{name}`");
                Ok(())
            }
        },
        Command::Graph {
            template,
            presets,
            answers,
            answers_file,
            json,
            diff,
        } => {
            use weft_engine::interact::NonInteractive;
            let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
            let template = Template::load_with(&template, &mut resolver)?;
            resolver.flush()?;
            let eval = weft_engine::eval();

            // Resolve answers when any were supplied, or when defaults alone
            // suffice; otherwise fall back to a structural (no-answers) graph.
            let flags_given = !presets.is_empty() || !answers.is_empty() || answers_file.is_some();
            let provided = weft_engine::answers::layered_answers(
                &template,
                &presets,
                answers_file.as_deref(),
                &answers,
            )?;
            let placeholders =
                weft_engine::answers::placeholder_secrets(&template.manifest.questions);
            let resolved = match weft_engine::answers::gather(
                &template,
                &provided,
                &placeholders,
                &eval,
                &mut NonInteractive,
            ) {
                Ok(resolved) => Some(resolved),
                Err(e) if flags_given => return Err(e),
                Err(_) => None,
            };

            if let Some(patch_name) = diff {
                let resolved = resolved.ok_or_else(|| {
                    anyhow::anyhow!(
                        "--diff needs a complete answer set; pass --answer/--answers-file/--preset"
                    )
                })?;
                let node_diff =
                    weft_engine::graph::node_diff(&template, &patch_name, &resolved, &eval)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&node_diff)?);
                } else {
                    eprintln!(
                        "patch `{}` ({}) — {}",
                        node_diff.name,
                        node_diff.id.short(),
                        if node_diff.active {
                            "active"
                        } else {
                            "inactive under these answers"
                        }
                    );
                    for file in &node_diff.files {
                        println!("{}", file.diff);
                    }
                }
                return Ok(());
            }

            let doc = weft_engine::graph::graph_doc(&template, resolved.as_ref(), &eval)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&doc)?);
            } else {
                println!(
                    "template `{}` — {} node(s), {} edge(s)",
                    doc.template.name,
                    doc.nodes.len(),
                    doc.edges.len()
                );
                for node in &doc.nodes {
                    let state = match node.active {
                        Some(true) => " [active]",
                        Some(false) => " [inactive]",
                        None => "",
                    };
                    let when = node
                        .when
                        .as_deref()
                        .map(|w| format!(" when={w}"))
                        .unwrap_or_default();
                    println!("• {} ({}){}{}", node.name, node.id.short(), when, state);
                    for op in &node.ops {
                        println!("    {} {}", op.kind, op.path);
                    }
                }
            }
            Ok(())
        }
        Command::Schema { out } => {
            for path in schema::write_schemas(&out)? {
                eprintln!("wrote {path}");
            }
            Ok(())
        }
        Command::Lsp => lsp::serve(),
        Command::Mcp { templates_dir } => mcp::serve(templates_dir),
        Command::Describe {
            template,
            json,
            agents_md,
        } => {
            let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), false);
            let tpl = Template::load_with(&template, &mut resolver)?;
            resolver.flush()?;
            let eval = weft_engine::eval();
            let doc = weft_engine::describe::describe(&tpl, &eval)?;
            if let Some(path) = agents_md {
                let md = weft_engine::describe::agents_md(&doc);
                if path == "-" {
                    print!("{md}");
                } else {
                    let target = if path.is_empty() {
                        template.join("AGENTS.md")
                    } else {
                        Utf8PathBuf::from(path)
                    };
                    std::fs::write(&target, md)?;
                    eprintln!("wrote {target}");
                }
                return Ok(());
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&doc)?);
            } else {
                print!("{}", weft_engine::describe::agents_md(&doc));
            }
            Ok(())
        }
        Command::Check {
            template,
            presets,
            answers,
            answers_file,
            json,
            frozen,
        } => {
            let mut resolver = hub::HubResolver::new(hub::registry_url(None).ok(), frozen);
            let name = Template::load_with(&template, &mut resolver)?
                .manifest
                .template
                .name;
            let opts = weft_engine::check::CheckOptions {
                template,
                presets,
                answers,
                answers_file,
            };
            let report = weft_engine::check::run(&opts, &mut resolver)?;
            resolver.flush()?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": report.issues.is_empty(),
                        "issues": report.issues,
                        "notes": report.notes,
                    })
                );
                if report.issues.is_empty() {
                    Ok(())
                } else {
                    std::process::exit(1);
                }
            } else {
                weft_engine::check::finish(&name, &report)
            }
        }
    }
}

/// Run the full-screen wizard when interactive (TTY, not --non-interactive,
/// not --no-wizard) and stash its answers as the highest-precedence JSON
/// layer. Falls through silently otherwise. Returns whether the wizard ran.
#[allow(clippy::too_many_arguments)]
fn maybe_wizard(
    template_dir: &Utf8PathBuf,
    presets: &[String],
    answers_file: Option<&camino::Utf8Path>,
    answer_args: &[String],
    answers_json: &mut Option<String>,
    non_interactive: bool,
    no_wizard: bool,
) -> anyhow::Result<bool> {
    use std::io::IsTerminal;
    if non_interactive || no_wizard || !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    let template = Template::load(template_dir)?;
    let layered = weft_engine::answers::layered_with_json_full(
        &template,
        presets,
        answers_file,
        answer_args,
        answers_json.as_deref(),
    )?;
    let provided = layered.answers;
    let locks = wizard::PresetLocks {
        locked: layered.locked,
        constraints: layered.constraints,
    };
    let eval = weft_engine::eval();
    let entered = wizard::run(
        &template.manifest.template.name,
        &template.manifest.questions,
        &provided,
        &locks,
        &eval,
    )?;
    if !entered.is_empty() {
        let mut merged = provided;
        merged.overlay(&entered);
        // Serialize the wizard layer only (secrets never appear here).
        let map: std::collections::BTreeMap<String, serde_json::Value> = entered
            .iter()
            .map(|(k, v)| {
                let json = value_to_json(v);
                (k.0.clone(), json)
            })
            .collect();
        *answers_json = Some(serde_json::to_string(&map)?);
    }
    Ok(true)
}

/// After an interactive `weft new` succeeds against a *local* template,
/// offer once to capture the user's answers (flags/file/wizard — not preset
/// locks or defaults) as a new preset in that template.
fn offer_preset_capture(
    template_dir: &Utf8PathBuf,
    answers_file: Option<&camino::Utf8Path>,
    answer_args: &[String],
    answers_json: Option<&str>,
    interaction: &mut dyn weft_engine::interact::Interaction,
) -> anyhow::Result<()> {
    use weft_engine::preset::{PresetEntry, PresetSpec};
    let template = Template::load(template_dir)?;
    // No presets selected here: only the user's own layers are captured.
    let captured = weft_engine::answers::layered_with_json(
        &template,
        &[],
        answers_file,
        answer_args,
        answers_json,
    )?;
    if captured.iter().next().is_none() {
        return Ok(());
    }
    if !interaction.confirm("Save these answers as a preset of the template?", false)? {
        return Ok(());
    }
    let name_question = weft_core::Question {
        id: weft_core::AnswerId("preset_name".into()),
        kind: weft_core::AnswerKind::String,
        prompt: Some("preset name".into()),
        description: None,
        example: None,
        default: None,
        when: None,
        computed: false,
        section: None,
    };
    let name = match interaction.ask(&name_question, None)? {
        weft_core::Value::String(s) => s,
        _ => anyhow::bail!("preset name must be a string"),
    };
    let mut spec = PresetSpec::default();
    for (id, value) in captured.iter() {
        spec.sources.insert(id.clone(), name.clone());
        spec.entries
            .insert(id.clone(), PresetEntry::Lock(value.clone()));
    }
    weft_engine::preset::save(template_dir, &name, &spec)?;
    println!("saved preset `{name}` in {template_dir}");
    Ok(())
}

/// JSON projection of a wizard-entered value. The wizard never holds secrets,
/// so `Value::Secret` (and any list containing one) is unreachable here.
fn value_to_json(v: &weft_core::Value) -> serde_json::Value {
    match v {
        weft_core::Value::String(s) => serde_json::Value::String(s.clone()),
        weft_core::Value::Bool(b) => serde_json::Value::Bool(*b),
        weft_core::Value::Int(i) => serde_json::Value::Number((*i).into()),
        weft_core::Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        weft_core::Value::Secret(_) => unreachable!("wizard never holds secrets"),
    }
}
