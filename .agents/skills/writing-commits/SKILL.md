---
name: writing-commits
description: Groups working-tree changes into coherent commits and writes messages from the staged diff. Covers what belongs in one commit, protecting unrelated user work while staging, and subjects and bodies that stay useful in log, blame, and bisect. Use when asked to commit changes, split work into commits, improve a commit message, or revert something. For amending, rebasing, squashing, force pushing, or reverting a merge, use rewriting-history.
---

# Writing commits

A commit is the only artefact that outlives everything else. Branches get deleted, pull requests close, issues are archived, trackers are migrated. The commit stays, attached to the lines it changed through `git blame` and addressable by `git bisect`.

Its reader is not the reviewer. The reviewer has the branch and the conversation. The commit's reader found it two years later while investigating a line that surprised them, with none of that available.

**Quality begins with choosing the right diff, not with polishing a message around a bad boundary.** A perfect message on an incoherent commit is worse than a plain message on a coherent one, because it disguises the problem.

## Read the tree before touching it

```bash
git status                      # everything, including what was already here
git diff                        # unstaged
git diff --cached               # staged
git log --oneline -10           # house style, and where the branch is
git status --porcelain=v2 --branch | head -3   # rebase, merge, or cherry-pick in progress
```

**Identify pre-existing user changes.** Anything modified before this session belongs to the user, not to the task. If which is which cannot be established from the conversation, ask. Do not infer it from file type or from what looks related.

## Protect user work

This outranks tidy history, always.

- **Never `git add .` or `git add -A`** when unrelated modifications exist. Stage named paths. When the tree is genuinely clean apart from this task's own work, broad staging is fine; the rule is about unrelated work being present, not about the command.
- **Never** `checkout`, `restore`, `reset --hard`, `clean`, or `stash drop` over work you did not create.
- When one file holds two unrelated changes, use `git add -p` and stage the relevant hunks, or ask.
- **Never `--no-verify`.** A failing hook is the repository's policy, not an obstacle.
- Leave unrelated changes unstaged, and say in your response what you left alone.

Losing someone's uncommitted work is unrecoverable. A messy commit is not. `references/staging-safety.md` has the full procedure, the commands never to run over user work, and how to handle a tree that is mid-merge or mid-rebase. Read it before staging in a tree you did not start clean.

## Group before writing anything

Group by coherent change, never by filename, directory, or the order things happened.

The primary test:

> **Would reverting this commit undo exactly one decision, and leave a state someone would plausibly want?**

That settles most cases. A rename across 200 files is one commit, because reverting it undoes one decision. A fix plus an unrelated typo is two, because whoever reverts wants one of them. Implementation and its test are one, because reverting the implementation alone leaves a test asserting behaviour that no longer exists.

`references/cohesion.md` holds the full test set and the split and do-not-split catalogue. Read it when the grouping is not obvious.

Propose the grouping before creating anything, and never create a multi-commit sequence without approval.

**Do not commit coding steps.** `Add helper`, `Add tests`, `Fix imports`, `Run formatter` as separate durable commits are development narration, not history.

## Write the message from the staged diff

**Read `git diff --cached` before writing a word.** The message describes what is staged, not the task, the tracker item, the pull request, or the branch name. Those supply context; only the staged patch determines content.

A message written from intent rather than from the diff is the most common way history becomes actively misleading.

**Read `references/message-patterns.md` before writing any body.** It holds the seven cases that earn one, the bullet rule, the worked rewrites, and the shapes for reverts, cherry-picks, merges and squashes. The summary below is for orientation and is not sufficient on its own; skipping the reference is how over-long bodies get written.

- Imperative, sentence case, no trailing full stop. A `type:` prefix only where something in the repository consumes it, and **no scope unless the repository documents its scopes**.
- Specific enough to stand alone in `git log --oneline`, plain enough to read as a release note.
- Backtick code identifiers, and do not count the backticks toward the length budget: the interfaces these are read in render them.
- Short enough to scan, without contorting to hit a character count.
- **A body only when the diff cannot explain why, and most commits do not need one.** A routine fix gets no body or one line. Before keeping any line, ask whether the diff already answers it.

Then run the message through `humanizer` before committing. Commit prose attracts the same tics as any other writing, and an em dash standing in for a colon is the most common one here.

## What a repository does, and how to find out

Establish the repository's facts before writing, because they decide the subject format:

```bash
ls release-please-config.json .commitlintrc* CHANGELOG.md 2>/dev/null
cat .pre-commit-config.yaml 2>/dev/null | grep -A6 conventional
grep -rl "conventional" .github/workflows/ 2>/dev/null
gh api repos/{owner}/{repo} --jq '{squash_title: .squash_merge_commit_title, squash_message: .squash_merge_commit_message}'
git log --format='%s' -40 | grep -cE '^(feat|fix|docs|chore|refactor|ci|test)(\(.+\))?: '
```

**No consumer of the prefix means no prefix.** Where a repository has no release tooling, no changelog generation, no commitlint and no CI check, the format costs the first characters of every permanent subject and nothing reads it. Do not introduce the syntax on style grounds.

**Where something does consume it, the type is load-bearing.** Release tooling maps each type to a changelog section and a version bump, and a CI title check or a `commit-msg` hook rejects anything else. Use only the types that configuration lists, rather than the Conventional Commits defaults from memory. A hook configured for the `commit-msg` stage fires only where that hook type is installed, so a format error may first surface in CI.

**Where feature branches squash**, the pull request title becomes the durable subject and the squash setting decides the durable body, so a long body is not private to your branch. `references/message-patterns.md` covers the settings.

Match a repository's *enforceable and meaningful* conventions, which means linters, required syntax, and a house style that is actually good. Do not imitate low-quality history. A repository whose log reads `updated stuff`, `fix`, `misc` does not establish a convention worth continuing, and writing one more `misc` because the neighbours did is how that history got there.

## Trailers

Use only established trailers that a tool or a person actually reads. `Co-Authored-By` is established and rendered by GitHub. Do not invent custom trailers, and do not confuse a sign-off with a cryptographic signature.

On attribution: follow the environment's policy where one exists, and add nothing otherwise. In Claude Code that policy is `attribution.commit` in `.claude/settings.json`; when it is set to an empty string, no attribution trailer goes in the commit. Do not add AI attribution on your own initiative.

**The tracker ID never goes in the commit.** Not the subject, not the body, not a trailer. It lives in exactly one place: the pull request title, as a suffix, `My title [PROJ-123]`. Not the branch name either, which takes a plain slug like `fix/dbt-parse-warnings`. An ID in a commit is meaningless the moment the tracker is migrated, and the pull request title already carries it into permanent history where the repository squashes. When a message needs to point at prior work, cite the commit SHA, which outlives any tracker. `references/message-patterns.md` covers closing keywords and why they stay out too.

## Reading history

The messages are written for `git log`, `git blame`, and `git bisect`. `references/reading-history.md` covers the commands and, more usefully, when to consult history before changing code: when the code is surprising, not when the change is large. Do not perform archaeology for a trivial change.

## Do not commit nothing

If the working tree holds no change, there is no commit to make. Do not manufacture one to mark that work happened.

The exception is an explicit operational request, such as an empty commit to retrigger a deployment. That is a real use of `git commit --allow-empty`, and the message should say what it is for rather than pretending to describe a change.

## Automated commits

Leave bot commits alone. A dependency bot's lockfile bump needs no curation, and rewriting its message can break the automation that reads it. Curate the human commit that adapts the code to the upgrade, not the bump itself.

## Authorisation does not cascade

```
propose a grouping → stage → commit → amend → rebase → force push → merge → revert
```

Each rung needs its own authorisation. "Commit my changes" authorises staging and creating commits. It does not authorise amending a pushed commit, rebasing, force pushing, merging, or reverting anything.

Everything from amend rightwards belongs to `rewriting-history`. Route there rather than improvising, and never rewrite history that others may have built on.

## Before you finish

- Every message describes the diff that is actually staged, which you read.
- Each commit would revert as one decision.
- Nothing unrelated to the task was staged, and nothing of the user's was discarded.
- No subject is `Fix`, `Update`, `WIP`, or an issue number standing in for meaning.
- No body narrates files, and no body exists that the subject and diff already covered.
- No new syntax, prefix, or trailer was introduced that nothing consumes.
- Nothing was amended, rebased, or pushed without explicit authorisation.
