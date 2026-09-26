---
name: agent-portability
description: Makes a repository's skills and persistent instructions work across coding-agent hosts from one canonical source. Audits agent configuration, migrates single-host setups, wires instruction adapters through import, config, symlink or generation, and reports what each target cannot represent. Use when asked to support several agents, migrate a .claude or .cursor setup, make AGENTS.md work on another host, sync agent config, or check cross-agent compatibility.
---

# Agent portability

One semantic source of truth, many compatibility projections.

There are **two canonical channels, and they are separate capabilities**:

```
persistent context   AGENTS.md and any justified scoped files
on-demand procedure  .agents/skills/
```

Deterministic behaviour is a third thing entirely — hooks, CI, linters, permissions — and is never made portable by writing it as prose in either channel.

Do not infer one channel from the other. A host reading `.agents/skills` tells you nothing about whether it reads `AGENTS.md`.

**Targets are the hosts this repository's contributors actually use.** Establish them from the repository and the user, never from the list of hosts that exist, and verify each one's behaviour against its own documentation before creating anything for it. `references/strategies.md` keeps dated notes on hosts checked so far; re-verify a note before relying on it.

Adding a target has a real cost. The target list is a decision, not an oversight.

Curating what belongs in the persistent channel is the `project-instructions` skill's job. This skill moves it between hosts without changing its meaning.

## Before doing anything

Establish the current state first: take the inventory in `references/migration.md`, and run the repository's own agent-configuration checker if it has one. Find a checker where the repository names its commands; never guess a script name.

**Do not add a target because it exists.** Every target has maintenance cost. A repository with one contributor and one agent may correctly want none of this.

Never assume the host you are running inside is the only target, and never add a projection for it just because it happens to be executing the migration.

## Strategies

| Class | When | What is created |
| --- | --- | --- |
| **Native** | The host reads `.agents/skills` directly | **Nothing.** More files is not more compatibility |
| **Symlink** | Same content, different discovery path, and the host is documented to follow symlinks | A relative symlink per skill directory |
| **Generated adapter** | Semantics need transformation | A generated, marked, reproducible file |
| **Partial** | Only some behaviour is representable | The representable part, plus an enumerated loss list |
| **Unsupported** | No safe equivalent | Nothing, and a plain statement |

For the persistent channel the ladder is different, because instructions are one file rather than a directory tree. Prefer the highest rung that works:

| Class | When | What is created |
| --- | --- | --- |
| **Native** | The host reads `AGENTS.md` already | **Nothing** |
| **Config** | The host can be told which filename or directory to read | One settings key |
| **Import** | The host reads its own file but supports includes | A vendor file whose entire body is an import, plus host-only additions |
| **Symlink** | Identical semantics, no host-only additions needed, symlinks reliable | A relative symlink |
| **Generated** | Translation genuinely required | A marked, reproducible file naming its source |
| **Partial** | Scope or precedence cannot survive | The representable part, plus the losses |
| **Unsupported** | No persistent mechanism exists | Nothing, and a plain statement |

**Import outranks symlink** wherever a host reads more than one instruction filename. A symlinked `CLAUDE.md` *is* `AGENTS.md`, so a host that reads both filenames sees the same bytes twice. An import's text is inert to hosts that do not expand it, and it survives Windows, where symlinks need elevation.

**Scoped rules never widen.** Making a rule scoped to one package repository-wide, on a host with no path scoping, does not make it portable; it makes it false everywhere else. Report it as partial.

Read `references/strategies.md` before choosing. The rules that bind:

- **Relative symlinks only.** An absolute path encodes one developer's machine.
- Symlink per skill directory, not the whole tree, so discovery stays predictable.
- Claude Code is documented to follow a skill-directory symlink and to load a shared target only once, so the projection does not double-load.
- Where a host's symlink behaviour is unverified, **test it or use a different strategy**. Do not assume.
- Windows contributors and cloud checkouts can lose symlinks. Confirm before relying on them.

## Migration

Read `references/migration.md`. The sequence:

1. **Inventory** every agent artefact: skills, commands, subagents, hooks, settings, instruction files.
2. **Classify by semantics, not filename.** A hook is not a skill because it lives near one.
3. **Extract the portable core** into `.agents/skills/`.
4. **Extract always-on context** into `AGENTS.md`, keeping genuinely host-specific behaviour in its own file.
5. **Project back** so the original host keeps working unchanged.
6. **Adapt** where transformation is required, marking generated files.
7. **Validate.** Every projection resolves, the original host discovers every skill, and nothing is editable in two places.
8. **Report losses** per target.

**Never mechanically rename `.claude` to `.agents`.** They hold different things: one is a host configuration tree containing settings, hooks, and commands; the other is a canonical skill store. Only skills move.

**A migration that breaks the original host has failed**, however portable the result is.

## Loss reporting

Every migration ends with a per-target report. The classes above are the vocabulary, and the discipline is in the last three.

Never write "compatible" when invocation control, tool restrictions, or execution semantics were dropped. A skill that relies on `disable-model-invocation` becomes model-invocable on a host that ignores the field, which for an audit or deploy skill is a behaviour change, not a cosmetic one. That is **partial**, and the report must say what changed.

For a host with no on-demand skill model, do not concatenate every skill into permanent context to claim support. Map stable always-on guidance to that host's instruction file, leave the procedures unavailable, and say so.

## Accuracy

Verify a host's behaviour against its own documentation before targeting it, and record the date. Do not claim support because a comparison article said so.

## Mutation safety

| Act | Default |
| --- | --- |
| Inventory, classify, report | Free |
| Run a read-only checker or a dry run | Free |
| Create symlinks or generated adapters | Ask |
| Move canonical skills | Ask, and check for open work touching those paths first |
| Modify a hand-written host config | **Never** without classifying it first |
| Delete anything not marked generated | Never |

## Before you finish

- Only the agreed targets received artefacts.
- Native targets received nothing.
- Every symlink is relative and resolves.
- No skill is editable in two places.
- Generated files carry a generated marker and a canonical pointer.
- The original host still works.
- Losses are enumerated per target, not summarised as "compatible".
- Nothing unverified was treated as supported.
