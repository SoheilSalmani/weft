---
name: project-instructions
description: Curates the persistent project context that coding agents load on every task, normally AGENTS.md. Decides what earns permanent context, routes the rest to skills, tooling or human docs, compresses bloated instruction files, classifies an existing CLAUDE.md or vendor rules file, and audits instructions for staleness and duplication. Use when asked what belongs in AGENTS.md, to create or shrink an instruction file, to migrate CLAUDE.md, or to check whether agent instructions are still true.
---

# Project instructions

Persistent project context is the information an agent should hold during most work in this repository. It is loaded on every task, so **every line is paid for on every task, whether or not it is relevant.**

That single fact drives all the judgement here. A skill costs nothing until it triggers. An instruction costs something always. So the bar for permanent context is not "is this true" or even "is this useful", it is **useful often enough to outrank its permanent cost.**

## The routing question

Before adding anything to an instruction file, work down this list. Stop at the first match.

| Test | Home |
| --- | --- |
| Correctness must not depend on the agent remembering | **Tooling**: hook, CI, linter, formatter, permissions |
| Useful on a large proportion of tasks here | **Persistent instructions** |
| A recognisable task class needing a procedure or expertise | **An Agent Skill** |
| Audience is human | **README** or **CONTRIBUTING** |
| Temporary, one-off, or current status | **Nothing durable** |

The order matters. Deterministic enforcement comes first because prose that duplicates a formatter is both redundant and weaker than the formatter. "Format with Prettier" is not an instruction, it is a `format` script; the only part worth writing down is the non-obvious operational bit, such as which command runs it or which paths it skips.

Read `references/budget.md` for the frequency-versus-cost heuristic and worked borderline cases.

## What earns a place

- **Authoritative commands.** Install, validate, test, typecheck, build. Not every script that exists, only the ones an agent needs to run correctly and would otherwise guess wrong.
- **Source-of-truth rules.** Generated directories that must not be hand-edited, schema sources, vendored trees. These are the highest-value instructions in most repositories, because the failure is silent: an agent edits the output, the edit is correct-looking, and the next regeneration erases it.
- **Universal architectural invariants**, compressed to one sentence plus a pointer. `Only the billing service writes billing-owned state. See ADR-012.` The rationale stays in the ADR.
- **Non-obvious safety constraints.** Commands that cost money, destroy data, or reach production.
- **Conventions that materially affect many changes** and are not already enforced and not obvious from surrounding code.
- **Repository topology**, only the parts needed to navigate. Never a directory dump; an agent can list files more cheaply than it can read a stale tree.

## What does not

Route these out, and be willing to say plainly that something does not belong:

Full procedures for pull requests, commits, ADRs, issue triage, releases, migrations, debugging. Anything with a lifecycle. Long rationale, which belongs in an ADR. Roadmaps, sprint state, "we are currently migrating X": durable files are the wrong place for transient facts, and a stale status line is worse than none. Dependency inventories and exhaustive structure, both cheaply discoverable and constantly wrong. Rules a formatter or linter already enforces. Framework documentation. Content copied from README or CONTRIBUTING. Vendor-specific tricks, which belong in that vendor's own adapter file. Secrets, tokens, and machine-local paths, ever.

## Promoting an ADR

A decision earns a compressed line in persistent context only when all of these hold:

1. It constrains a broad share of future changes, not one subsystem touched rarely.
2. An agent could plausibly violate it **without** knowing to go and read the ADR.
3. The compressed form is stable, so it will not need editing every time the design moves.
4. One sentence genuinely captures the constraint. If it needs a paragraph, the ADR link is the instruction.

Most ADRs fail test 1 or 2. Promoting all of them is how instruction files reach 1,500 lines.

## Scoped and nested instructions

A nested instruction file is justified when a subtree has rules that would be **wrong** elsewhere, not merely irrelevant. Two packages using different package managers justify it. One package having a preference does not.

Inheritance: the root file stays authoritative for the whole repository; a nested file adds to it and overrides only where it conflicts. Do not restate the root file in a nested one; duplication is how the two drift.

Portability is the constraint that decides the mechanism. Many hosts read `AGENTS.md` natively. Claude Code does not read it at all, and expresses scoping through `.claude/rules/*.md` with a `paths:` glob instead. So a scoped rule may need two representations, and `agent-portability` owns that translation.

**Never widen a scoped rule to global just to claim portability.** Making "frontend uses pnpm" repository-wide on a host that lacks scoping is not partial support, it is a false instruction. Report the loss instead.

## Auditing

Two audits, both worth running before trusting an instruction file.

**Staleness.** Persistent instructions are dangerous when wrong, because agents trust them repeatedly without re-checking. Verify what is mechanically checkable: commands still exist in the manifest, referenced paths still exist, the package manager matches the lockfile, links resolve, referenced ADRs are still current. Flag high-risk prose that cannot be verified rather than asserting it is fine.

**Duplication.** Look for the same *authoritative* instruction in more than one editable place: the canonical file, a vendor file, a skill, CONTRIBUTING. Textual overlap is not automatically wrong; two editable copies of a rule are, because they drift and the agent then gets a contradiction. Keep one source and point at it.

If the repository has a checker for its agent configuration, run it. Find it where the repository names its commands, and never guess a script name. Without one, check by hand: the canonical file exists, every adapter's import or symlink resolves, no rule sits in both the canonical file and an adapter, each file is within its host's published size guidance, and every command and path the files name still exists.

## Writing and compressing

Instructions are context, not enforcement, and specific beats vague: `Run pnpm typecheck before claiming a build passes` outperforms `verify your work`. Group with headers. Never generate filler such as "write clean code", "follow best practices", or "add tests". It consumes budget and changes nothing.

For each existing line ask: **would removing this make a material number of ordinary tasks worse or riskier?** If not, cut it or route it. Then ask: **could the agent discover this cheaply and reliably when needed?** If yes, it probably does not need to be permanent. But keep non-obvious, high-consequence facts even when technically discoverable, because expensive archaeology is not cheap discovery.

Do not enforce a fixed line count. Optimise for relevance and density, and treat a vendor's own published guidance as the constraint where one exists. Claude Code documents a 200-line target per file.

## Accuracy

Never state a convention you have not verified against the repository. Not the package manager, not the test command, not a generated path, not an architectural owner. Read `package.json`, the lockfile, the config, the directory. If a command is not wired up anywhere, say so rather than inventing a plausible one. See `references/classification.md` for migrating an existing instruction file section by section.

## Before you finish

- Every line was tested against frequency, not just truth.
- Deterministic rules were routed to tooling rather than written as prose.
- Procedures went to skills; human content went to README or CONTRIBUTING.
- No transient status, no secrets, no machine-local paths.
- Every factual claim was checked against the repository.
- Scoped rules stayed scoped, and losses were reported rather than widened.
- One editable copy per instruction.
