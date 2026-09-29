# Migrating a single-host repository

## Contents

- Inventory
- Classification
- Order of operations
- Preserving the original host
- The no-op case

## Inventory

List everything before deciding anything:

```bash
find .claude .agents skills -maxdepth 2 2>/dev/null
ls AGENTS.md CLAUDE.md GEMINI.md 2>/dev/null
find . -maxdepth 4 -name SKILL.md -not -path "./node_modules/*"
find . -type l -not -path "./node_modules/*" -not -path "./.git/*"
```

Also check for open work touching those paths. Moving a directory that an open pull request modifies converts a clean merge into a conflict, and that is a reason to sequence the migration, not to skip the check.

## Classification

By semantics, never by location.

| Artefact | Usually becomes |
| --- | --- |
| A skill whose frontmatter fits the six spec fields | Canonical skill in `.agents/skills/` |
| A skill relying on host extensions | Canonical core, plus the extension left in host config, plus a noted portability limit |
| Universal conventions in `CLAUDE.md` | `AGENTS.md` |
| A long procedure embedded in `CLAUDE.md` | A skill |
| Genuinely host-specific behaviour in `CLAUDE.md` | Stays in `CLAUDE.md` |
| A command implementing a reusable workflow | A skill, if portable. `argument-hint` and `arguments` do not travel |
| A thin command alias | Stays host-specific, or becomes a script |
| A subagent | Host-specific. No portable equivalent |
| A hook | **Host-specific.** No cross-host mechanism exists |
| Settings and permissions | **Never translated.** Permission semantics differ per host and an approximation is dangerous |

The two that people convert wrongly: hooks, because they look like automation that should be portable, and settings, because they look like configuration that should map. Neither has a cross-host equivalent with the same guarantees.

## Order of operations

1. Inventory, and check for open work on those paths.
2. Classify every artefact and write the plan down.
3. Create `.agents/skills/` and move canonical skills with `git mv`, preserving history.
4. Write or update `AGENTS.md` from extracted universal context.
5. Create projections for the agreed targets only.
6. Validate: every projection resolves and nothing is editable in two places. Use the repository's own checker if it has one.
7. Confirm the original host still discovers every skill.
8. Write the loss report.

Move rather than copy. Two editable copies is the failure the whole architecture exists to prevent, and it appears immediately if step 3 copies instead of moving.

## Preserving the original host

The migration is not finished when the canonical tree exists. It is finished when the original host works exactly as before.

For Claude Code that means every skill still resolves through `.claude/skills/<name>`, `/skill-name` invocation still works, and user-invoked-only skills are still user-invoked-only, which depends on the extension field surviving in the canonical file or in host config.

If any of that regresses, the migration has traded working tooling for a tidier layout.

## The no-op case

A repository already using `.agents/skills` with correct projections needs **nothing**. The correct output is a statement that it is already canonical, plus the evidence: every projection resolves, and nothing is editable in two places.

Equally, a repository with one contributor and one host may correctly want no portability at all. Migrating it adds a directory layer and a set of symlinks or adapters to maintain, in exchange for compatibility nobody is consuming. **Say that plainly rather than performing the migration because it was requested in the abstract.**
