# Strategies and loss classes

## Contents

- Choosing a strategy
- Host notes
- Symlinks
- Generated adapters
- Hosts without an on-demand model
- Loss reporting
- Host extensions

## Choosing a strategy

Verify the host's behaviour from its own documentation, then:

```
Does the host read .agents/skills?
├─ yes → NATIVE. Create nothing
└─ no
   Does it read some other path with identical content?
   ├─ yes, and it is documented to follow symlinks → SYMLINK
   ├─ yes, symlink behaviour unknown → test it, or GENERATED ADAPTER
   └─ no
      Can the semantics be transformed into something it does read?
      ├─ fully → GENERATED ADAPTER
      ├─ partly → PARTIAL, with the losses enumerated
      └─ no → UNSUPPORTED. Say so
```

## Host notes

Observed behaviour of hosts checked so far. These are starting points, not target recommendations, and each row is only as current as its date. Re-verify from the host's own documentation before acting on a row, and update the date when you do.

| Host | `AGENTS.md` | Skills | Verified |
| --- | --- | --- | --- |
| Claude Code | Not read. A `CLAUDE.md` whose body imports it | Reads `.claude/skills` only, so each skill is symlinked from `.agents/skills` | 2026-08-14 |
| OpenCode | Native | Reads `.agents/skills` natively, so nothing is generated | 2026-08-14 |
| Hermes Agent | Native, as one of several context files where the first match wins | Reads `.agents/skills` at the project root natively, once the repository is trusted with `hermes skills trust` | 2026-09-25 |

## Symlinks

Verified 2026-08-14: **Claude Code** follows a symlink placed at a skill entry and reads `SKILL.md` from the target, and when the same target is reachable from several locations it loads the skill once. For any other host, symlink behaviour is unverified and must be tested before it is relied on.

Rules:

- **Relative, always.** `.claude/skills/x -> ../../.agents/skills/x`. An absolute path breaks on every machine but one.
- **Per skill directory**, never the whole tree. Linking `.claude/skills -> .agents/skills` makes discovery depend on the host resolving a directory symlink rather than a skill entry, which is a different and less documented behaviour.
- **Check the failure mode**, which is silent: the path exists, the host simply loads nothing.
- **Windows and cloud checkouts.** Symlink creation on Windows depends on developer mode or elevated permissions, and Git's `core.symlinks` setting decides whether a clone materialises them. A cloud agent's checkout behaviour is usually undocumented. If either is in scope, prefer a generated adapter.

## Generated adapters

An adapter is required when the target needs transformed content rather than identical content.

Every generated file must:

- Carry a header marking it generated and naming the canonical source.
- Be reproducible from the canonical source alone.
- Be checked for drift, so a hand edit is detected rather than silently overwritten.
- Never be hand-edited.

Before writing over any existing host config, classify it. A hand-written file is not an adapter, and overwriting it destroys work that has no other copy.

## Hosts without an on-demand model

Some hosts have persistent instruction files and no skill mechanism at all. Aider, Devin, Windsurf, and Kilo Code are documented AGENTS.md supporters and are not on the Agent Skills client registry.

The temptation is to concatenate every skill into the instruction file to claim support. **Do not.** It inflates permanent context with procedures that are relevant on a small fraction of tasks, which is the exact problem progressive disclosure exists to solve.

The honest adapter maps only stable, always-relevant guidance into the host's instruction file, and reports that on-demand procedures are unavailable there.

## Loss reporting

| Class | Means | Report says |
| --- | --- | --- |
| Native | Canonical files read directly | Nothing needed |
| Alias | Identical content, different path | Which path, which mechanism |
| Generated adapter | Transformed reproducibly | What transformation, and anything dropped |
| **Partial** | Some behaviour representable | **Enumerate exactly what is lost** |
| **Unsupported** | No safe equivalent | Say so plainly |

Losses that are easy to miss and matter most: invocation control, so a user-invoked-only skill becomes model-invocable; tool restrictions, so a skill that declared a narrow toolset no longer does; and path scoping, so a skill intended for one directory applies everywhere.

## Host extensions

Extensions stay in host configuration and never in a canonical skill.

| Extension | Host | If lost |
| --- | --- | --- |
| `disable-model-invocation` | Claude Code | Skill becomes model-invocable. **Behavioural, not cosmetic** |
| `user-invocable` | Claude Code | Skill becomes user-visible |
| `allowed-tools`, `disallowed-tools` | Claude Code, and spec-experimental | Tool restriction not applied |
| `context: fork` | Claude Code | Runs in the main context instead of a subagent |
| `argument-hint`, `arguments` | Claude Code | Autocomplete and substitution unavailable |

The canonical skill must work correctly with all of them absent. If it does not, semantics leaked into an extension and belong back in the portable core.
