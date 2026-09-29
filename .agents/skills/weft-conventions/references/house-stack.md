# The house stack

What the house has already decided, read from the templates that exist. Surveyed 2026-09-25 across `~/Desktop/Projects/templates/{base,java,scala,dbt,fastapi,airflow}` and `~/Desktop/Projects/trendlift/weft-templates/web`. **Look at the templates directory before trusting this file**: a newer template outranks it, and the survey is the reading, not the rule. The rule is in `SKILL.md`: a tool is a decision the template makes; a question is for what varies between projects.

## Contents

- Every repository: the `base` template
- Per language
- Pinned, not asked
- Not yet decided
- Defects in the existing templates

## Every repository: the `base` template

`base` is the root every repository starts from and the donor of the portable patches every stack template carries (see `patch-format.md`). Its patches, and the decision each one embodies:

| Patch | Decided | Gate |
| --- | --- | --- |
| `editorconfig` | UTF-8, LF, final newline, four spaces; two for YAML, JSON, TOML; trailing whitespace kept in Markdown | |
| `gitattributes` | `* text=auto eol=lf`; a stack adds its own rules as a hunk (java: `*.bat` CRLF, `*.jar` binary) | |
| `mise` | `mise.toml` pins `node = "24"` for skill scripts and MCP servers; a stack pins its runtime with a `mise-<tool>` patch that hunks `[tools]` (`mise-java` → `java = "25"`, `mise-uv` → `uv = "0.12"`, `mise-astro`); `mise.local.toml` is the gitignored personal layer; hooks `verify-mise` and `mise-install` (`mise trust -q && mise install -y`, `glob:mise.toml`) | |
| `paseo` | `paseo.json` for Paseo worktrees: `worktree.setup` copies the gitignored personal files (`.env`, `.env.*`, `mise.local.toml`, `.claude/settings.local.json`) from `$PASEO_SOURCE_CHECKOUT_PATH` without overwriting tracked ones, then runs `mise trust -q && mise install -y`; a stack appends its install steps with a `paseo-<tool>` patch that hunks the `setup` array (`paseo-uv` → `uv sync`, `paseo-dbt` → `uv sync`, `uv run dbt deps`). Paseo stops at the first failing step and marks the setup failed, so every step must succeed on a fresh checkout | |
| `instructions` | `AGENTS.md` is the one instruction file; `CLAUDE.md` is `@AGENTS.md`. Codex and oh-my-pi read `AGENTS.md` and `.agents/skills` natively | |
| `skills` | the 15 house Agent Skills under `.agents/skills/`, canonical, including `designing-clis`, ungated because any repository can grow a script with flags; `.claude/skills/` is a symlink farm rebuilt (and pruned) by the post hook `sync-claude-skills` with `glob:.agents/skills/**` | |
| `github` | 8 GitHub skills; they drive `gh`, no GitHub MCP server | `use_github` (True) |
| `linear`, `jira` | the tracker's skills | `use_linear` (True), `use_jira` (False) |
| `anki`, `obsidian` | personal skills; their MCP servers are per machine and stay in user-level config | `use_anki`, `use_obsidian` (False) |
| `meetings` | `preparing-client-meetings`, a personal skill for dailies and weeklies; it finds the Obsidian vault per machine and reads trackers through the `mcp` servers, so it adds no MCP entry of its own | `use_meetings` (False) |
| `mcp` | remote MCP servers as definition-only entries, identical in `.mcp.json` (Claude Code), `.codex/config.toml` (Codex, trusted projects) and `.omp/mcp.json` (oh-my-pi); `linear-server` at `https://mcp.linear.app/mcp`, `atlassian` at `https://mcp.atlassian.com/v2/mcp`; OAuth per user, nothing stored. The entries are hand-written `expr` lines on the two bools because one file cannot be shared by two sibling patches | `use_linear or use_jira` |
| `base` (not portable) | `README.md` with the `mise install` recipe and the `.gitignore` stanzas (OS, editors, `.env*`, `mise.local.toml`, `.claude/settings.local.json`); hooks `git-init` and `git-commit --after` every setup hook | |

Two rules follow from it:

- The trackers are **bools, not a choice**: choice values are abstracted wherever they appear and `linear` is all over the skill prose (`questions.md`).
- The portable copies are kept identical by `scripts/sync-portable.sh` and `scripts/check-portable-drift.sh` in the templates repository; CI runs the drift check and `weft check` on every template. Edit the donor in `base`, never a copy.

## Per language

| Stack | Decided | Asked with `use_*` | Evidence |
| --- | --- | --- | --- |
| Python | uv pinned by mise (`mise-uv`), Python pinned by `.python-version` (uv reads it natively), `pyproject.toml`; hooks `verify-uv`, `uv-sync` (`uv lock && uv sync`, `glob:pyproject.toml`); `paseo-uv` syncs each Paseo worktree | Docker | `fastapi`, `dbt`, `web` (apps/api) |
| Node | pnpm (`packageManager` pinned), Turborepo, Prettier | shadcn/ui, Prisma, Better Auth, Resend, Fumadocs, Mastra, PostHog, Stripe, Typesense, Cloudinary | `web` |
| Java | JDK 25 by mise (`mise-java`), Gradle with the Kotlin DSL and a committed wrapper, Spotless + palantir-java-format, Error Prone, JUnit + AssertJ | Spring Boot, Testcontainers, GitHub Actions, Renovate | `java` |
| Scala | JDK 25 by mise (`mise-java`), scala-cli entry point, scalafmt with a pinned version, `verify-scala` and `verify-scalafmt` pre checks | sbt, ZIO | `scala` |
| dbt | uv-managed Python, snake_case `package_name` feeding both `dbt_project.yml` and `pyproject.toml`, `profile_name` defaulting to it, `.vscode/extensions.json` recommending the dbt extension, a `dbt-skills` patch with the four dbt skills; hooks `uv-sync`, `dbt-deps` (`uv run dbt deps`, `glob:packages.yml`); `paseo-dbt` syncs and installs packages in each Paseo worktree | | `dbt` |
| Airflow | Astro CLI project, Astro CLI pinned by mise (`mise-astro`), `verify-astro` | | `airflow` |

## Pinned, not asked

- Language and tool versions: `node`, `java`, `uv`, `astro` in `mise.toml`; `.python-version`; `packageManager`; the Gradle wrapper version; `scalafmt`; Spring Boot. Bump with `weft patch amend`; `weft update` moves every project. `java_version` was dropped from `java` on 2026-09-25. `scala` still asks `scala_version`, a 2.13-versus-3 axis rather than a plain version; do not add version questions elsewhere.
- Lockfiles: never in a patch. `uv.lock`, `pnpm-lock.yaml`, `gradle.lockfile` are produced by a `setup` hook with `inputs` on the manifest. List them in `.weftignore`.
- Formatter and linter configuration: shipped, not offered.
- Editor and git configuration, AGENTS.md, the skills, the MCP file set, the Paseo worktree setup: shipped by the portable patches, never asked.

## Not yet decided

No template ships a LICENSE, pre-commit, ruff or mypy configuration, a justfile or Makefile, a devcontainer, or Renovate outside `java`. When a new template needs one of these, decide once, write it into the patch, and add the decision here. Do not turn the absence of a decision into a question.

## Defects in the existing templates

Steer away from these; they are not conventions:

- `java` is a chain `base → testing → formatter → linter → spring-boot` because every patch edits the same `plugins {}` and `dependencies {}` blocks of `build.gradle.kts` and each hunk anchors on the previous patch's lines. That is the legitimate shared-config case, not a smell; do not try to flatten it. The Gradle build, its wrapper and the entry point are one `base` patch: a Gradle project without its wrapper is not a state anyone wants.
- `weft hook ls` prints `git-commit` before the setup hooks it lists in `--after`; `weft describe --json` and the actual run honour the order. Display only.
- `web-template` and `python-service` do not match their directory names.
