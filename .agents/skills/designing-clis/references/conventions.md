# CLI conventions

Tables to consult while naming a flag, choosing an exit code, reading an environment variable, placing a file, or implementing terminal behaviour in a particular runtime. Read 2026-09-27; runtime behaviour marked "observed" was run that day on macOS arm64.

## Contents

- Flag names
- Grammar and output flags in well-known tools
- Exit codes
- Colour
- Environment variables
- Where files go
- Runtime pitfalls: Rust, Python, Go, Node
- Deprecation policies to copy

## Flag names

| Flag | Meaning | Notes |
| --- | --- | --- |
| `-h`, `--help` | help | Reserve `-h` for help alone. Output to stdout, exit 0 |
| `--version`, `-V` | version | First line `NAME VERSION` (GNU). Stdout, exit 0 |
| `-v`, `--verbose` | more detail | `-v` is contested: verbose in POSIX tradition, clap and argparse; version in some tools. Never both in one tool |
| `-q`, `--quiet` | less output | GNU pairs `--quiet` with `--silent` as synonyms |
| `-n`, `--dry-run` | show what would happen, change nothing | kubectl: every mutation should support it |
| `-f`, `--force` | override a safety check | `-f` also means `--file` in kubectl and older tools; pick one meaning per tool |
| `-y`, `--yes` | answer yes to confirmations | Not the same as `--force` |
| `--no-input` | never prompt; fail if something is missing | Also seen as `--non-interactive`; keep whichever the tool already has |
| `-i`, `--interactive` | opt in to prompts | Microsoft reserves it for this; kubectl uses `-i` to attach stdin |
| `-o`, `--output` | output file or directory | kubectl and the AWS CLI use `-o`/`--output` for the format instead; Microsoft and GNU reserve it for a path |
| `--json` | machine-readable output | Or `--format json` / `--output json` where the tool already has a format flag |
| `--color=auto\|always\|never` | colour control | `--no-color` as the short spelling |
| `-a`, `--all` | include everything | |
| `-d`, `--debug` | diagnostic detail | |
| `-C DIR` | run as if started in `DIR` | git, make, tar |
| `--` | end of options | POSIX Guideline 10 |
| `-` | stdin, or stdout where an output is expected | POSIX Guideline 13 |

POSIX Utility Syntax Guidelines worth knowing: short options are one character (3), may be grouped behind one `-` (5), should not take optional arguments (7), and their order should not matter (11). GNU `getopt` permits options after operands; POSIX does not. Prefer long options that mirror every short one.

## Grammar and output flags in well-known tools

| Tool | Grammar | Machine output | Stability promise |
| --- | --- | --- | --- |
| gh | noun verb (`gh pr create`) | `--json FIELDS`, `--jq`, `--template` | preserve flags, defaults, exit behaviour, JSON fields and streams unless a change explicitly authorises breaking them |
| docker | noun verb management commands, plus legacy verbs | `--format` (Go template or `json`) | none found for `--format json` |
| kubectl | verb noun (`kubectl get pods`) | `-o json\|yaml\|name\|jsonpath` | `--short` output is for reading, not parsing |
| Heroku | `topic:command` (`apps:create`) | `--json` | after GA, additions are fine and changes are not |
| Terraform | verb (`terraform plan`) | `-json`, one object per line, a versioned `ui` message first | JSON and exit codes are the supported interface; human text is not |
| git | verb, porcelain over plumbing | `--porcelain[=v2]`, `-z` | porcelain formats stable across versions and user config |
| cargo | verb | `--message-format=json`; `cargo metadata --format-version` | metadata is stable and versioned; new fields may appear |
| AWS CLI | service, then operation | `--output json\|yaml\|text\|table`, `--query` | `text` columns can shift; pair with `--query` |

Consumers of versioned formats are told to ignore unknown fields and reject an unsupported major version (Terraform, git porcelain v2, rustc JSON).

## Exit codes

| Code | Meaning | Source |
| --- | --- | --- |
| 0 | success | POSIX |
| 1 | general failure | convention |
| 2 | usage error | Bash builtins; clap, argparse and Click (observed) |
| 3–125 | tool-specific, documented | gh: 2 cancelled, 4 authentication required. kubectl: 3 with `--error-unchanged`. Terraform `plan -detailed-exitcode`: 2 means changes present, which collides with usage errors |
| 101 | Rust panic (observed) | a crash, never a designed outcome |
| 126 / 127 | found but not executable / not found | shell |
| 128+N | killed by signal N: 130 SIGINT, 141 SIGPIPE, 143 SIGTERM | Bash |
| 64–78 | `sysexits.h` | FreeBSD marks it deprecated and discourages its use |

POSIX advises callers to test for zero rather than rely on one particular non-zero value, so a code is only useful once it is documented. Diagnostics take the form `tool: message`, lowercase, no trailing period (GNU).

## Colour

Decide per stream, in this order:

1. `--color=always|never` or a config setting wins.
2. `NO_COLOR` present and non-empty disables colour, whatever its value. An empty value counts as unset. It does not forbid bold, underline or italic, and a tool that never colours by default can ignore it.
3. `FORCE_COLOR` (or `CLICOLOR_FORCE`) present and non-empty enables colour even when piped.
4. `TERM=dumb` disables colour.
5. Otherwise colour only if the stream is a terminal.

Sources disagree on step 2 against step 3: Python lets `NO_COLOR` beat `FORCE_COLOR`, while Node ignores `NO_COLOR` when `FORCE_COLOR` is set. Pick one and document it. gh also honours `CLICOLOR=0`.

## Environment variables

Honour these rather than inventing equivalents:

| Variable | Use |
| --- | --- |
| `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR_FORCE` | colour, as above |
| `TERM` | `dumb` means no colour and no cursor movement |
| `PAGER` | run through `sh -c`, only when stdout is a terminal; git reads `GIT_PAGER` first, gh `GH_PAGER` |
| `EDITOR`, `VISUAL` | git's order: `GIT_EDITOR`, `core.editor`, `VISUAL`, `EDITOR`, then `vi` |
| `COLUMNS`, `LINES` | override the terminal size when set |
| `TMPDIR` | temporary files instead of `/tmp` |
| `HOME`, `XDG_*` | file locations, below |
| `http_proxy`, `https_proxy`, `no_proxy` | no standard; lowercase is the most widely honoured, and curl ignores uppercase `HTTP_PROXY` |
| `CI` | `true` on GitHub Actions and GitLab; no cross-vendor specification |
| `DO_NOT_TRACK` | `1` opts out of telemetry; its spec site has lapsed, but gh still honours it |

A tool's own variables share one uppercase prefix. Mirror each flag that automation needs (`TOOL_NO_INPUT`, `TOOL_TOKEN`), and let the flag win over the variable.

## Where files go

| Kind | Linux (XDG) | macOS | Windows |
| --- | --- | --- | --- |
| config | `$XDG_CONFIG_HOME` or `~/.config/<tool>` | `~/Library/Application Support/<tool>`, though git and gh use `~/.config` | `%APPDATA%` |
| data | `$XDG_DATA_HOME` or `~/.local/share/<tool>` | `~/Library/Application Support/<tool>` | `%APPDATA%` |
| state and logs | `$XDG_STATE_HOME` or `~/.local/state/<tool>` | state with data; logs in `~/Library/Logs/<tool>` (`platformdirs`) | no convention; Rust's `dirs` returns none |
| cache | `$XDG_CACHE_HOME` or `~/.cache/<tool>` | `~/Library/Caches/<tool>` | `%LOCALAPPDATA%` |

An XDG variable that is unset, empty or relative falls back to the default. Create missing directories with mode 0700. Use a library (`dirs` in Rust, `platformdirs` in Python, `os.UserConfigDir` in Go) rather than hard-coding. macOS is contested for command-line tools: `~/.config` is what developers expect from git and gh, and the library default is `Application Support`. Choose one and document it.

Project config lives in the repository, found by walking up from the working directory. Precedence, highest first: flags, environment, project, user, system.

## Runtime pitfalls: Rust, Python, Go, Node

| | Terminal check | `tool \| head -1` by default | Fix |
| --- | --- | --- | --- |
| Rust | `std::io::IsTerminal` | `println!` panics with "failed printing to stdout: Broken pipe", exit 101 (observed) | On stable, write with `writeln!` and treat `ErrorKind::BrokenPipe` as a clean exit, or restore `SIGPIPE` to `SIG_DFL` at the top of `main` on Unix. `-Zon-broken-pipe=kill` is nightly only |
| Python | `sys.stdout.isatty()` | `BrokenPipeError` traceback, exit 1 (observed) | Catch `BrokenPipeError` around the entry point, `dup2` `os.devnull` onto stdout, exit 1. The docs say not to reset `SIGPIPE` to `SIG_DFL` |
| Go | `golang.org/x/term` `IsTerminal` | killed by `SIGPIPE`, silent, status 141 (observed) | Nothing needed for stdout |
| Node | `process.stdout.isTTY` | `console.log` swallows EPIPE, exit 0; `process.stdout.write` throws an unhandled EPIPE with a stack trace, exit 1 (both observed) | Listen for `'error'` on `process.stdout` and exit quietly on `EPIPE` |

Ctrl-C: Python's uncaught `KeyboardInterrupt` exits by the signal (status 130) but prints a traceback. Catch it at the entry point, clean up, then restore `SIG_DFL` and re-send `SIGINT` to yourself. Node's default handler exits with 128 plus the signal number, and adding a listener removes that default.

Parsers: clap, argparse and Click exit 2 on a usage error and 0 on `--help` (observed). Check any other parser before relying on it.

## Deprecation policies to copy

- **Kubernetes**: a deprecated user-facing flag or command keeps working for 12 months or 2 releases, whichever is longer, and warns when used.
- **Docker**: a deprecated feature stays for at least one stable release.
- **Terraform v1**: documented workflow commands and flags are not removed within v1; an accidental break is a bug.
- **git**: breaking changes wait for a major version and are listed ahead of time in `BreakingChanges.adoc`.

Sources: POSIX.1-2024 XBD chapters 8 and 12 and XCU 2.8.2; GNU Coding Standards 4.4, 4.8 and 4.10; Bash manual 3.7.5; FreeBSD sysexits(3); `no-color.org`; `force-color.org`; `bixense.com/clicolors`; XDG Base Directory 0.8; Python `signal` docs; Node `process` and `console` docs; the gh, Heroku, kubectl, Terraform, git, cargo and AWS CLI documentation named in `SKILL.md`.
