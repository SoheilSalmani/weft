mod lsp;
mod mcp;
mod schema;
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
    /// Scaffold a template into a destination directory.
    New {
        /// Path to the template (a directory containing weft.toml).
        template: Utf8PathBuf,
        /// Destination directory (must be empty or absent). Defaults to `.`.
        #[arg(default_value = ".")]
        dest: Utf8PathBuf,
        /// Apply a named preset (repeatable; later presets win).
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
    },
}

#[derive(Subcommand)]
enum InstanceCmd {
    /// Add an instance of a repeatable include (renders its files in place).
    Add {
        /// The [[include]] name in the template.
        include: String,
        /// The new instance's key (lowercase alphanumerics, `-`, `_`).
        key: String,
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
    /// Add a hook to a patch.
    Add {
        /// Patch name (file stem) that owns the hook.
        patch: String,
        /// Template directory (defaults to `.`).
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Unique hook id (referenced by --after / hook: inputs).
        #[arg(long)]
        id: String,
        /// `pre` (guard, before writing) or `post` (after writing).
        #[arg(long)]
        phase: String,
        /// `check` (read-only), `setup` (idempotent local), `deploy` (external).
        #[arg(long)]
        effect: String,
        /// Short human label, e.g. "Verify the Go toolchain is installed".
        #[arg(long)]
        label: String,
        /// Shell command to run.
        #[arg(long)]
        action: String,
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
    Set {
        /// Patch name (file stem).
        name: String,
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
    },
    /// List patches with their metadata.
    Ls {
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
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
        } => {
            let mut opts = NewOptions {
                template,
                dest,
                presets,
                answers,
                answers_file,
                answers_json,
                instances,
                skip_tasks,
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
            weft_engine::new::run(&opts, interaction.as_mut())
        }
        Command::Update {
            dest,
            dry_run,
            template,
            skip_tasks,
            non_interactive,
        } => {
            let opts = weft_engine::update::UpdateOptions {
                dest,
                dry_run,
                template_override: template,
                skip_tasks,
                drop_instances: vec![],
            };
            let mut interaction = auto_interaction(non_interactive);
            let report = weft_engine::update::run(&opts, interaction.as_mut())?;
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
            } => {
                let mut interaction = auto_interaction(non_interactive);
                let report = weft_engine::instance::add(
                    &weft_engine::instance::InstanceAddOptions {
                        dest,
                        include,
                        key,
                        answers,
                        skip_tasks,
                    },
                    interaction.as_mut(),
                )?;
                weft_engine::update::finish(&report)
            }
            InstanceCmd::Remove {
                include,
                key,
                dest,
                skip_tasks,
            } => {
                let mut interaction = auto_interaction(false);
                let report = weft_engine::instance::remove(
                    &dest,
                    &include,
                    &key,
                    skip_tasks,
                    interaction.as_mut(),
                )?;
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
            } => weft_engine::author::hook_add(
                &template,
                &weft_engine::author::HookAddOptions {
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
            ),
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
            } => weft_engine::author::patch_set(
                &template,
                &weft_engine::author::PatchSetOptions {
                    name,
                    title,
                    describe,
                    tags,
                    clear_tags,
                },
            ),
            PatchCmd::Ls { template } => weft_engine::author::patch_ls(&template),
        },
        Command::Record {
            template,
            base,
            presets,
            answers,
            answers_file,
            foreach,
            force,
            non_interactive,
            no_wizard,
        } => {
            let mut opts = weft_engine::record::RecordOptions {
                template,
                base,
                presets,
                answers,
                answers_file,
                answers_json: None,
                foreach,
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
            let worktree = weft_engine::record::run(&opts, interaction.as_mut())?;
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
        } => {
            let opts = weft_engine::commit::CommitOptions {
                template,
                name,
                when,
                title,
                describe,
                tags,
                decisions: None,
            };
            let mut interaction = auto_interaction(yes);
            weft_engine::commit::run(&opts, interaction.as_mut())
        }
        Command::Presets { command } => match command {
            PresetsCommand::List { template } => {
                let template = Template::load(&template)?;
                for preset in &template.manifest.presets {
                    println!("{}\t{}", preset.name, preset.file);
                }
                Ok(())
            }
            PresetsCommand::Show { name, template } => {
                let template = Template::load(&template)?;
                let answers = template.preset(&name)?;
                print!("{}", toml::to_string_pretty(&answers)?);
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
            let template = Template::load(&template)?;
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
            let tpl = Template::load(&template)?;
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
        } => {
            let name = Template::load(&template)?.manifest.template.name;
            let opts = weft_engine::check::CheckOptions {
                template,
                presets,
                answers,
                answers_file,
            };
            let report = weft_engine::check::run(&opts)?;
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
/// layer. Falls through silently otherwise.
#[allow(clippy::too_many_arguments)]
fn maybe_wizard(
    template_dir: &Utf8PathBuf,
    presets: &[String],
    answers_file: Option<&camino::Utf8Path>,
    answer_args: &[String],
    answers_json: &mut Option<String>,
    non_interactive: bool,
    no_wizard: bool,
) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    if non_interactive || no_wizard || !std::io::stdin().is_terminal() {
        return Ok(());
    }
    let template = Template::load(template_dir)?;
    let provided = weft_engine::answers::layered_with_json(
        &template,
        presets,
        answers_file,
        answer_args,
        answers_json.as_deref(),
    )?;
    let eval = weft_engine::eval();
    let entered = wizard::run(
        &template.manifest.template.name,
        &template.manifest.questions,
        &provided,
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
