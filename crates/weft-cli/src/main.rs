mod mcp;
mod schema;

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
        /// Do not run template tasks after scaffolding.
        #[arg(long)]
        skip_tasks: bool,
        /// Never prompt; fail if answers are missing.
        #[arg(long)]
        non_interactive: bool,
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
        /// Discard an existing session instead of failing.
        #[arg(long)]
        force: bool,
        /// Never prompt; fail if answers are missing.
        #[arg(long)]
        non_interactive: bool,
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
            skip_tasks,
            non_interactive,
        } => {
            let opts = NewOptions {
                template,
                dest,
                presets,
                answers,
                answers_file,
                answers_json,
                skip_tasks,
            };
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
            };
            let mut interaction = auto_interaction(non_interactive);
            let report = weft_engine::update::run(&opts, interaction.as_mut())?;
            if opts.dry_run {
                Ok(())
            } else {
                weft_engine::update::finish(&report)
            }
        }
        Command::Record {
            template,
            base,
            presets,
            answers,
            answers_file,
            force,
            non_interactive,
        } => {
            let opts = weft_engine::record::RecordOptions {
                template,
                base,
                presets,
                answers,
                answers_file,
                answers_json: None,
                force,
            };
            let mut interaction = auto_interaction(non_interactive);
            let worktree = weft_engine::record::run(&opts, interaction.as_mut())?;
            weft_engine::record::announce(&worktree);
            Ok(())
        }
        Command::Commit {
            template,
            name,
            when,
            describe,
            tags,
            yes,
        } => {
            let opts = weft_engine::commit::CommitOptions {
                template,
                name,
                when,
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
