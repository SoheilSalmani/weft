mod addpatch;
mod ctx;
mod diffcmd;
mod git;
mod hub;
mod lsp;
mod mcp;
mod schema;
mod shell;
mod source;

use std::io::IsTerminal;

use anyhow::Context as _;
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
    /// Re-render against the current template state (and, with `--answer`
    /// & co., changed answers) and 3-way merge the changes over local edits.
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
        /// Never prompt; fail if new questions lack answers.
        #[arg(long)]
        non_interactive: bool,
        /// Fail if a remote include isn't already pinned in weft.lock (CI).
        #[arg(long)]
        frozen: bool,
        /// Never touch the network: update from the cached git mirror only.
        #[arg(long)]
        offline: bool,
    },
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
    /// Unstage paths from the session index (like `git reset`). No paths
    /// unstages everything.
    Reset {
        /// Path globs to unstage, relative to the directory you run this in
        /// (to the worktree root when `--session` names it from outside).
        patterns: Vec<String>,
        #[command(flatten)]
        scope: ctx::Scope,
    },
    /// Show the session: staged and unstaged changes, plus the base and
    /// answers.
    Status {
        #[command(flatten)]
        scope: ctx::Scope,
    },
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
enum HookCmd {
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
enum SessionCmd {
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
            frozen,
            offline,
        } => {
            // Remote sources (hub, git) are fetched into the local caches;
            // the state file records the resolved ref, never the cache path.
            let source = source::Source::parse(&template)?;
            let located = source::fetch(&source, offline)?;
            let opts = NewOptions {
                template: located.dir,
                dest,
                presets,
                answers,
                answers_file,
                answers_json,
                instances,
                skip_tasks,
                stored: located.stored,
            };
            let mut interaction = auto_interaction(non_interactive);
            let mut resolver =
                source::RemoteResolver::new(hub::registry_url(None).ok(), frozen).offline(offline);
            let result = weft_engine::new::run(&opts, &mut resolver, interaction.as_mut());
            resolver.flush()?;
            let entered = result?;
            // Offer to capture the answers as a preset once a person typed
            // some — local templates only (a cache checkout isn't the
            // author's working tree).
            if !entered.is_empty() && source.is_path() {
                if let Err(e) = offer_preset_capture(
                    &opts.template,
                    opts.answers_file.as_deref(),
                    &opts.answers,
                    opts.answers_json.as_deref(),
                    &entered,
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
            diff,
            template,
            to,
            answers,
            presets,
            answers_file,
            answers_json,
            unset,
            allow_dirty,
            skip_tasks,
            non_interactive,
            frozen,
            offline,
        } => {
            let changes = weft_engine::update::AnswerChanges {
                presets,
                answers,
                answers_file,
                answers_json,
                unset,
            };
            let state = weft_engine::state::State::load(&dest)?;
            let (template_dir, stored) = match template {
                // A local path override replaces the recorded source.
                Some(dir) => (dir, None),
                None => {
                    let located = source::locate_project(&state, to.as_deref(), offline)?;
                    // Nothing to re-render when neither the source nor the
                    // answers move, and no pending hook waits to run.
                    let still = changes.is_empty() && state.state.pending_hooks.is_empty();
                    if let (true, Some(s)) = (still, &located.stored) {
                        if let Some(commit) = &s.commit {
                            if s.template == state.state.template
                                && state.state.commit.as_deref() == Some(commit)
                            {
                                eprintln!("up to date: {} at {}", s.template, git::short(commit));
                                return Ok(());
                            }
                        }
                    }
                    (located.dir, located.stored)
                }
            };

            let opts = weft_engine::update::UpdateOptions {
                dest,
                dry_run,
                diff,
                template_override: Some(template_dir),
                stored,
                skip_tasks,
                drop_instances: vec![],
                answers: changes,
                allow_dirty,
            };
            let mut interaction = auto_interaction(non_interactive);
            let mut resolver =
                source::RemoteResolver::new(hub::registry_url(None).ok(), frozen).offline(offline);
            let report = weft_engine::update::run(&opts, &mut resolver, interaction.as_mut())?;
            resolver.flush()?;
            if opts.dry_run {
                Ok(())
            } else {
                weft_engine::update::finish(&report)
            }
        }
        Command::Answers { dest, json } => {
            let state = weft_engine::state::State::load(&dest)?;
            let rows = state.answer_rows();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({ "answers": rows }))?
                );
                return Ok(());
            }
            let width = rows.iter().map(|r| r.id.len()).max().unwrap_or(0);
            let shown: Vec<String> = rows
                .iter()
                .map(|r| match (&r.value, &r.source) {
                    (Some(v), _) => weft_engine::update::show_value(v),
                    (None, Some(src)) => src.clone(),
                    (None, None) => String::new(),
                })
                .collect();
            let value_width = shown.iter().map(|s| s.chars().count()).max().unwrap_or(0);
            for (row, value) in rows.iter().zip(&shown) {
                println!(
                    "{:width$}  {value:value_width$}  {}",
                    row.id,
                    row.origin.label()
                );
            }
            eprintln!(
                "change one with `weft update --answer ID=VALUE`, or hand one back to its \
                 default with `--unset ID`"
            );
            Ok(())
        }
        Command::Instance { cmd } => match cmd {
            InstanceCmd::Add {
                include,
                key,
                answers,
                dest,
                skip_tasks,
                allow_dirty,
                non_interactive,
            } => {
                let state = weft_engine::state::State::load(&dest)?;
                let source = source::locate_project(&state, None, false)?;
                let mut interaction = auto_interaction(non_interactive);
                let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
                let report = weft_engine::instance::add(
                    &weft_engine::instance::InstanceAddOptions {
                        dest,
                        include,
                        key,
                        answers,
                        skip_tasks,
                        allow_dirty,
                        source,
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
                allow_dirty,
            } => {
                let state = weft_engine::state::State::load(&dest)?;
                let source = source::locate_project(&state, None, false)?;
                let mut interaction = auto_interaction(false);
                let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
                let report = weft_engine::instance::remove(
                    &weft_engine::instance::InstanceRemoveOptions {
                        dest,
                        include,
                        key,
                        skip_tasks,
                        allow_dirty,
                        source,
                    },
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
                before,
                inputs,
            } => {
                // No --action: write the command in $VISUAL/$EDITOR.
                let action = match action {
                    Some(action) => action,
                    None => weft_engine::interact::edit_command(
                        "Enter the hook's shell command.\n\
                         Lines starting with '#' are ignored. Multi-line commands run \
                         via sh -c.\n\
                         ${answer_or_expr} interpolates declared answers; unknown ${...} \
                         passes through to the shell.",
                    )?,
                };
                let opts = weft_engine::author::HookAddOptions {
                    patch,
                    id,
                    phase,
                    effect,
                    label,
                    action,
                    description,
                    when,
                    after,
                    before,
                    inputs,
                };
                source::with_resolver(|r| weft_engine::author::hook_add(&template, &opts, r))
            }
            HookCmd::Rm {
                patch,
                id,
                template,
            } => source::with_resolver(|r| weft_engine::author::hook_rm(&template, &patch, &id, r)),
            HookCmd::Ls { template } => {
                source::with_resolver(|r| weft_engine::author::hook_ls(&template, r))
            }
        },
        Command::Patch { cmd } => match cmd {
            PatchCmd::Set {
                name,
                template,
                title,
                describe,
                tags,
                clear_tags,
            } => {
                let opts = weft_engine::author::PatchSetOptions {
                    name,
                    title,
                    describe,
                    tags,
                    clear_tags,
                };
                source::with_resolver(|r| weft_engine::author::patch_set(&template, &opts, r))
            }
            PatchCmd::Ls { template } => {
                source::with_resolver(|r| weft_engine::author::patch_ls(&template, r))
            }
            PatchCmd::Resync {
                names,
                template,
                all,
                answers,
                keep_literal,
                dry_run,
                yes,
                json,
            } => {
                let opts = weft_engine::generate::ResyncOptions {
                    template,
                    names,
                    all,
                    answers,
                    keep_literal,
                    dry_run,
                    accept_new_refs: yes,
                };
                let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
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
                source::with_resolver(|r| {
                    weft_engine::author::set_generator_command(&template, &name, &command, r)
                })?;
                if resync {
                    let opts = weft_engine::generate::ResyncOptions {
                        template,
                        names: vec![name],
                        all: false,
                        answers: vec![],
                        keep_literal: vec![],
                        dry_run: false,
                        accept_new_refs: false,
                    };
                    let mut resolver =
                        source::RemoteResolver::new(hub::registry_url(None).ok(), false);
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
            PatchCmd::Detach { name, template } => source::with_resolver(|r| {
                weft_engine::author::detach_generator(&template, &name, r)
            }),
            PatchCmd::Squash {
                names,
                template,
                into,
                title,
            } => source::with_resolver(|r| {
                weft_engine::squash::squash(
                    &template,
                    &weft_engine::squash::SquashOptions { names, into, title },
                    r,
                )
            }),
            PatchCmd::Amend {
                name,
                template,
                presets,
                answers,
                answers_file,
                force,
                non_interactive,
            } => {
                let opts = weft_engine::amend::AmendOptions {
                    template,
                    // The edit gets a session named after the patch.
                    session: name.clone(),
                    name,
                    presets,
                    answers,
                    answers_file,
                    force,
                };
                let mut interaction = auto_interaction(non_interactive);
                let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
                let worktree =
                    weft_engine::amend::start(&opts, &mut resolver, interaction.as_mut())?;
                resolver.flush()?;
                shell::announce_amend(&opts.template, &opts.name, &worktree);
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
                let mut resolver = source::RemoteResolver::new(Some(registry.clone()), false);
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
            let mut resolver = source::RemoteResolver::new(registry, false);
            if upgrade {
                resolver.clear_lock(&template)?;
            }
            Template::load_with(&template, &mut resolver)?;
            resolver.flush()?;
            eprintln!("lock up to date");
            Ok(())
        }
        Command::Commit {
            scope,
            name,
            when,
            title,
            describe,
            tags,
            yes,
            keep_literal,
            stack,
            sibling,
            depends_on,
            after,
            share,
        } => {
            let depends_on = match (depends_on.is_empty(), after) {
                (true, None) => None,
                (_, Some(one)) => Some(vec![one]),
                (false, None) => Some(depends_on),
            };
            let link = if stack {
                Some(weft_engine::commit::CommitLink::Stack)
            } else if sibling {
                Some(weft_engine::commit::CommitLink::Sibling)
            } else {
                None
            };
            let here = scope.resolve()?;
            let session = weft_engine::session::Session::load(&here.template, &here.session).ok();
            // Asked before committing: a commit that ends the session deletes
            // the worktree the answer depends on.
            let under_shell = session
                .as_ref()
                .is_some_and(|s| shell::stands_in(s, &here.template, &here.session));
            let opts = weft_engine::commit::CommitOptions {
                template: here.template,
                session: here.session,
                name,
                when,
                title,
                describe,
                tags,
                decisions: None,
                keep_literal,
                link,
                depends_on,
                share,
            };
            let mut interaction = auto_interaction(yes);
            let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
            let result = weft_engine::commit::run(&opts, &mut resolver, interaction.as_mut());
            resolver.flush()?;
            result?;
            if under_shell && !weft_engine::session::Session::exists(&opts.template, &opts.session)
            {
                shell::exit_hint();
            }
            Ok(())
        }
        Command::Share {
            paths,
            template,
            name,
            title,
            describe,
            examples,
            yes,
            dry_run,
        } => {
            let examples = examples
                .iter()
                .map(|spec| {
                    let (path, file) = spec
                        .split_once('=')
                        .with_context(|| format!("--example expects PATH=FILE, got `{spec}`"))?;
                    let text = std::fs::read_to_string(file)
                        .with_context(|| format!("reading the combined file {file}"))?;
                    Ok((path.trim_start_matches("./").to_owned(), text))
                })
                .collect::<anyhow::Result<_>>()?;
            let opts = weft_engine::share::ShareOptions {
                template,
                paths,
                name,
                title,
                describe,
                examples,
                dry_run,
            };
            let mut interaction = auto_interaction(yes);
            let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
            let report = weft_engine::share::run(&opts, &mut resolver, interaction.as_mut());
            resolver.flush()?;
            let report = report?;
            if dry_run {
                for (path, text) in &report.files {
                    println!("{path}, with every patch on:");
                    print!("{text}");
                }
                eprintln!(
                    "dry run: would create patch `{}` and rewrite {}",
                    report.owner,
                    report
                        .rewritten
                        .iter()
                        .map(|n| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            } else {
                for line in &report.lines {
                    eprintln!("{line}");
                }
                eprintln!("run `weft check` with answers that turn them on");
            }
            Ok(())
        }
        Command::Diff {
            scope,
            abstracted,
            json,
            staged,
        } => {
            let here = scope.resolve()?;
            if staged && !weft_engine::stage::exists(&here.template, &here.session) {
                eprintln!("nothing staged; `weft diff` shows the whole worktree");
                return Ok(());
            }
            let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
            let tpl = Template::load_with(&here.template, &mut resolver)?;
            resolver.flush()?;
            let mut interaction = auto_interaction(false);
            let preview = weft_engine::commit::preview_target(
                &tpl,
                &here.session,
                staged,
                interaction.as_mut(),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&preview)?);
                Ok(())
            } else {
                diffcmd::print(&preview, abstracted)
            }
        }
        Command::Add {
            patterns,
            scope,
            all,
            patch,
        } => {
            let here = scope.resolve()?;
            let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
            let tpl = Template::load_with(&here.template, &mut resolver)?;
            resolver.flush()?;
            // `-A` means the whole worktree wherever you stand; everything
            // else is relative to the directory you ran this in, like git.
            // `-p` with no paths walks every change, like `git add -p`.
            let pats: Vec<String> = if all {
                vec!["**".to_owned()]
            } else if patterns.is_empty() {
                if !patch {
                    anyhow::bail!("specify paths to stage (globs), or pass -A to stage everything");
                }
                vec![ctx::qualify(&here.prefix, ".")]
            } else {
                patterns
                    .iter()
                    .map(|p| ctx::qualify(&here.prefix, p))
                    .collect()
            };
            let globs = weft_engine::stage::globset(&pats)?;
            let mut interaction = auto_interaction(true);
            let trees =
                weft_engine::commit::session_trees(&tpl, &here.session, interaction.as_mut())?;
            let mut staged = weft_engine::stage::staged_tree(
                &here.template,
                &here.session,
                &tpl.ignore,
                &trees.base_tree,
            )?;
            if !all && !patterns.is_empty() {
                let missing = weft_engine::stage::unmatched(
                    &globs,
                    &[&trees.base_tree, &staged, &trees.work_tree],
                );
                ctx::refuse_unmatched(&here, &patterns, &missing, &trees.sess.session.scope)?;
            }
            let n = if patch {
                let paths: Vec<_> = weft_engine::stage::changed_paths(&staged, &trees.work_tree)
                    .into_iter()
                    .filter(|p| globs.is_match(p.as_str()))
                    .collect();
                if paths.is_empty() {
                    eprintln!("nothing to stage — the staged tree already matches the worktree");
                    return Ok(());
                }
                addpatch::stage_interactively(
                    &mut std::io::stdout(),
                    &addpatch::Paint {
                        color: std::io::stdout().is_terminal(),
                    },
                    &mut addpatch::TerminalDecider::default(),
                    &mut staged,
                    &trees.work_tree,
                    &paths,
                )?
            } else {
                weft_engine::stage::add(&mut staged, &trees.work_tree, &trees.base_tree, &globs)
            };
            weft_engine::stage::write(&here.template, &here.session, &staged, &trees.base_tree)?;
            eprintln!("staged {n} path(s)");
            Ok(())
        }
        Command::Reset { patterns, scope } => {
            let here = scope.resolve()?;
            let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
            let tpl = Template::load_with(&here.template, &mut resolver)?;
            resolver.flush()?;
            let mut interaction = auto_interaction(true);
            let trees =
                weft_engine::commit::session_trees(&tpl, &here.session, interaction.as_mut())?;
            let mut staged = weft_engine::stage::staged_tree(
                &here.template,
                &here.session,
                &tpl.ignore,
                &trees.base_tree,
            )?;
            let globs = if patterns.is_empty() {
                None
            } else {
                let pats: Vec<String> = patterns
                    .iter()
                    .map(|p| ctx::qualify(&here.prefix, p))
                    .collect();
                Some(weft_engine::stage::globset(&pats)?)
            };
            if let Some(globs) = &globs {
                let missing = weft_engine::stage::unmatched(
                    globs,
                    &[&trees.base_tree, &staged, &trees.work_tree],
                );
                ctx::refuse_unmatched(&here, &patterns, &missing, &trees.sess.session.scope)?;
            }
            weft_engine::stage::reset(&mut staged, &trees.base_tree, globs.as_ref());
            weft_engine::stage::write(&here.template, &here.session, &staged, &trees.base_tree)?;
            eprintln!("unstaged");
            Ok(())
        }
        Command::Status { scope } => {
            let here = scope.resolve()?;
            let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
            let tpl = Template::load_with(&here.template, &mut resolver)?;
            resolver.flush()?;
            let sess = weft_engine::session::Session::load(&here.template, &here.session)?;
            let mut interaction = auto_interaction(true);
            let trees =
                weft_engine::commit::session_trees(&tpl, &here.session, interaction.as_mut())?;
            let staged = weft_engine::stage::staged_tree(
                &here.template,
                &here.session,
                &tpl.ignore,
                &trees.base_tree,
            )?;
            let staged_changes = weft_engine::stage::changed_paths(&trees.base_tree, &staged);
            let unstaged_changes = weft_engine::stage::changed_paths(&staged, &trees.work_tree);
            println!(
                "session `{}` on `{}`",
                here.session, tpl.manifest.template.name
            );
            println!("  worktree: {}", here.worktree);
            let base_names: Vec<_> = sess
                .session
                .base
                .iter()
                .filter_map(|id| tpl.id_to_name.get(id).cloned())
                .collect();
            println!(
                "  base: {}",
                if base_names.is_empty() {
                    "(empty template)".to_owned()
                } else {
                    base_names.join(", ")
                }
            );
            if !sess.session.scope.is_empty() {
                println!("  scope: {}", sess.session.scope.join(", "));
            }
            for (id, value) in sess.answers.iter() {
                println!("  answer: {} = {}", id.0, value.render_text());
            }
            for id in sess.secrets.keys() {
                println!("  answer: {} = (secret)", id.0);
            }
            println!();
            if staged_changes.is_empty() {
                println!("no staged changes (a plain `weft commit` commits the whole worktree)");
            } else {
                println!("staged (the next commit takes these):");
                for p in &staged_changes {
                    println!("  {p}");
                }
            }
            if !unstaged_changes.is_empty() {
                println!("unstaged:");
                for p in &unstaged_changes {
                    println!("  {p}");
                }
            }
            Ok(())
        }
        Command::Session { cmd } => match cmd {
            SessionCmd::New {
                name,
                path,
                template,
                base,
                presets,
                answers,
                answers_file,
                foreach,
                exec,
                force,
                non_interactive,
                shell: open_shell,
            } => {
                // Refused before anything is rendered or written, so no
                // half-made session is left behind.
                if open_shell && !shell::has_terminal() {
                    anyhow::bail!(
                        "--shell opens a shell in the new worktree and needs a terminal; \
                         drop it and `cd` into the path `weft session new` prints"
                    );
                }
                let template = ctx::Scope {
                    template,
                    session: None,
                }
                .template()?;
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
                let opts = weft_engine::start::StartOptions {
                    template,
                    name,
                    path,
                    base,
                    presets,
                    answers,
                    answers_file,
                    answers_json: None,
                    foreach,
                    exec,
                    force,
                };
                let mut interaction = auto_interaction(non_interactive);
                let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
                let worktree = weft_engine::start::run(&opts, &mut resolver, interaction.as_mut())?;
                resolver.flush()?;
                shell::announce_new(&opts.template, &opts.name, &worktree, open_shell);
                if open_shell {
                    shell::enter(&worktree, &opts.name, &[]);
                }
                Ok(())
            }
            SessionCmd::Adopt {
                path,
                name,
                template,
                base,
                presets,
                answers,
                answers_file,
                scope,
                force,
                non_interactive,
            } => {
                let opts = weft_engine::adopt::AdoptOptions {
                    path,
                    name,
                    template,
                    base,
                    presets,
                    answers,
                    answers_file,
                    answers_json: None,
                    scope,
                    force,
                };
                let mut interaction = auto_interaction(non_interactive);
                let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
                let result = weft_engine::adopt::run(&opts, &mut resolver, interaction.as_mut());
                resolver.flush()?;
                let (template_root, worktree) = result?;
                println!("{worktree}");
                eprintln!(
                    "session `{}` adopted `{worktree}` against `{template_root}` — \
                     `weft status` shows what it sees",
                    opts.name
                );
                Ok(())
            }
            SessionCmd::Scope { scope, add, rm } => {
                let here = scope.resolve_or_only()?;
                let mut sess = weft_engine::session::Session::load(&here.template, &here.session)?;
                for glob in &rm {
                    if !sess.session.scope.iter().any(|g| g == glob) {
                        anyhow::bail!("`{glob}` is not in this session's scope");
                    }
                    sess.session.scope.retain(|g| g != glob);
                }
                for glob in add {
                    if !sess.session.scope.contains(&glob) {
                        sess.session.scope.push(glob);
                    }
                }
                // Validate before storing: a bad glob would break every later
                // command on this session.
                weft_engine::stage::globset(&sess.session.scope)?;
                sess.save(&here.template, &here.session)?;
                if sess.session.scope.is_empty() {
                    println!("session `{}` looks at the whole worktree", here.session);
                } else {
                    println!("session `{}` looks at:", here.session);
                    for glob in &sess.session.scope {
                        println!("  {glob}");
                    }
                }
                Ok(())
            }
            SessionCmd::List { template } => {
                let template = ctx::Scope {
                    template,
                    session: None,
                }
                .template()?;
                let sessions = weft_engine::session::Session::list(&template)?;
                if sessions.is_empty() {
                    eprintln!("no sessions in `{template}`; start one with `weft session new`");
                    return Ok(());
                }
                // Mark the session whose worktree we are standing in.
                let current = weft_engine::discover::cwd()
                    .ok()
                    .and_then(|c| weft_engine::discover::locate(&c))
                    .filter(|l| l.template_root == template)
                    .and_then(|l| l.session);
                for (name, sess) in sessions {
                    let mark = if current.as_deref() == Some(name.as_str()) {
                        "*"
                    } else {
                        " "
                    };
                    let kind = if sess.session.adopted {
                        "  (adopted)"
                    } else {
                        ""
                    };
                    println!("{mark} {name}\t{}{kind}", sess.worktree(&template, &name));
                }
                Ok(())
            }
            SessionCmd::Path { name, template } => {
                let here = ctx::Scope {
                    template,
                    session: name,
                }
                .resolve_or_only()?;
                println!("{}", here.worktree);
                Ok(())
            }
            SessionCmd::Shell {
                name,
                template,
                command,
            } => {
                let here = ctx::Scope {
                    template,
                    session: name,
                }
                .resolve_or_only()?;
                if !here.worktree.is_dir() {
                    anyhow::bail!(
                        "session `{0}` has no worktree at `{1}`; \
                         `weft session end {0} --discard{2}` forgets it",
                        here.session,
                        here.worktree,
                        shell::template_arg(&here.template)
                    );
                }
                if command.is_empty() && !shell::has_terminal() {
                    anyhow::bail!(
                        "`weft session shell` without a command opens a shell and needs a \
                         terminal; run one command there with `{} -- CMD`",
                        shell::command_for(&here.template, &here.session)
                    );
                }
                shell::enter(&here.worktree, &here.session, &command)
            }
            SessionCmd::Move {
                name,
                dest,
                template,
            } => {
                let here = ctx::Scope {
                    template,
                    session: Some(name),
                }
                .resolve()?;
                let dest = weft_engine::discover::absolute(&dest);
                if dest.exists()
                    && dest
                        .read_dir_utf8()
                        .map(|mut d| d.next().is_some())
                        .unwrap_or(true)
                {
                    anyhow::bail!("`{dest}` is not an empty or absent directory");
                }
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if dest.exists() {
                    std::fs::remove_dir(&dest)?;
                }
                std::fs::rename(&here.worktree, &dest)
                    .with_context(|| format!("moving {} to {dest}", here.worktree))?;
                let mut sess = weft_engine::session::Session::load(&here.template, &here.session)?;
                sess.session.worktree = Some(dest.clone());
                sess.save(&here.template, &here.session)?;
                ctx::link(&dest, &here.template, &here.session)?;
                println!("{dest}");
                eprintln!("moved session `{}` to `{dest}`", here.session);
                Ok(())
            }
            SessionCmd::Refresh {
                scope,
                presets,
                answers,
                answers_file,
                answers_json,
                non_interactive,
            } => {
                let here = scope.resolve_or_only()?;
                let opts = weft_engine::refresh::RefreshOptions {
                    template: here.template,
                    session: here.session,
                    presets,
                    answers,
                    answers_file,
                    answers_json,
                };
                let mut interaction = auto_interaction(non_interactive);
                let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
                let result = weft_engine::refresh::run(&opts, &mut resolver, interaction.as_mut());
                resolver.flush()?;
                let report = result?;
                eprintln!(
                    "session refreshed: {} file(s) updated{}",
                    report.updated,
                    if report.conflicts.is_empty() {
                        String::new()
                    } else {
                        format!(", {} conflict(s)", report.conflicts.len())
                    }
                );
                for c in &report.conflicts {
                    eprintln!("  {c} — resolve the conflict markers");
                }
                for n in &report.notes {
                    eprintln!("  note: {n}");
                }
                if report.conflicts.is_empty() {
                    Ok(())
                } else {
                    std::process::exit(1);
                }
            }
            SessionCmd::End {
                name,
                template,
                discard,
            } => {
                let scope = ctx::Scope {
                    template,
                    session: name,
                };
                if weft_engine::session::Session::list(&scope.template()?)?.is_empty() {
                    eprintln!("no session to end");
                    return Ok(());
                }
                let here = scope.resolve_or_only()?;
                let sess = weft_engine::session::Session::load(&here.template, &here.session)?;
                if !discard {
                    let mut resolver =
                        source::RemoteResolver::new(hub::registry_url(None).ok(), false);
                    let tpl = Template::load_with(&here.template, &mut resolver)?;
                    resolver.flush()?;
                    let mut interaction = auto_interaction(true);
                    let trees = weft_engine::commit::session_trees(
                        &tpl,
                        &here.session,
                        interaction.as_mut(),
                    )?;
                    let changed =
                        weft_engine::stage::changed_paths(&trees.base_tree, &trees.work_tree);
                    if !changed.is_empty() {
                        anyhow::bail!(
                            "{} uncommitted change(s) in session `{}`; commit them, \
                             or pass --discard to throw them away",
                            changed.len(),
                            here.session
                        );
                    }
                }
                let adopted = sess.session.adopted;
                let under_shell = shell::stands_in(&sess, &here.template, &here.session);
                sess.end(&here.template, &here.session)?;
                eprintln!(
                    "session `{}` ended{}",
                    here.session,
                    if adopted {
                        " (its worktree was left in place)"
                    } else {
                        ""
                    }
                );
                if under_shell {
                    shell::exit_hint();
                }
                Ok(())
            }
        },
        Command::Presets { command } => match command {
            PresetsCommand::List { template } => {
                let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
                let template = Template::load_with(&template, &mut resolver)?;
                resolver.flush()?;
                for preset in &template.manifest.presets {
                    println!("{}\t{}", preset.name, preset.file);
                }
                Ok(())
            }
            PresetsCommand::Show { name, template } => {
                let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
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
            } => {
                use weft_engine::preset::{PresetEntry, PresetSpec};
                let template = source::load_template(&dir)?;

                // --answer locks an answer; --fix/--block constrain a multichoice.
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
                source::with_resolver(|r| weft_engine::preset::save(&dir, &name, &spec, r))?;
                println!("saved preset `{name}`");
                Ok(())
            }
            PresetsCommand::Rm {
                name,
                template: dir,
            } => {
                source::with_resolver(|r| weft_engine::preset::remove(&dir, &name, r))?;
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
            let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
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
            let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), false);
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
            let mut resolver = source::RemoteResolver::new(hub::registry_url(None).ok(), frozen);
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

/// After `weft new` against a *local* template asked something, offer once
/// to capture the person's own answers (flags, answers file, JSON, and what
/// they typed; not preset locks or defaults) as a new preset in that
/// template.
fn offer_preset_capture(
    template_dir: &Utf8PathBuf,
    answers_file: Option<&camino::Utf8Path>,
    answer_args: &[String],
    answers_json: Option<&str>,
    entered: &weft_core::AnswerSet,
    interaction: &mut dyn weft_engine::interact::Interaction,
) -> anyhow::Result<()> {
    use weft_engine::preset::{PresetEntry, PresetSpec};
    let template = source::load_template(template_dir)?;
    // No presets selected here: only the person's own layers are captured.
    let mut captured = weft_engine::answers::layered_with_json(
        &template,
        &[],
        answers_file,
        answer_args,
        answers_json,
    )?;
    captured.overlay(entered);
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
        narrowing: Default::default(),
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
    source::with_resolver(|r| weft_engine::preset::save(template_dir, &name, &spec, r))?;
    println!("saved preset `{name}` in {template_dir}");
    Ok(())
}
