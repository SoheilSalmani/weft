# Staging without destroying work

Losing uncommitted work is unrecoverable. A messy commit is not. Every rule here follows from that asymmetry.

## Contents

- Establish what was already there
- Staging precisely
- Partial staging
- Commands never to run over user work
- In-progress operations
- When you cannot tell

## Establish what was already there

Before staging anything:

```bash
git status --porcelain          # tracked changes and untracked files
git diff --stat                 # shape of the unstaged work
git stash list                  # someone may have work parked here
```

Changes that predate this session belong to the user. If the conversation does not establish which are which, **ask**. Do not infer ownership from file type, directory, or apparent relatedness to the task: a config file you touched and a config file they touched look identical in `git status`.

Untracked files deserve particular care. An untracked file may be a scratch file, a local override, or something they have not finished. Never stage untracked files in bulk.

## Staging precisely

Stage named paths:

```bash
git add packages/db/src/schema.ts packages/db/migrations/0004_add_claim.sql
```

Never `git add .`, `git add -A`, or `git add -u` when the tree holds anything you did not create. Those three commands are the usual mechanism by which someone's half-finished work ends up in a commit with a message that does not mention it.

After staging, verify before committing:

```bash
git diff --cached --stat        # exactly what is going in
git status                      # what is deliberately left behind
```

Say in your response what you left unstaged and why. Silence about the rest of the tree reads as a claim that there was nothing else.

## Partial staging

When one file holds two unrelated changes, stage the hunks that belong to this commit:

```bash
git add -p path/to/file
```

In a non-interactive context, `git apply --cached` with a filtered patch works, but prefer proposing the split and letting a human run the interactive command when the hunks are entangled. A wrongly split hunk produces a commit that does not build, which is worse than asking.

Never assume all modifications in one file belong to one commit. That assumption is what makes single-file commits incoherent.

## Look at what is about to be committed

Before committing, read the staged file list, not just the staged diff:

```bash
git diff --cached --name-only
```

Stop and ask when something appears that a commit would not normally carry: `.env` or any file holding credentials, a key or certificate, a debug script, an editor or IDE directory, a large binary, a coverage or build output directory, or a test snapshot unrelated to the change.

This is not a security guarantee and should not be described as one. It catches the obvious case where a file was swept in by a broad `git add`, which is the common way secrets reach history. Once a secret is committed and pushed, removing it means rewriting history and rotating the credential, so the check is worth the two seconds.

## Commands never to run over user work

| Command | Why |
| --- | --- |
| `git checkout -- <path>`, `git restore <path>` | Silently discards uncommitted changes, with no reflog entry to recover from |
| `git reset --hard` | Same, for the whole tree |
| `git clean -fd` | Deletes untracked files permanently |
| `git stash drop`, `git stash clear` | Destroys parked work |
| `git commit --no-verify` | Bypasses the repository's own policy |

If a task appears to require one of these, stop and say why, rather than running it. `--no-verify` is wrong whether or not the repository configures any hooks.

## In-progress operations

Check before doing anything:

```bash
git status --porcelain=v2 --branch | head -3
ls .git/MERGE_HEAD .git/REBASE_HEAD .git/CHERRY_PICK_HEAD 2>/dev/null
```

A tree in the middle of a merge, rebase, or cherry-pick is not a normal dirty tree, and committing into one has different consequences. Finish or abort the operation deliberately, with authorisation, rather than committing through it.

## When you cannot tell

Ask. The cost of one question is a few seconds. The cost of committing someone's unfinished experiment, or of discarding it to get a clean tree, is work that does not exist anywhere else.

The same applies to grouping: proposing a commit breakdown and waiting is always available, and is the right move whenever the tree is mixed.
