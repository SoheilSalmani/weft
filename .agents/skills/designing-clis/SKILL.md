---
name: designing-clis
description: Designs, builds and reviews command-line interfaces so they work for a person at a terminal, for scripts and CI, and for coding agents driving them through a shell. Covers command and flag grammar, help and version, stdout versus stderr, exit codes, colour and TTY detection, prompts and non-interactive use, confirmation and dry runs for destructive commands, JSON output as a contract, secrets, configuration and environment variables, signals and broken pipes, and changing a CLI that scripts already depend on. Use when creating a CLI or a subcommand, adding or renaming a flag, writing help text or error messages, adding --json, making a tool scriptable or agent-friendly, or reviewing a CLI's UX, with any parser such as clap, Click, Typer, argparse, Cobra or commander. Not for running or scripting an existing CLI, HTTP or library API design, or the visual design of a full-screen TUI.
---

# Designing CLIs

A command-line tool has three kinds of user: a person at a terminal, a script or CI job, and a coding agent driving it through a shell tool. The person needs clarity, the script needs stability, and the agent needs both plus a way past every prompt. Design for the person, guarantee for the script, and never trap the agent.

**Everything a caller can observe is interface**: command and flag names, argument order, defaults, what goes to stdout and what to stderr, exit codes, JSON fields, environment variables, config keys and prompts. Once something scripts it, it is an API.

## Before you design or change anything

- **Read the interface as it is.** Run `--help` at every level, read the parser definitions, and learn what the parser library already does for free.
- **Find the callers.** Search the repository, CI, docs, tests and any agent instructions (`AGENTS.md`, skills) for invocations. Whatever they parse or branch on is a contract.
- **Keep the local conventions.** An existing CLI's grammar, flag style, output shape and exit codes beat this skill's defaults, because consistency inside one tool matters more than agreement with a guide. Where this skill would change something callers see, say what it recommends, what the tool does, and what the change would break, then let the user decide.
- **Size the job.** A script one person runs needs the essentials below and nothing more. Config files, completions, man pages, update checks and telemetry are costs, added only when a user needs them.
- **Know which skill owns which page.** Help text and error messages live in the binary and follow this skill. Pages about the tool, such as a README section or a reference page, follow `writing-repository-readmes` or `writing-documentation`, which load alongside this one; keep the two consistent.

## When rules collide

1. **A caller's safety beats consistency.** A hang, a silently wrong result, lost data or a leaked secret gets fixed even when the tool's own convention causes it. Where the fix changes what callers see, say so and let the user choose how.
2. **Compatibility beats polish.** A better name or a tidier grammar never justifies breaking a script: add an alias and keep the old path working.
3. **Stable machine output beats convenient human output.** Human text may change freely; JSON fields, exit codes and which stream carries what change only by addition.
4. **The tool's own conventions beat this skill's style defaults**, such as flag spellings, grammar and output layout.

## The essentials

Every CLI, however small:

1. **Parse arguments with a library**, never by hand. An unknown flag or command is an error that exits 2, which clap, argparse and Click already do.
2. **`-h` and `--help` work on every command and subcommand**, print to stdout and exit 0. `--version` prints the version to stdout and exits 0.
3. **Data goes to stdout; everything else goes to stderr**: progress, warnings, errors, prompts and interactive screens. `x=$(tool …)` must capture the result and nothing else.
4. **Exit 0 on success and only on success.**
5. **No prompt, colour, spinner or pager unless the stream it uses is a terminal**, and every prompt can be answered by a flag instead.
6. **Errors say what failed and what to do next**, in a line or two, with no stack trace unless asked for.
7. **A secret is never a flag value.** It leaks into `ps` and shell history.

## Commands, arguments and flags

- **One grammar everywhere.** A tool that acts on several kinds of object uses `tool <noun> <verb>` (`gh pr create`); a single-purpose tool takes its verb directly. Reuse the same verbs (`list`, `show`, `create`, `delete`) across nouns, and never ship near-synonyms such as `update` and `upgrade`.
- **Prefer flags to positional arguments.** One kind of positional, repeatable, is fine (`rm a b c`); two is suspect unless the order is obvious (`cp src dst`); three is wrong.
- Every flag has a long form. Short forms go only to flags people type constantly, and a letter means the same thing in every subcommand.
- Use a conventional name before inventing one: `--json`, `-n/--dry-run`, `-f/--force`, `-y/--yes`, `-q/--quiet`, `-o/--output`, `--no-input`, `--color`. `--force` overrides a safety check and `--yes` answers a confirmation; they are different flags. Read `references/conventions.md` when naming a flag, since some letters mean different things in different traditions.
- A boolean that defaults to on gets a `--no-` form. A flag either always takes a value or never does, because an optional value can swallow the argument after it; where "nothing" must be expressible, use a word such as `none`, never an empty string.
- A flag with a fixed set of values declares them to the parser, so `--help` lists them and a wrong value is a usage error rather than a failure halfway through.
- `-` means stdin or stdout wherever a file is expected, and `--` ends option parsing.
- **No catch-all default command and no implicit prefix abbreviations.** Both freeze the namespace: a later command cannot be added without breaking someone.
- A typo gets a suggestion ("did you mean `status`?") and a non-zero exit, never an automatic run.
- **An explicit argument that matches nothing is an error**, not a silent success that the next command then builds on.
- Names that become file paths are validated before use: reject path separators, `..` and control characters.

## Help text

- `--help` opens with a usage line and a one-sentence description, then examples of the commonest uses, then the flags, most used first, each saying what it does and its default. A flag with fixed values lists them.
- A command list gives each command one short line; the long explanation belongs to that command's own help. Text wraps to the terminal width.
- A command that needs an argument and gets none prints a short usage with one example and a pointer to `--help`, on stderr, and exits 2.
- **Examples show what a person types.** Flags for unattended runs (`--no-input`, `--yes`, `--json`) are listed with the others, and appear in an example only when the example is about automation, such as a CI job.
- Point to fuller documentation only where it exists. Never invent a URL.

## Output

- **Human first, machine on request.** The default is readable text. `--json`, or the tool's established `--format json`, gives a stable machine format on every command that returns data.
- **Presentation may adapt to a terminal; content may not.** Colour, alignment, truncation, headers, progress and paging can change when a stream is not a TTY. The records, the fields and the exit code cannot, and an explicit flag always beats detection.
- Plain text is one record per line, without borders, so `grep`, `cut` and `wc` work.
- **On success, say what changed.** The value a caller may want to capture, such as an ID, path or URL, goes to stdout on its own line; the narration around it goes to stderr, and `-q` silences the narration.
- **Bound open-ended output.** Listings, searches and logs that can grow without limit get a default limit or pagination, with `--limit` or `--all`; when output is cut short, say so and say how to get the rest. The record of what a mutation did is never truncated in machine output.
- **Colour** is off when the stream is not a terminal, when `NO_COLOR` is set to a non-empty value, when `TERM=dumb`, or with `--color=never`. `--color=always` turns it on. A flag or a config setting beats `NO_COLOR`. Check stdout and stderr separately, and never let colour alone carry meaning.
- **Animation** (spinners, progress bars, redrawn lines) only on a terminal. A long operation without one prints an occasional plain line to stderr, so a log or an agent can see it is alive.
- **A child process's output is not your output.** A hook, generator or compiler you spawn writes to your stderr unless its output is the result the user asked for; otherwise it corrupts your stdout.
- **Stop quietly when the reader goes away.** `tool | head -1` must not print a panic or a traceback. Several runtimes get this wrong by default; `references/conventions.md` has the handling for Rust, Python, Go and Node.

## Machine-readable output is a contract

- In `--json` mode stdout carries one JSON document, or one object per line for a stream, and nothing else: no banner, no update notice, no progress, no child process output.
- **Evolve it additively.** Adding a field is safe. Renaming, removing or retyping one is breaking and needs a new version, carried in the output or selected by a flag such as `--format-version`.
- Keep values machine-shaped: ISO 8601 timestamps with an offset, sizes in bytes, enumerations as stable strings, a stable order, never localised text as a value.
- **Failure is still a non-zero exit.** Decide where a machine-readable error goes, a JSON object on stderr or an envelope with an `ok` field on stdout, and document it. A stable error `code` beats matching message text.
- Human text is not a contract. Say so in the docs, and check that nothing parses it before changing it.

## Prompts and non-interactive use

- **Prompt only when stdin and the stream you draw on are both terminals.** Otherwise fail at once, exit 2, and name the flag that supplies the value. Never block on a prompt nobody can see.
- **Ask only for what is missing.** A value given as a flag is never asked for again, and a cheap precondition such as "the destination exists" is checked before the first question.
- **A TTY check is not an agent check.** Some agent harnesses run commands inside a pseudo-terminal, so the flags that skip prompts must be findable from `--help` and from the error printed when a value is missing.
- Offer `--no-input` to disable every prompt, and honour it from an environment variable for CI.
- **An unattended run never treats a prompt's default as consent.** A destructive or security-weakening change without `--yes` or an explicit flag is refused or skipped, never assumed.
- Confirm by severity: a mild change needs none; a moderate one prompts and offers `--dry-run`; a severe one makes the user type the resource's name, or pass `--confirm=<name>` in a script. A destructive confirmation defaults to no (`[y/N]`), and declining exits non-zero.
- A full-screen wizard or form is never the only path; everything it asks is also a flag.
- Ctrl-C always works, and passwords are never echoed.

## Errors and exit codes

- **0 success, 1 failure, 2 usage error.** Add specific codes only where a caller branches on them, such as "changes present" or "authentication required", document them, and keep them between 3 and 125: 126 and 127 belong to the shell, and above 128 means killed by a signal. `sysexits.h` codes are deprecated.
- **A message says what failed, why, and the next command to run**, with the actionable line last and the whole thing short, because agent harnesses cut long failed output in the middle.

  ```text
  Error: No such file or directory (os error 2)
  ```

  names neither the file nor the way out. This does both, with the fix on the last line:

  ```text
  notesync: cannot read /home/ana/notes/.notesync/config.toml: no such file
  Run 'notesync init' in /home/ana/notes, or pass --config PATH.
  ```

- Expected errors are rewritten in the user's terms. Unexpected ones say how to get detail (`--debug`, `-v`, an environment variable) and where to report the bug. Default output shows no stack trace or panic.
- **Validate all input before the first side effect.** Treat a value from an agent like a value from the network.
- A partial failure reports each item's outcome and exits non-zero.
- Redact secrets from everything printed: errors, debug logs and suggested commands.

## Configuration, environment and secrets

- Precedence, highest first: flags, environment variables, project config, user config, system config.
- Files go where the platform expects: the XDG base directories on Linux, and the platform directories on macOS and Windows, found through a library rather than hard-coded. `references/conventions.md` has the paths.
- The tool's own environment variables share one prefix (`TOOL_`). Honour the general ones: `NO_COLOR`, `PAGER`, `EDITOR` and `VISUAL`, `TMPDIR`, `HOME`, the proxy variables and `COLUMNS`.
- **Secrets come from a file (`--token-file`), stdin or a credential store.** clig.dev also rules out environment variables, which leak into logs and process inspection, yet CI systems and agent harnesses inject secrets exactly that way. Accept one where automation needs it, and never echo it.
- Ask before changing another program's configuration. Where that program reads a directory of files, add your own file instead.

## Robustness

- Print something within about 100 ms, and before any network request.
- Network calls have a default timeout and a flag to change it.
- Running a command again after an interruption continues or repeats safely. Two runs never mutate the same state at once; take a lock, and say who holds it.
- **On Ctrl-C or SIGTERM**, stop promptly and clean up under a timeout, and let a second Ctrl-C skip the cleanup. Then restore the default handler and re-raise the signal, so the calling shell sees an interrupt (status 130 for SIGINT) and stops its own loop; exiting 130 by hand is second best.
- Each invocation stands alone. Take paths and context as flags rather than relying on shell state from an earlier call.
- **Write only what was asked for.** A file the user did not name, other than the tool's own cache and state, is written only when a flag or argument asks for it. A file the tool did not create is overwritten only with `--force`. A scratch directory the tool creates inside a project carries a `.gitignore` containing `*`, as uv does for `.venv`.

## Changing a CLI people already use

- **Add rather than change.** A new flag, command or JSON field breaks nobody.
- Rename through an alias: the old name keeps working, hidden from help, and prints a deprecation warning on stderr naming the replacement for at least one release. Record the removal in the changelog.
- Never reuse a removed name for a different meaning.
- Changing a default, an exit code, the stream something goes to, or the shape of machine output is a breaking change, whatever the version number says.

## Agents as callers

An agent is a script that reads your help text and your errors as its documentation.

- Agent harnesses usually run commands without a terminal, show the model stderr as well as stdout, time out long commands, and truncate long output. What each harness does changes release to release; `references/agent-harnesses.md` records what was checked and when.
- So every capability is reachable without a prompt; `--help` carries examples; anything worth parsing has `--json`; output is bounded with a truncation notice; errors name the next command; and read-only commands (`status`, `show`, `list`) let an agent verify what a mutation did.
- A tool with a large surface can describe itself with a command that prints its commands, flags and output shapes as JSON.
- **Environment variables that detect an agent are not standardised.** Use one to change presentation at most, never to relax a safety check, and let an explicit flag win.

## Reviewing a CLI

1. **Inventory the surface**: the command tree, every flag, the output streams, the exit codes, the environment variables and the config files.
2. **Probe the binary** from a scratch directory. Run only read-only commands against real data, and mutating ones on a copy. These probes are for the reviewer; they never go into help text or pages written for people.

   ```sh
   tool --help; echo "exit=$?"            # stdout, exit 0
   tool sub --help                        # every level
   tool --version
   tool --no-such-flag; echo "exit=$?"    # stderr, exit 2, says what to do
   tool sub </dev/null; echo "exit=$?"    # no hang; names the flag it needed
   tool sub | cat                         # no escape codes, no spinner
   NO_COLOR=1 tool sub
   tool sub --json | jq -e . >/dev/null   # valid JSON and nothing else on stdout
   tool sub 2>/dev/null | head -1         # no panic when the reader leaves
   tool create ../outside                 # a name that becomes a path stays inside
   tool add no-such-thing; echo "exit=$?" # an argument that matches nothing fails
   ```

3. **Read the paths probes cannot reach**: TTY checks, where child processes write, where a user-supplied name becomes a file path, signal handling, how errors are printed, and what happens with no terminal at all.
4. **Report findings ranked by who breaks.** Scripts and agents first, because a hang or a silently wrong result costs the most; people second; polish last. Each finding gives its evidence (the command and what it printed, or `path:line`), the change, and what the change would break. Then say what already works, and list the checks you ran.

A CLI that meets the bar gets "no changes needed". Never invent findings to fill a report.

## Before you finish

- Every prompt has a flag, and without a terminal the command fails fast and names it.
- stdout carries only data, including in `--json` mode and while child processes run.
- 0 means success and only success; usage errors exit 2; any other code is documented and below 126.
- Colour and animation are off when piped, with a non-empty `NO_COLOR`, and with `TERM=dumb`.
- Every command answers `--help` with an example of what a person types, and no example meant for people carries `--no-input`, `--yes` or `--json` unless it is about automation.
- Every error names what failed and the next step, with the fix on its last line.
- No secret is accepted as a flag value or printed anywhere.
- Nothing callers depend on changed without an alias and a deprecation warning, or the user's decision.
- A review ran its probes against the real binary and names them.
- A CLI that already met the bar was left alone.

## Sources

Read 2026-09-27. Guidance about agents moves fastest; recheck it before relying on it.

- Command Line Interface Guidelines, `clig.dev` (content last changed 2026-01-26).
- POSIX.1-2024, Base Definitions chapter 12, Utility Conventions; GNU Coding Standards, Command-Line Interfaces.
- `no-color.org`; `force-color.org`; XDG Base Directory Specification 0.8.
- GitHub CLI `docs/primer` and command-development guide; Heroku CLI Style Guide; kubectl conventions and the Kubernetes deprecation policy; Microsoft System.CommandLine design guidance.
- Anthropic, "Writing effective tools for agents" (2025-09-11); Justin Poehnelt, "You Need to Rewrite Your CLI for AI Agents" (2026-03-04); supabase/cli PR #6670 (2026-09-18).
