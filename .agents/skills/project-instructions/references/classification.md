# Classifying an existing instruction file

Applies equally to `CLAUDE.md`, `GEMINI.md`, `.cursor/rules/*.mdc`, `.clinerules`, `.windsurfrules`, `.github/copilot-instructions.md`, and any other vendor instruction file. **Claude Code is not a special case**; it is just the most common starting point.

## The two forbidden shortcuts

```bash
cp CLAUDE.md AGENTS.md      # copies procedures, status and vendor tricks into permanent context
ln -s AGENTS.md CLAUDE.md   # same content, now with two names and no classification
```

Both preserve bytes and discard meaning. A large vendor file is almost never all persistent context: it accumulated because there was nowhere else to put things. Classify first, then move.

The symlink is a legitimate *adapter* once the content is genuinely canonical and the host has no vendor-specific additions. It is never a migration step.

## Section by section

Read the file and assign every section exactly one destination.

| Destination | What it looks like |
| --- | --- |
| **Canonical instructions** | Universal, frequently relevant, short. `Always use pnpm.` |
| **Agent Skill** | A procedure with steps, or expertise for a recognisable task. `When creating a PR, follow these 80 lines...` |
| **Vendor-specific** | Only meaningful on that host. `Use subagent X.` `Use plan mode for src/billing.` Stays in that host's own file |
| **Human documentation** | Onboarding, project pitch, contribution workflow. `Project overview for new users...` → README or CONTRIBUTING |
| **Deterministic tooling** | Anything correctness depends on. `Never commit secrets` → a scanner or pre-commit hook, with at most a pointer left in prose |
| **Obsolete** | Duplicated, contradicted, or describing something that no longer exists. Delete, and say what you deleted |

Ambiguity is common. A section that is one universal sentence followed by forty lines of procedure splits: the sentence is promoted, the procedure becomes a skill.

## Order of operations

1. **Inventory** every instruction file in the repository, including nested and vendor directories.
2. **Classify** each section. Do not move anything yet.
3. **Present the classification** as a dry run: what moves to canonical, what becomes a skill, what stays vendor-specific, what is deleted as duplicate, what cannot be represented.
4. **Get approval**, then move.
5. **Wire the adapter** so the original host still loads the canonical content: import, config, or symlink.
6. **Verify the original host still works.** A migration that breaks the host it started on has failed.
7. **Only then** remove the old content, and only content whose semantics you preserved somewhere.

## Preserving vendor-specific meaning

Never delete a vendor file wholesale. After extraction it usually still has a job: it becomes the adapter, holding the import plus whatever is genuinely host-only.

```markdown
@AGENTS.md

## Claude Code

Use plan mode for changes under `src/billing/`.
```

The import mechanism is host-specific and must be verified from the host's own documentation before use. Claude Code documents `@path` with a four-hop limit and paths resolved relative to the importing file. Hosts that read `AGENTS.md` natively need nothing.

## Avoiding a double load

After wiring an adapter, check that no host sees the same instruction twice. The failure modes:

- A vendor file that **copies** canonical content instead of importing it. Two editable copies, guaranteed drift.
- A symlinked vendor file on a host that reads **both** filenames. If a host accepts a root `CLAUDE.md` as an alternative to `AGENTS.md`, then `CLAUDE.md -> AGENTS.md` is read as two copies of the same bytes. Prefer an import, whose literal text is inert to hosts that do not expand it.
- A `context.fileName` array listing both `AGENTS.md` and a vendor file that still exists.

A compatibility strategy that makes every instruction appear twice is incorrect even when each file is individually valid.

## Generated adapters

Prefer, in order: native reading, configuration, import, symlink, generated file, partial, unsupported. Generate only when nothing above works.

A generated adapter must name its canonical source in a comment where the format allows, must never be hand-edited, must regenerate deterministically, and must be detectable as drifted. Do not generate a file merely because a platform has its own branded instruction filename. An unnecessary adapter is a second copy waiting to diverge.
