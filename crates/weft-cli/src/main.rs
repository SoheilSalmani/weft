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
