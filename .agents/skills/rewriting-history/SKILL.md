---
name: rewriting-history
description: Use only when explicitly asked. Amends, reorders, splits, squashes, or reverts commits that already exist, and force pushes the result safely. Covers when history is safe to rewrite, autosquashing fixup commits, and the distinct semantics of reverting a merge. Use when asked to clean up commits, amend, rebase, squash, drop, or revert something.
---

# Rewriting history

Every operation here is destructive, disruptive, or both. None of them should start because history looked untidy, which is why this skill runs only when you ask for it.

Creating new commits is `writing-commits`. This skill changes commits that already exist.

## Is this safe to rewrite

Establish the answer before proposing anything.

```bash
git log --oneline @{upstream}..HEAD 2>/dev/null   # commits not yet pushed
git status -sb                                    # ahead or behind
gh pr list --head "$(git branch --show-current)" --json number,reviews
```

| State | Rewriting |
| --- | --- |
| Unpushed local commits | Free. Amend, rebase, reorder, split |
| Pushed, personal branch, no pull request, nobody else has it | Allowed with authorisation, using `--force-with-lease` |
| Pushed with an open pull request under review | Discouraged. Rewriting detaches review comments from their lines and makes reviewers re-read. Prefer a follow-up commit, or fixups squashed at merge |
| Others may have based work on it | **Never** without explicit coordination |
| `main`, or any shared branch | **Never** |

Pro Git states the rule plainly: "Do not rebase commits that exist outside your repository and that people may have based work on." The reason is collaborators, not aesthetics. Someone who has built on a commit you rewrote ends up re-merging duplicated commits with the same author, date, and message.

The synthesis worth following: rebase local work before pushing, and never rebase anything already pushed that someone else may hold.

**Check how the repository merges pull requests** (`gh repo view --json mergeCommitAllowed,squashMergeAllowed,rebaseMergeAllowed`, or merge commits in `git log --merges`). Under merge commits or rebase merges, branch commits land on the default branch individually, which raises the value of cleaning a branch before it merges. Under squash, the branch history is discarded and the squash message is what lasts. Either way, anything already on the default branch is permanently off limits.

## Amending

`git commit --amend` replaces the tip commit. It is right for the commit you just made and have not pushed.

Before amending anything else, check the table above. Amending a pushed commit requires a force push, which is a separate authorisation.

When amending changes what the commit contains, **reread the diff and update the message**. An amended commit whose message describes the previous version is worse than the original, because the message now lies with the authority of history.

## Fixup commits

During development, attach a correction to the commit it belongs to rather than inventing a new one:

```bash
git commit --fixup=<sha>            # content fix, message untouched
git commit --fixup=amend:<sha>      # content fix, and edit that commit's message
git commit --fixup=reword:<sha>     # message only, no content
```

Then, before integration:

```bash
git rebase -i --autosquash <base>
```

Neither `fixup!` nor `amend!` changes the authorship of the target commit.

Leave no `fix tests`, `oops`, or `address review comments` commits in durable history here, because merge commits carry branch commits onto `main`. In a repository that squash merges, branch commit hygiene matters much less and polishing it is wasted effort.

## Interactive rebase

Legitimate reasons: combine fixups, split a mixed commit, reorder so prerequisites come first, reword an inaccurate message, drop an accidental commit.

Not a reason: making subjects uniform, or imposing a narrative on history that is already accurate and safe.

Splitting a commit:

```bash
git rebase -i <base>        # mark the commit 'edit'
git reset HEAD^             # unstage its changes, keeping the working tree
# stage and commit the pieces separately, per writing-commits
git rebase --continue
```

If a rebase goes wrong, `git rebase --abort` restores the starting state, and `git reflog` finds commits that appear lost. Say so rather than improvising recovery.

## Force pushing

Always `--force-with-lease`, never bare `--force`:

```bash
git push --force-with-lease --force-if-includes
```

`--force-with-lease` refuses the push if the remote ref no longer points where you expect, so you cannot silently overwrite someone else's push. Its limitation is documented and specific: it "interacts very badly with anything that implicitly runs `git fetch` on the remote to be pushed to in the background", such as a fetch on a cron job, because the background fetch updates your remote-tracking ref and the check then passes against work you never saw.

`--force-if-includes` closes that gap by verifying "if the tip of the remote-tracking ref is reachable from one of the reflog entries of the local branch", meaning you actually integrated the remote work before replacing it. Git's own documentation recommends pairing them.

Never force push a branch you did not create without explicit coordination, and never force push `main`.

## Reverting

For an ordinary commit, `git revert <sha>` creates a new commit undoing it. Keep the generated subject so the relationship stays discoverable, and add a body when the reason is not obvious. This is safe: it adds history rather than rewriting it.

### Reverting a merge is a different operation

Git's own documentation is explicit: reverting a merge commit "undoes the *data* that the commit changed, but it does absolutely nothing to the effects on *history* that the merge had."

The consequence bites later. Once a merge is reverted, merging that branch again brings in nothing: "none of the changes made in A or B will be in the result, because they were reverted". The branch looks merged to Git, so its commits are never reapplied.

The remedy is to revert the revert before merging again.

Rules:

- **Never run `git revert -m` without stating this consequence first.** Explain that the branch will need its revert reverted before it can usefully merge again.
- Prefer reverting the individual commits, or fixing forward, when either is viable.
- `-m 1` treats the first parent as the mainline, which is the branch that was merged into. Getting this backwards inverts the revert.

## Authorisation

| Act | Default |
| --- | --- |
| Inspect history, propose a plan | Free |
| Amend an unpushed commit | Ask |
| Rebase unpushed commits | Ask |
| Rewrite anything pushed | Explicit authorisation, after stating what breaks |
| Force push | Explicit authorisation, with `--force-with-lease` |
| Revert an ordinary commit | Ask |
| Revert a merge | Explicit authorisation, after explaining the re-merge consequence |
| Rewrite `main` or shared history | Refuse |

Authorisation for one operation is never authorisation for the next. "Squash my commits" does not authorise the force push that publishing them requires; say that the push is needed and ask.

## Before you finish

- The safety tier was established from the actual branch state, not assumed.
- Any rewritten commit's message still describes what that commit now contains.
- Nothing shared or based-upon was rewritten.
- Any force push used `--force-with-lease`.
- A merge revert, if any, was preceded by an explanation of the re-merge consequence.
- Uncommitted work in the tree survived the operation.
