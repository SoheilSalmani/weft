# How agent harnesses run a CLI

Read when designing behaviour an agent will meet, or when working out why an agent hangs on, misreads or misuses a command. Checked 2026-09-27 against each harness's documentation or source. These facts change release to release, so recheck any one before a design depends on it.

## Contents

- The harnesses
- Detecting an agent
- Incidents worth designing against
- Where the sources disagree

## The harnesses

| | Claude Code | OpenAI Codex CLI | Gemini CLI |
| --- | --- | --- | --- |
| Checked against | docs at `code.claude.com` (tools reference, env vars), versions up to 2.1.283 | source at commit `89bf86d0`, `codex-rs/core/src/unified_exec` | source at commit `2fe7c2d3`, `shellExecutionService.ts` |
| Terminal | Not documented. A user report (anthropics/claude-code#37523) describes no PTY and stdin disconnected | Plain pipes unless the model asks for `tty: true` | A PTY through node-pty, `TERM=xterm-256color` |
| Environment it sets | `CLAUDECODE=1` | `NO_COLOR=1`, `TERM=dumb`, `PAGER=cat`, `GIT_PAGER=cat`, `GH_PAGER=cat`, `LANG=C.UTF-8`, `CODEX_CI=1` | `GEMINI_CLI=1`, `PAGER=cat`, `GIT_PAGER=cat`, `GIT_TERMINAL_PROMPT=0`, `GH_PROMPT_DISABLED=1` |
| Time limit | 120 s by default, up to 600 s; then the command moves to the background | returns after 10 s by default with a session to poll | not recorded |
| Output kept | about 30,000 characters inline on success, the rest in a file; about 10,000 characters of head and tail on failure | a budget of about 10,000 tokens | not recorded |

Consequences for a CLI:

- **A TTY check alone does not find an agent.** Gemini CLI, and Codex when asked, run inside a pseudo-terminal, so a prompt can appear with nobody to answer it. Non-interactive flags must be discoverable from `--help` and from errors.
- **stderr is not free.** Anthropic's reference bash tool returns stdout and stderr together, and a pseudo-terminal merges them, so progress bars, banners and warnings reach the model alongside the data.
- **The middle of a long failure is lost.** A short error with the fix on the last line survives truncation; a thousand-line log followed by the error may not.
- **Each call is a fresh process.** Exported variables and directory changes do not reliably carry over, so context arrives as flags and paths.
- Claude Code treats exit status 1 as a failure for everything except a list of search and comparison commands (`grep`, `rg`, `find`, `diff`, `test`, `git diff` and a few more), and shows less output when a command fails.

## Detecting an agent

No variable is standardised. The candidates, and who sets them:

| Variable | Set by |
| --- | --- |
| `CLAUDECODE=1` | Claude Code |
| `CODEX_CI=1` | Codex CLI |
| `GEMINI_CLI=1` | Gemini CLI |
| `AGENT=goose` | Goose (merged 2026-02-10) |
| `AI_AGENT` | promoted by Vercel's `detect-agent` |

Requests for a shared `AGENT` variable were closed by Claude Code (#24838) and Codex (#13416), and the agents.md proposal (#136) is still open. `CI=true` is not a proxy: CI means unattended with minimal output, which is not what an agent wants.

Vendors that detect agents use it to change defaults: Supabase picks JSON output when no format flag is given, Vercel turns on its non-interactive mode, and Bun hides passing tests. An explicit flag always wins over detection, and the first incident below shows what happens when detection also changes what counts as consent.

## Incidents worth designing against

- **A prompt default taken as consent.** Supabase CLI 2.109.1, in agent mode, took a prompt's default answer before reading the piped "n" and lowered a hosted project's auth security settings. The fix (supabase/cli PR #6670, 2026-09-18) skips the change unless `--yes` or `SUPABASE_YES` is given.
- **A banner on stdout.** Wrangler printed a notice to stdout in shells that were neither a TTY nor CI, which broke `--format json | jq` (cloudflare/workers-sdk#14036, closed 2026-06-05).
- **A secret echoed in a suggestion.** Vercel CLI repeated `--token` values verbatim in the follow-up commands it suggested (GHSA-pgf8-2hgj-grqg, CVE-2026-44479, 2026-04-30).

## Where the sources disagree

- **JSON by default for agents.** Arcjet and Justin Poehnelt want JSON whenever stdout is not a terminal; Supabase switches when it detects an agent; others keep text, since JSON costs tokens. Anthropic's tool-writing guidance says to choose by evaluation. The conservative choice is text by default and `--json` on request, with detection changing nothing that a script could depend on.
- **Suggesting corrections.** clig.dev suggests the likely command after a typo; Arcjet makes unknown input a hard failure with no guess. Both refuse to run the guess.
- **More or less output.** Bun shows agents only failures; others argue agents want full detail such as long tracebacks. A `--verbose` level the agent can raise serves both.
- **CLI or MCP server.** Benchmarks (Mario Zechner, 2025-08-15) found tool design mattered more than protocol. A good CLI with `--help`, `--json` and non-interactive flags is usable by any agent with a shell; an MCP server is a separate decision.

Sources: Anthropic, "Writing effective tools for agents" (2025-09-11); Justin Poehnelt, "You Need to Rewrite Your CLI for AI Agents" (2026-03-04); Arcjet, "Designing a CLI for AI agents" (2026-06-02); Mario Zechner, "MCP vs CLI" (2025-08-15); the harness documentation, source, issues and advisories named above.
