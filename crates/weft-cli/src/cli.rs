//! The command line: every command, argument and flag weft accepts, and
//! the help text clap renders from their doc comments.

use camino::Utf8PathBuf;
use clap::{CommandFactory as _, FromArgMatches as _, Parser, Subcommand};

use crate::ctx;
use crate::ui::{ColorChoice, Presentation};

/// Parse the command line, exiting 2 with clap's usage error when it does
/// not parse. Help and usage errors honour `--color` even though clap
/// prints them before the flag is parsed.
pub(crate) fn parse() -> Cli {
    let color = match prescan_color(std::env::args_os()) {
        Some(ColorChoice::Always) => clap::ColorChoice::Always,
        Some(ColorChoice::Never) => clap::ColorChoice::Never,
        _ => clap::ColorChoice::Auto,
    };
    let matches = Cli::command().color(color).get_matches();
    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let mut level = &matches;
    cli.title = String::from("weft");
    while let Some((name, sub)) = level.subcommand() {
        cli.title.push(' ');
        cli.title.push_str(name);
        level = sub;
    }
    cli
}

/// The last `--color WHEN` or `--color=WHEN` before a `--`.
fn prescan_color(args: impl Iterator<Item = std::ffi::OsString>) -> Option<ColorChoice> {
    let mut found = None;
    let mut args = args.skip(1);
    while let Some(arg) = args.next() {
        let value = match arg.to_str() {
            Some("--") => break,
            Some("--color") => args.next(),
            Some(a) => match a.strip_prefix("--color=") {
                Some(v) => Some(v.into()),
                None => continue,
            },
            None => continue,
        };
        found = match value.as_ref().and_then(|v| v.to_str()) {
            Some("always") => Some(ColorChoice::Always),
            Some("never") => Some(ColorChoice::Never),
            Some("auto") => Some(ColorChoice::Auto),
            _ => found,
        };
    }
    found
}

/// Help colours: headers and usage in weft's accent, literals bold.
const STYLES: clap::builder::Styles = {
    use clap::builder::styling::{Ansi256Color, AnsiColor, Color, Style};
    let accent = Style::new()
        .bold()
        .fg_color(Some(Color::Ansi256(Ansi256Color(141))));
    clap::builder::Styles::styled()
        .header(accent)
        .usage(accent)
        .literal(Style::new().bold())
        .placeholder(Style::new().dimmed())
        .context(Style::new().dimmed())
        .valid(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green))))
        .invalid(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow))))
        .error(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Red))),
        )
};

#[derive(Parser)]
#[command(name = "weft", version, about = "Record-based project scaffolding", styles = STYLES)]
pub(crate) struct Cli {
    /// When to colour output; `auto` colours what goes to a terminal, unless
    /// NO_COLOR is set.
    #[arg(
        long,
        global = true,
        value_enum,
        value_name = "WHEN",
        default_value_t = ColorChoice::Auto,
        help_heading = "Global options"
    )]
    pub(crate) color: ColorChoice,
    #[command(subcommand)]
    pub(crate) command: Command,
    /// The command as typed, `weft session new`: the prompt rail's title.
    #[arg(skip)]
    pub(crate) title: String,
}

impl Command {
    /// How this command's output may be presented: the language server and
    /// the MCP server speak a protocol on stdout, so they never draw.
    pub(crate) fn presentation(&self) -> Presentation {
        match self {
            Command::Lsp | Command::Mcp { .. } => Presentation::Protocol,
            _ => Presentation::Terminal,
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Scaffold a template into a destination directory.
    New {
        /// The template: a directory containing weft.toml, a hub ref
        /// (`hub:owner/name[@version]`), or a git source — `gh:owner/repo`,
        /// any git URL — with an optional `//subdir` and `@rev` (tag,
        /// branch, or commit): `gh:acme/templates//base@v1`.
        template: String,
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
        /// Never prompt: take every default, and fail on a question without
        /// one.
        #[arg(long)]
        non_interactive: bool,
        /// Fail if a remote include isn't already pinned in weft.lock (CI).
        #[arg(long)]
        frozen: bool,
        /// Never touch the network: use only cached git sources.
        #[arg(long)]
        offline: bool,
    },
    /// Re-render and merge template changes over local edits
    ///
    /// Re-render against the current template state (and, with `--answer`
    /// & co., changed answers) and 3-way merge the changes over local edits.
    /// In a terminal it first offers the project's answers to change, asks
    /// what the project never answered, and puts each conflict to you.
    Update {
        /// Scaffolded project directory (defaults to `.`).
        #[arg(default_value = ".")]
        dest: Utf8PathBuf,
        /// Print the plan without changing anything.
        #[arg(long)]
        dry_run: bool,
        /// With --dry-run: also print the unified diff of every planned write.
        #[arg(long, requires = "dry_run")]
        diff: bool,
        /// Use this local template path instead of the recorded source
        /// (and record it from now on).
        #[arg(long, conflicts_with = "to")]
        template: Option<Utf8PathBuf>,
        /// Move a remote-sourced project to another revision (git: tag,
        /// branch, or commit; hub: version) and track it from now on.
        #[arg(long, value_name = "REV")]
        to: Option<String>,
        /// Change an answer as KEY=VALUE (repeatable). Include answers are
        /// namespaced: `svc.port=8080`, `connector.stripe.port=8080`.
        /// Values derived from it (defaults, computed values, binds)
        /// follow; answers you gave stay.
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// Apply a named preset's answers (repeatable).
        #[arg(long = "preset")]
        presets: Vec<String>,
        /// TOML file with answers to change.
        #[arg(long = "answers-file")]
        answers_file: Option<Utf8PathBuf>,
        /// Answers to change as a JSON object (inline, `@file`, or `-`).
        #[arg(long = "answers-json")]
        answers_json: Option<String>,
        /// Hand an answer back to its default (or include bind), so it
        /// follows the other answers again (repeatable).
        #[arg(long = "unset", value_name = "ID")]
        unset: Vec<String>,
        /// Write even over files with uncommitted git changes (by default
        /// the update stops, so git can show and undo what the merge does).
        #[arg(long)]
        allow_dirty: bool,
        /// Do not run template tasks after merging.
        #[arg(long)]
        skip_tasks: bool,
        /// Never prompt: keep every answer, take new questions' defaults
        /// (fail on one without), and leave conflicts marked in the files.
        #[arg(long)]
        non_interactive: bool,
        /// Fail if a remote include isn't already pinned in weft.lock (CI).
        #[arg(long)]
        frozen: bool,
        /// Never touch the network: update from the cached git mirror only.
        #[arg(long)]
        offline: bool,
    },
    /// Show a project's answers and where each came from
    ///
    /// Show a scaffolded project's answers and where each came from:
    /// `given` (yours — every re-render starts from these), `derived`
    /// (defaults, computed values, include binds — they follow the other
    /// answers), or `secret` (resolved from its source, never stored).
    Answers {
        /// Scaffolded project directory (defaults to `.`).
        #[arg(default_value = ".")]
        dest: Utf8PathBuf,
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
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
    /// Manage patch hooks
    ///
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
    /// Commit the session's changes as a new patch
    ///
    /// Diff the session's worktree against its base and append the result as
    /// a new patch (with value abstraction).
    Commit {
        #[command(flatten)]
        scope: ctx::Scope,
        /// Name for the new patch. Asked in a terminal, offering patch-NNN,
        /// which is also what an unattended commit takes.
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
        /// Accept every abstraction proposal without prompting, and take
        /// patch-NNN when --name is left out.
        #[arg(long)]
        yes: bool,
        /// Keep one occurrence literal: ANSWER@PATH:LINE[:NTH] (repeatable;
        /// see `weft diff` for the keys). Implies accepting the rest.
        #[arg(long = "keep-literal")]
        keep_literal: Vec<String>,
        /// When changes remain, build the next patch on top of this one (its
        /// base advances to include it).
        #[arg(long, conflicts_with = "sibling")]
        stack: bool,
        /// When changes remain, keep the next patch independent of this one
        /// (an unstacked sibling that must commute). This is the default
        /// non-interactively.
        #[arg(long)]
        sibling: bool,
        /// Depend on these patches instead of the base's leaves (comma
        /// separated or repeated). They must be in the session's base, and
        /// the patch must still apply with only them present.
        #[arg(long = "depends-on", value_delimiter = ',')]
        depends_on: Vec<String>,
        /// Depend on exactly this patch — sugar for `--depends-on NAME`.
        #[arg(long, conflicts_with = "depends_on")]
        after: Option<String>,
        /// Share each file this patch creates with the patches that already
        /// create it: NAME is the new patch that owns the lines they share
        /// (see `weft share`).
        #[arg(long, value_name = "NAME")]
        share: Option<String>,
    },
    /// Show the session's uncommitted changes
    ///
    /// Show the session's changes before committing: the concrete diff with
    /// abstraction candidates highlighted.
    Diff {
        #[command(flatten)]
        scope: ctx::Scope,
        /// Show the stored form ({answer} placeholders) instead of values.
        #[arg(long)]
        abstracted: bool,
        /// Emit the preview as JSON (files, hunks, candidates, occurrences).
        #[arg(long)]
        json: bool,
        /// Preview only the staged changes (what the next commit will write),
        /// instead of the whole worktree.
        #[arg(long, visible_alias = "cached")]
        staged: bool,
    },
    /// Share a file that several patches create
    ///
    /// Share a file that several independent patches create: a new patch
    /// owns the lines they have in common, and each of them adds its own
    /// lines to a slot in it, so they can all be on at once. In a terminal,
    /// weft opens the combined file in $EDITOR for you to confirm.
    Share {
        /// The file(s), as paths in the project (`.mcp.json`).
        #[arg(required = true)]
        paths: Vec<String>,
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Name of the new patch that owns the shared lines.
        #[arg(long)]
        name: Option<String>,
        /// Title of the new patch (metadata, not hashed).
        #[arg(long)]
        title: Option<String>,
        /// Description of the new patch (metadata, not hashed).
        #[arg(long)]
        describe: Option<String>,
        /// Split a file as a combined file shows it, instead of confirming
        /// weft's proposal: PATH=FILE (repeatable).
        #[arg(long = "example", value_name = "PATH=FILE")]
        examples: Vec<String>,
        /// Accept weft's proposal without opening the editor.
        #[arg(long)]
        yes: bool,
        /// Print each combined file and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Stage worktree changes into the session index
    ///
    /// Stage worktree changes into the session index (like `git add`). The
    /// next `weft commit` commits only what is staged.
    Add {
        /// Path globs to stage, relative to the directory you run this in (to
        /// the worktree root when `--session` names it from outside).
        patterns: Vec<String>,
        #[command(flatten)]
        scope: ctx::Scope,
        /// Stage the whole worktree, wherever you run this from.
        #[arg(long, short = 'A')]
        all: bool,
        /// Choose what to stage hunk by hunk (like `git add -p`).
        #[arg(long, short = 'p')]
        patch: bool,
    },
    /// Unstage paths from the session index
    ///
    /// Unstage paths from the session index (like `git reset`). No paths
    /// unstages everything.
    Reset {
        /// Path globs to unstage, relative to the directory you run this in
        /// (to the worktree root when `--session` names it from outside).
        patterns: Vec<String>,
        #[command(flatten)]
        scope: ctx::Scope,
    },
    /// Show the session's staged and unstaged changes
    ///
    /// Show the session: staged and unstaged changes, plus the base and
    /// answers.
    Status {
        #[command(flatten)]
        scope: ctx::Scope,
    },
    /// Start, list, move, and finish sessions
    ///
    /// Start, list, move, and finish sessions — one worktree each, like
    /// `git worktree`.
    Session {
        #[command(subcommand)]
        cmd: SessionCmd,
    },
    /// List or show template presets.
    Presets {
        #[command(subcommand)]
        command: PresetsCommand,
    },
    /// Show the template's patch graph
    ///
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
    /// Write JSON Schemas for editor validation
    ///
    /// Write JSON Schemas for weft.toml and patches/*.json (editor
    /// validation + completion).
    Schema {
        /// Output directory.
        #[arg(long, default_value = "schemas")]
        out: Utf8PathBuf,
    },
    /// Run the weft language server
    ///
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
    /// Print the template's contract
    ///
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
    /// Validate the template
    ///
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
pub(crate) enum InstanceCmd {
    /// Add an instance of a repeatable include (renders its files in
    /// place).
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
        /// Write even over files with uncommitted git changes.
        #[arg(long)]
        allow_dirty: bool,
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
        /// Write even over files with uncommitted git changes.
        #[arg(long)]
        allow_dirty: bool,
    },
    /// List the project's include instances.
    List {
        #[arg(long, default_value = ".")]
        dest: Utf8PathBuf,
    },
}

#[derive(Subcommand)]
// Parsed once per process; boxing the fields would only obscure clap's derive.
#[allow(clippy::large_enum_variant)]
pub(crate) enum HookCmd {
    /// Add a hook to a patch
    ///
    /// Add a hook to a patch. Leave out --action to write the command in
    /// $VISUAL/$EDITOR.
    Add {
        /// Patch name (file stem) that owns the hook.
        patch: String,
        /// Template directory (defaults to `.`).
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Unique hook id (referenced by --after / --before / hook: inputs).
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
        /// Shell command to run (leave out to write it in $VISUAL/$EDITOR).
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
        /// Hook id this one must run before (repeatable); how an extender
        /// orders its hooks ahead of one it inherits.
        #[arg(long = "before")]
        before: Vec<String>,
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
pub(crate) enum SessionCmd {
    /// Start a session in a new worktree
    ///
    /// Start a session: render a base state into a worktree and print its
    /// path, so `cd $(weft session new)` drops you into it (or pass --shell).
    New {
        /// Session name (also its directory under `.weft-sessions/`).
        #[arg(default_value = weft_engine::session::DEFAULT_SESSION_NAME)]
        name: String,
        /// Where to put the worktree (must be empty or absent). Defaults to
        /// `<template>/.weft-sessions/<name>/worktree`.
        #[arg(long)]
        path: Option<Utf8PathBuf>,
        /// Template directory (defaults to the one found from here).
        #[arg(long)]
        template: Option<Utf8PathBuf>,
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
        /// Replace an existing session of this name instead of failing.
        #[arg(long)]
        force: bool,
        /// Never prompt: take every default, and fail on a question without
        /// one.
        #[arg(long)]
        non_interactive: bool,
        /// Open a shell in the new worktree once it is ready (`exit` returns
        /// here); needs a terminal. `weft session shell` does the same later.
        #[arg(long)]
        shell: bool,
    },
    /// Link an existing directory as a session's worktree
    ///
    /// Link a directory you already have as a session's worktree, so code you
    /// wrote in a real project can be promoted back into patches. A project
    /// scaffolded by `weft new` supplies its own template, base and answers.
    Adopt {
        /// The directory to adopt.
        path: Utf8PathBuf,
        /// Session name.
        #[arg(long, short = 'n', default_value = weft_engine::session::DEFAULT_SESSION_NAME)]
        name: String,
        /// Template to record against (required unless the directory was
        /// scaffolded by weft).
        #[arg(long)]
        template: Option<Utf8PathBuf>,
        /// Base state: `latest` or a patch name. Defaults to the project's
        /// own pinned base when scaffolded, `latest` otherwise.
        #[arg(long)]
        base: Option<String>,
        /// Apply a named preset (repeatable).
        #[arg(long = "preset")]
        presets: Vec<String>,
        /// Answer a question inline as KEY=VALUE (repeatable).
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// TOML file with answers.
        #[arg(long = "answers-file")]
        answers_file: Option<Utf8PathBuf>,
        /// Only look at paths matching this glob (repeatable). Without it the
        /// whole directory is diffed against the base, which on a project weft
        /// did not scaffold means every file reads as new.
        #[arg(long = "scope")]
        scope: Vec<String>,
        /// Replace an existing session of this name instead of failing.
        #[arg(long)]
        force: bool,
        /// Never prompt; fail if answers are missing.
        #[arg(long)]
        non_interactive: bool,
    },
    /// Show or change an adopted session's paths
    ///
    /// Show or change which paths an adopted session looks at. Outside a
    /// worktree, `--session` defaults to the template's only session.
    Scope {
        #[command(flatten)]
        scope: ctx::Scope,
        /// Start looking at paths matching this glob (repeatable).
        #[arg(long = "add")]
        add: Vec<String>,
        /// Stop looking at this glob (must match one already recorded).
        #[arg(long = "rm")]
        rm: Vec<String>,
    },
    /// List the template's sessions, with a marker for the one you are in.
    #[command(visible_alias = "ls")]
    List {
        /// Template directory (defaults to the one found from here).
        #[arg(long)]
        template: Option<Utf8PathBuf>,
    },
    /// Print a session's worktree path: `cd $(weft session path NAME)`.
    Path {
        /// Session name (defaults to the one you are in, or the only one).
        name: Option<String>,
        /// Template directory (defaults to the one found from here).
        #[arg(long)]
        template: Option<Utf8PathBuf>,
    },
    /// Open a shell or run a command in a session's worktree
    ///
    /// Open a shell in a session's worktree (`exit` returns here), or run
    /// one command there: `weft session shell NAME -- code .`.
    ///
    /// The shell is `$SHELL`, else /bin/sh, with `WEFT_SESSION` naming the
    /// session, and needs a terminal; a command after `--` runs without one,
    /// and weft exits with its status (127 when it is not found). No program
    /// can move the shell that started it, so this is a new shell: `cd
    /// $(weft session path NAME)` stays in the current one.
    Shell {
        /// Session name (defaults to the one you are in, or the only one).
        name: Option<String>,
        /// Template directory (defaults to the one found from here).
        #[arg(long)]
        template: Option<Utf8PathBuf>,
        /// A command to run in the worktree instead of a shell.
        #[arg(last = true, value_name = "CMD")]
        command: Vec<String>,
    },
    /// Move a session's worktree to another directory
    ///
    /// Move a session's worktree to another directory (like `git worktree
    /// move`). Moving it with `mv` also works — weft repairs the record.
    Move {
        /// Session name.
        name: String,
        /// The new worktree location (must be empty or absent).
        dest: Utf8PathBuf,
        /// Template directory (defaults to the one found from here).
        #[arg(long)]
        template: Option<Utf8PathBuf>,
    },
    /// Re-render the base and merge it onto the worktree
    ///
    /// Re-read weft.toml, re-resolve answers, re-render the base, and 3-way
    /// merge it onto the worktree (preserving your edits). Use after editing
    /// the manifest or to change answers mid-session. Outside a worktree,
    /// `--session` defaults to the template's only session.
    Refresh {
        #[command(flatten)]
        scope: ctx::Scope,
        /// Apply a named preset (repeatable).
        #[arg(long = "preset")]
        presets: Vec<String>,
        /// Change an answer inline as KEY=VALUE (repeatable).
        #[arg(long = "answer")]
        answers: Vec<String>,
        /// TOML file with answers.
        #[arg(long = "answers-file")]
        answers_file: Option<Utf8PathBuf>,
        /// Answers as a JSON object.
        #[arg(long = "answers-json")]
        answers_json: Option<String>,
        /// Never prompt; fail if answers are missing.
        #[arg(long)]
        non_interactive: bool,
    },
    /// End a session
    ///
    /// End a session. Refuses if the worktree has uncommitted changes unless
    /// `--discard`. A worktree weft created is removed; an adopted one is
    /// only unlinked.
    End {
        /// Session name (defaults to the one you are in, or the only one).
        name: Option<String>,
        /// Template directory (defaults to the one found from here).
        #[arg(long)]
        template: Option<Utf8PathBuf>,
        /// End even with uncommitted changes, throwing them away.
        #[arg(long)]
        discard: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum PatchCmd {
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
    /// Re-run a generator patch's command and rewrite its ops
    ///
    /// Re-run the stored generator command of patches recorded with
    /// `weft session new --exec` and rewrite their ops from the fresh output.
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
        /// Accept answer references the previous version of the patch did
        /// not have.
        #[arg(long)]
        yes: bool,
        /// Emit the report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Replace a generator patch's stored command
    ///
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
    /// Turn a generator patch into a plain patch
    ///
    /// Drop a patch's generator metadata: it becomes a plain, freely
    /// editable patch (`weft patch amend`), but `resync` no longer applies.
    Detach {
        /// Patch name (file stem).
        name: String,
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
    },
    /// Combine several patches into one
    ///
    /// Combine several patches into one (their ops concatenated in dependency
    /// order). Members must be convex in the graph, share a gate, and be
    /// neither generator nor foreach patches.
    Squash {
        /// Patch names to combine (two or more).
        names: Vec<String>,
        #[arg(long, default_value = ".")]
        template: Utf8PathBuf,
        /// Name of the resulting patch (may reuse a member's name).
        #[arg(long)]
        into: String,
        /// Title for the combined patch (defaults to the tip member's).
        #[arg(long)]
        title: Option<String>,
    },
    /// Edit a patch's recorded content
    ///
    /// Edit a patch's recorded content: reopens its contribution in a
    /// scratch worktree; `weft commit` re-derives its ops in place (id
    /// changes, like any content edit). Dependents replay over the new
    /// content; commit reports any whose hunks no longer apply.
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
        /// Never prompt: take every default, and fail on a question without
        /// one.
        #[arg(long)]
        non_interactive: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum HubCmd {
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
pub(crate) enum PresetsCommand {
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
    /// Create or update a preset
    ///
    /// Create or update a preset: --answer locks an answer, --fix and
    /// --block constrain a multichoice.
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
    },
    /// Remove a preset: its declaration in weft.toml and its file.
    Rm {
        name: String,
        #[arg(default_value = ".")]
        template: Utf8PathBuf,
    },
}
