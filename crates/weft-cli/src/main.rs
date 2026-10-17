mod addpatch;
mod cli;
mod ctx;
mod diffcmd;
mod git;
mod hub;
mod lsp;
mod mcp;
mod prompt;
mod schema;
mod shell;
mod source;
mod ui;

use std::process::ExitCode;

use anyhow::Context as _;
use camino::Utf8PathBuf;
use cli::{Command, HookCmd, HubCmd, InstanceCmd, PatchCmd, PresetsCommand, SessionCmd};
use prompt::auto_interaction;
use ui::{palette, Out};
use weft_engine::new::NewOptions;
use weft_engine::report;
use weft_engine::template::Template;

fn main() -> ExitCode {
    let cli = cli::parse();
    ui::init(cli.color, cli.command.presentation(), cli.title);
    ui::finish(run(cli.command))
}

fn run(command: Command) -> anyhow::Result<()> {
    match command {
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
                    report::warning(format_args!("preset capture skipped: {e:#}"));
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
                    (located.dir, located.stored)
                }
            };
            let mut interaction = auto_interaction(non_interactive);
            // A bare update in a terminal first offers the project's answers
            // to change; flags already say what changes, and a dry run only
            // previews.
            let review = if !dry_run && changes.is_empty() && interaction.interactive() {
                offer_answer_review(&state, interaction.as_mut())?
            } else {
                weft_engine::update::Review::default()
            };
            // Nothing to re-render when neither the source nor the answers
            // move, and no held-back hook waits to run.
            if changes.is_empty() && review.is_empty() && state.state.pending_hooks.is_empty() {
                if let Some(s) = &stored {
                    if let Some(commit) = &s.commit {
                        if s.template == state.state.template
                            && state.state.commit.as_deref() == Some(commit)
                        {
                            report::done(format_args!(
                                "up to date: {} at {}",
                                s.template,
                                git::short(commit)
                            ));
                            return Ok(());
                        }
                    }
                }
            }

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
                review,
            };
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
                ui::outln!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({ "answers": rows }))?
                );
                return Ok(());
            }
            print_answers(&rows);
            report::hint(
                "change them with `weft update` in a terminal, one with `weft update --answer \
                 ID=VALUE`, or hand one back to its default with `--unset ID`",
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
            InstanceCmd::List { dest } => {
                print_instances(&weft_engine::instance::list(&dest)?);
                Ok(())
            }
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
                    None => prompt::edit_command(
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
                let rows = source::with_resolver(|r| weft_engine::author::hook_ls(&template, r))?;
                print_hooks(&rows);
                Ok(())
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
                let rows = source::with_resolver(|r| weft_engine::author::patch_ls(&template, r))?;
                print_patches(&rows);
                Ok(())
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
                    ui::outln!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print_resync_entries(&report.entries);
                    for note in &report.notes {
                        report::note(format_args!("note: {note}"));
                    }
                    for issue in &report.issues {
                        report::problem(format_args!("issue: {issue}"));
                    }
                    if report.entries.iter().any(|e| {
                        matches!(e.outcome, weft_engine::generate::Outcome::Rewritten { .. })
                    }) {
                        report::hint(
                            "review with `git diff patches/`; run `weft check` to re-verify \
                             the graph",
                        );
                    }
                }
                if report.issues.is_empty() {
                    Ok(())
                } else {
                    Err(ui::SilentExit(1).into())
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
                    None => prompt::edit_command(
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
                    print_resync_entries(&report.entries);
                    if !report.issues.is_empty() {
                        for issue in &report.issues {
                            report::problem(format_args!("issue: {issue}"));
                        }
                        return Err(ui::SilentExit(1).into());
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
            report::done("lock up to date");
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
                    ui::outln!("{path}, with every patch on:");
                    ui::out_str(text);
                }
                report::step(format_args!(
                    "dry run: would create patch `{}` and rewrite {}",
                    report.owner,
                    report
                        .rewritten
                        .iter()
                        .map(|n| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            } else {
                for line in &report.lines {
                    report::done(line);
                }
                report::hint("run `weft check` with answers that turn them on");
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
                report::info("nothing staged; `weft diff` shows the whole worktree");
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
                ui::outln!("{}", serde_json::to_string_pretty(&preview)?);
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
                    report::info("nothing to stage — the staged tree already matches the worktree");
                    return Ok(());
                }
                addpatch::stage_interactively(
                    &mut std::io::stdout(),
                    &addpatch::Paint::stdout(),
                    &mut addpatch::TerminalDecider::default(),
                    &mut staged,
                    &trees.work_tree,
                    &paths,
                )?
            } else {
                weft_engine::stage::add(&mut staged, &trees.work_tree, &trees.base_tree, &globs)
            };
            weft_engine::stage::write(&here.template, &here.session, &staged, &trees.base_tree)?;
            report::done(format_args!("staged {n} path(s)"));
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
            report::done("unstaged");
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
            let heading = format!(
                "session `{}` on `{}`",
                here.session, tpl.manifest.template.name
            );
            ui::outln!(
                "{}",
                ui::highlight(Out::Stdout, Some(palette::bold()), &heading)
            );
            ui::outln!("  worktree: {}", here.worktree);
            let base_names: Vec<_> = sess
                .session
                .base
                .iter()
                .filter_map(|id| tpl.id_to_name.get(id).cloned())
                .collect();
            ui::outln!(
                "  base: {}",
                if base_names.is_empty() {
                    "(empty template)".to_owned()
                } else {
                    base_names.join(", ")
                }
            );
            if !sess.session.scope.is_empty() {
                ui::outln!("  scope: {}", sess.session.scope.join(", "));
            }
            for (id, value) in sess.answers.iter() {
                ui::outln!("  answer: {} = {}", id.0, value.render_text());
            }
            for id in sess.secrets.keys() {
                ui::outln!("  answer: {} = (secret)", id.0);
            }
            ui::outln!();
            if staged_changes.is_empty() {
                ui::outln!(
                    "{}",
                    ui::highlight(
                        Out::Stdout,
                        None,
                        "no staged changes (a plain `weft commit` commits the whole worktree)"
                    )
                );
            } else {
                ui::outln!(
                    "{}",
                    ui::paint(
                        Out::Stdout,
                        palette::bold(),
                        "staged (the next commit takes these):"
                    )
                );
                for p in &staged_changes {
                    ui::outln!("  {}", ui::paint(Out::Stdout, palette::success(), p));
                }
            }
            if !unstaged_changes.is_empty() {
                ui::outln!("{}", ui::paint(Out::Stdout, palette::bold(), "unstaged:"));
                for p in &unstaged_changes {
                    ui::outln!("  {}", ui::paint(Out::Stdout, palette::error(), p));
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
                    Some(cmd) if cmd.is_empty() => Some(prompt::edit_command(
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
                ui::outln!("{worktree}");
                report::done(format_args!(
                    "session `{}` adopted `{worktree}` against `{template_root}` — \
                     `weft status` shows what it sees",
                    opts.name
                ));
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
                    ui::outln!("session `{}` looks at the whole worktree", here.session);
                } else {
                    ui::outln!("session `{}` looks at:", here.session);
                    for glob in &sess.session.scope {
                        ui::outln!("  {glob}");
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
                    report::info(format_args!(
                        "no sessions in `{template}`; start one with `weft session new`"
                    ));
                    return Ok(());
                }
                // Mark the session whose worktree we are standing in.
                let current = weft_engine::discover::cwd()
                    .ok()
                    .and_then(|c| weft_engine::discover::locate(&c))
                    .filter(|l| l.template_root == template)
                    .and_then(|l| l.session);
                if ui::Table::fits() {
                    let mut table = ui::Table::new(&["", "NAME", "WORKTREE", "KIND"]);
                    for (name, sess) in &sessions {
                        let mark = if current.as_deref() == Some(name.as_str()) {
                            "*"
                        } else {
                            ""
                        };
                        let kind = if sess.session.adopted { "adopted" } else { "" };
                        table.row([
                            ui::cell(mark).success(),
                            ui::cell(name.as_str()).accent(),
                            ui::cell(sess.worktree(&template, name).to_string()).dim(),
                            ui::cell(kind).dim(),
                        ]);
                    }
                    table.print();
                    return Ok(());
                }
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
                    ui::outln!("{mark} {name}\t{}{kind}", sess.worktree(&template, &name));
                }
                Ok(())
            }
            SessionCmd::Path { name, template } => {
                let here = ctx::Scope {
                    template,
                    session: name,
                }
                .resolve_or_only()?;
                ui::outln!("{}", here.worktree);
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
                ui::outln!("{dest}");
                report::done(format_args!("moved session `{}` to `{dest}`", here.session));
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
                report::done(format_args!(
                    "session refreshed: {} file(s) updated{}",
                    report.updated,
                    if report.conflicts.is_empty() {
                        String::new()
                    } else {
                        format!(", {} conflict(s)", report.conflicts.len())
                    }
                ));
                for c in &report.conflicts {
                    report::warning(format_args!("  {c} — resolve the conflict markers"));
                }
                for n in &report.notes {
                    report::note(format_args!("  note: {n}"));
                }
                if report.conflicts.is_empty() {
                    Ok(())
                } else {
                    Err(ui::SilentExit(1).into())
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
                    report::info("no session to end");
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
                report::done(format_args!(
                    "session `{}` ended{}",
                    here.session,
                    if adopted {
                        " (its worktree was left in place)"
                    } else {
                        ""
                    }
                ));
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
                if ui::Table::fits() {
                    let mut table = ui::Table::new(&["NAME", "FILE"]);
                    for preset in &template.manifest.presets {
                        table.row([
                            ui::cell(preset.name.as_str()).accent(),
                            ui::cell(preset.file.to_string()).dim(),
                        ]);
                    }
                    table.print();
                } else {
                    for preset in &template.manifest.presets {
                        ui::outln!("{}\t{}", preset.name, preset.file);
                    }
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
                    ui::outln!(
                        "{id} = {tv} {}",
                        ui::paint(Out::Stdout, palette::dim(), "(locked)")
                    );
                }
                for (id, (fixed, blocked)) in spec.constraints() {
                    let mut parts = Vec::new();
                    if !fixed.is_empty() {
                        parts.push(format!("fixed: {}", fixed.join(", ")));
                    }
                    if !blocked.is_empty() {
                        parts.push(format!("blocked: {}", blocked.join(", ")));
                    }
                    ui::outln!("{id}: {}", parts.join(" · "));
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
                ui::outln!("saved preset `{name}`");
                Ok(())
            }
            PresetsCommand::Rm {
                name,
                template: dir,
            } => {
                source::with_resolver(|r| weft_engine::preset::remove(&dir, &name, r))?;
                ui::outln!("removed preset `{name}`");
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
                    ui::outln!("{}", serde_json::to_string_pretty(&node_diff)?);
                } else {
                    report::info(format_args!(
                        "patch `{}` ({}) — {}",
                        node_diff.name,
                        node_diff.id.short(),
                        if node_diff.active {
                            "active"
                        } else {
                            "inactive under these answers"
                        }
                    ));
                    for file in &node_diff.files {
                        ui::outln!("{}", ui::unified_diff(Out::Stdout, &file.diff));
                    }
                }
                return Ok(());
            }

            let doc = weft_engine::graph::graph_doc(&template, resolved.as_ref(), &eval)?;
            if json {
                ui::outln!("{}", serde_json::to_string_pretty(&doc)?);
            } else {
                let heading = format!(
                    "template `{}` — {} node(s), {} edge(s)",
                    doc.template.name,
                    doc.nodes.len(),
                    doc.edges.len()
                );
                ui::outln!("{}", ui::highlight(Out::Stdout, None, &heading));
                for node in &doc.nodes {
                    let state = match node.active {
                        Some(true) => ui::paint(Out::Stdout, palette::success(), " [active]"),
                        Some(false) => ui::paint(Out::Stdout, palette::dim(), " [inactive]"),
                        None => ui::paint(Out::Stdout, console::Style::new(), ""),
                    };
                    let when = node
                        .when
                        .as_deref()
                        .map(|w| format!(" when={w}"))
                        .unwrap_or_default();
                    ui::outln!(
                        "• {} ({}){}{}",
                        ui::paint(Out::Stdout, palette::accent(), &node.name),
                        ui::paint(Out::Stdout, palette::dim(), node.id.short()),
                        when,
                        state
                    );
                    for op in &node.ops {
                        ui::outln!(
                            "    {} {}",
                            ui::paint(Out::Stdout, palette::dim(), &op.kind),
                            op.path
                        );
                    }
                }
            }
            Ok(())
        }
        Command::Schema { out } => {
            for path in schema::write_schemas(&out)? {
                report::done(format_args!("wrote {path}"));
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
                    ui::out_str(&md);
                } else {
                    let target = if path.is_empty() {
                        template.join("AGENTS.md")
                    } else {
                        Utf8PathBuf::from(path)
                    };
                    std::fs::write(&target, md)?;
                    report::done(format_args!("wrote {target}"));
                }
                return Ok(());
            }
            if json {
                ui::outln!("{}", serde_json::to_string_pretty(&doc)?);
            } else {
                ui::out_str(&weft_engine::describe::agents_md(&doc));
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
                ui::outln!(
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
                    Err(ui::SilentExit(1).into())
                }
            } else {
                weft_engine::check::finish(&name, &report)
            }
        }
    }
}

/// List the project's answers and ask whether to change any. On a yes, a
/// project with include instances is asked whose answers to go through (its
/// own, checked, and each instance's); `weft update` then asks every
/// question of those again, offering its current value. Secrets are left
/// out: they resolve from their source, never from an answer.
fn offer_answer_review(
    state: &weft_engine::state::State,
    interaction: &mut dyn weft_engine::interact::Interaction,
) -> anyhow::Result<weft_engine::update::Review> {
    use weft_engine::state::AnswerOrigin;
    use weft_engine::update::Review;
    let rows: Vec<_> = state
        .answer_rows()
        .into_iter()
        .filter(|row| row.origin != AnswerOrigin::Secret)
        .collect();
    if rows.is_empty() {
        return Ok(Review::default());
    }
    report::info("answers:");
    for line in answer_lines(&rows) {
        report::info(format_args!("  {line}"));
    }
    if !interaction.confirm("Change any of these answers?", false)? {
        return Ok(Review::default());
    }
    // The frames with answers to change, named as their answers are: the
    // root, `svc`, `connector.stripe` (a key other than its include's name is
    // a repeat instance, as `weft answers` reads it).
    let mut frames: Vec<(Option<(String, String)>, String)> = Vec::new();
    if rows.iter().any(|row| !row.id.contains('.')) {
        frames.push((None, "the project's own".to_owned()));
    }
    for inst in &state.instances {
        let label = if inst.key == inst.include {
            inst.include.clone()
        } else {
            format!("{}.{}", inst.include, inst.key)
        };
        let prefix = format!("{label}.");
        if rows.iter().any(|row| row.id.starts_with(&prefix)) {
            frames.push((Some((inst.include.clone(), inst.key.clone())), label));
        }
    }
    let picked = if frames.len() == 1 {
        vec![0]
    } else {
        let labels: Vec<&str> = frames.iter().map(|(_, label)| label.as_str()).collect();
        let mut checked = vec![false; frames.len()];
        checked[0] = true;
        interaction.pick(
            "Whose answers? (space toggles, enter confirms)",
            &labels,
            &checked,
        )?
    };
    let mut review = Review::default();
    for i in picked {
        match &frames[i].0 {
            None => review.root = true,
            Some(slot) => {
                review.instances.insert(slot.clone());
            }
        }
    }
    if !review.is_empty() {
        report::info(
            "Enter keeps each value; giving a question its template default makes it follow \
             the template again.",
        );
    }
    Ok(review)
}

/// Stored answers as aligned `id  value  origin` lines, values as you would
/// type them (a secret shows its source reference).
fn answer_lines(rows: &[weft_engine::state::AnswerRow]) -> Vec<String> {
    let width = rows.iter().map(|r| r.id.len()).max().unwrap_or(0);
    let shown: Vec<String> = rows.iter().map(answer_value).collect();
    let value_width = shown.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    rows.iter()
        .zip(&shown)
        .map(|(row, value)| {
            format!(
                "{:width$}  {value:value_width$}  {}",
                row.id,
                row.origin.label()
            )
        })
        .collect()
}

/// A stored answer's value as you would type it; a secret shows its source
/// reference.
fn answer_value(row: &weft_engine::state::AnswerRow) -> String {
    match (&row.value, &row.source) {
        (Some(v), _) => weft_engine::answers::show_value(v),
        (None, Some(src)) => src.clone(),
        (None, None) => String::new(),
    }
}

/// `weft answers`: a table on a terminal, the aligned lines otherwise.
fn print_answers(rows: &[weft_engine::state::AnswerRow]) {
    if ui::Table::fits() {
        if rows.is_empty() {
            return;
        }
        let mut table = ui::Table::new(&["ID", "VALUE", "ORIGIN"]);
        for row in rows {
            table.row([
                ui::cell(row.id.as_str()).accent(),
                ui::cell(answer_value(row)),
                ui::cell(row.origin.label()).dim(),
            ]);
        }
        table.print();
    } else {
        for line in answer_lines(rows) {
            ui::outln!("{line}");
        }
    }
}

/// `weft hook ls`: every hook, in execution order.
fn print_hooks(rows: &[weft_engine::author::HookRow]) {
    if rows.is_empty() {
        report::info("no hooks declared in this template");
        return;
    }
    if ui::Table::fits() {
        let mut table =
            ui::Table::new(&["PHASE", "EFFECT", "ID", "PATCH", "LABEL", "WHEN", "RUNS"]);
        for row in rows {
            table.row([
                ui::cell(row.phase),
                ui::cell(row.effect),
                ui::cell(row.id.as_str()).accent(),
                ui::cell(row.patch.as_str()).accent(),
                ui::cell(row.label.as_str()),
                ui::cell(row.when.as_deref().unwrap_or("")).dim(),
                ui::cell(row.runs.as_str()).dim(),
            ]);
        }
        table.print();
        return;
    }
    for row in rows {
        let gate = row
            .when
            .as_deref()
            .map(|w| format!(" when={w}"))
            .unwrap_or_default();
        ui::outln!(
            "{phase:4} {effect:6} {id:20} [{patch}] {label}{gate} (runs: {runs})",
            phase = row.phase,
            effect = row.effect,
            id = row.id,
            patch = row.patch,
            label = row.label,
            runs = row.runs,
        );
    }
}

/// `weft patch ls`: one row per root-frame patch, in dependency order.
fn print_patches(rows: &[weft_engine::author::PatchRow]) {
    if ui::Table::fits() {
        if rows.is_empty() {
            return;
        }
        let mut table = ui::Table::new(&["NAME", "OPS", "HOOKS", "FLAGS", "TITLE"]);
        for row in rows {
            table.row([
                ui::cell(row.name.as_str()).accent(),
                ui::cell(row.ops.to_string()).right(),
                ui::cell(row.hooks.to_string()).right(),
                ui::cell(row.flags.join(", ")).dim(),
                ui::cell(row.title.as_str()),
            ]);
        }
        table.print();
        return;
    }
    for row in rows {
        let flags = if row.flags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", row.flags.join(", "))
        };
        ui::outln!(
            "{name:20} {ops} op(s), {hooks} hook(s){flags}  {title}",
            name = row.name,
            ops = row.ops,
            hooks = row.hooks,
            title = row.title,
        );
    }
}

/// `weft instance list`: the project's include instances.
fn print_instances(rows: &[weft_engine::instance::InstanceRow]) {
    if rows.is_empty() {
        report::info("no include instances");
        return;
    }
    if ui::Table::fits() {
        let mut table = ui::Table::new(&["INCLUDE", "KEY", "MOUNT", "PINNED"]);
        for row in rows {
            table.row([
                ui::cell(row.include.as_str()).accent(),
                ui::cell(row.key.as_str()).accent(),
                ui::cell(row.mount.as_str()).dim(),
                ui::cell(row.pinned.to_string()).right(),
            ]);
        }
        table.print();
        return;
    }
    for row in rows {
        ui::outln!(
            "{}={} @ {} ({} patch(es) pinned)",
            row.include,
            row.key,
            row.mount,
            row.pinned
        );
    }
}

/// What `weft patch resync` (and `set-command --resync`) did to each patch.
fn print_resync_entries(entries: &[weft_engine::generate::ResyncEntry]) {
    use weft_engine::generate::Outcome;
    for entry in entries {
        let name = &entry.name;
        match &entry.outcome {
            Outcome::UpToDate => report::done(format_args!("{name}: up to date")),
            Outcome::Rewritten { ops } => {
                report::done(format_args!("{name}: rewritten ({ops} op(s))"))
            }
            Outcome::WouldRewrite { ops } => {
                report::step(format_args!("{name}: would rewrite ({ops} op(s))"))
            }
            Outcome::Skipped { reason } => {
                report::warning(format_args!("{name}: skipped — {reason}"))
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
    ui::outln!("saved preset `{name}` in {template_dir}");
    Ok(())
}
