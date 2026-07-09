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
        /// Accept all abstraction proposals without prompting.
        #[arg(long)]
        yes: bool,
    },
    /// List or show template presets.
    Presets {
        #[command(subcommand)]
        command: PresetsCommand,
    },
    /// Validate the template manifest and patch graph.
    Check {
        /// Template directory (defaults to `.`).
        #[arg(default_value = ".")]
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
            skip_tasks,
            non_interactive,
        } => {
            let opts = NewOptions {
                template,
                dest,
                presets,
                answers,
                answers_file,
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
            weft_engine::update::finish(&report)
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
            yes,
        } => {
            let opts = weft_engine::commit::CommitOptions {
                template,
                name,
                when,
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
        Command::Check { template } => {
            let template = Template::load(&template)?;
            println!(
                "ok: template `{}` with {} question(s), {} patch(es), {} task(s)",
                template.manifest.template.name,
                template.manifest.questions.len(),
                template.patches.len(),
                template.manifest.tasks.len()
            );
            Ok(())
        }
    }
}
