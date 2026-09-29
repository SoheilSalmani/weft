# Message patterns

## Contents

- Subject
- Body
- Worked rewrites
- Reverts
- Cherry-picks
- Merges
- Squash merges
- Conventional Commits
- References and trailers

## Subject

Imperative, sentence case, no trailing full stop. A `type:` prefix where tooling consumes it, and a scope only where the repository documents its scopes (see `SKILL.md`).

**Backtick code identifiers**: table and column names, procedures, config keys, paths, flags, packages. `nest \`meta\` under \`config\`` reads as one thing rather than two English words, and it survives being quoted into a pull request or a ticket.

**Do not count the backticks toward the length target.** GitHub and GitLab render them as code when displaying a commit, so they cost the reader nothing in the interface where these subjects are actually read. Measure the subject with them stripped. The one real cost: a terminal `git log --oneline` prints them literally, so a subject already at the top of the budget with several backticked identifiers can wrap there. Worth knowing once, not worth dropping the rule for.

Imperative because the subject completes "Applying this commit will ___", and because Git's own generated subjects are imperative: `Merge branch ...`, `Revert "..."`. Past tense describes the author's activity rather than the change.

On length: Git calls a subject under 50 characters "a good idea", not a requirement, and the only hard rule is that the text up to the first blank line is the title. Aim to stay under roughly 72 so the subject survives `git log --oneline` and GitHub's commit list, and do not mangle meaning to reach 50. A subject truncated into ambiguity is worse than one running slightly long.

A subject running past about 90 characters is a different problem: it is usually carrying detail that belongs in the body, or describing two changes. Move the detail down or split the commit, rather than trimming words until it fits.

**Where a changelog generator reads the subject, it has a second audience.** With release-please, the text after the colon is published to `CHANGELOG.md` for readers who do not know the codebase. That pulls against archaeological specificity, and plainness wins: `correct the data-quality invariants for overridden scores` over `stop asserting OVERRIDDEN implies SCORING_METHOD = 'MANUAL'`. Expand internal abbreviations, drop operators and quoted literals, and write a sentence rather than a predicate. The body is where precision goes.

| Weak | Why | Better |
| --- | --- | --- |
| `Fix bug` | True of most commits ever made | `Prevent duplicate webhook claims during retries` |
| `Update auth` | Names an area, not a change | `Remove the legacy token validation path` |
| `Changes` | Not a sentence, not information | Name what changed |
| `Address review comments` | Describes the workflow | Describe what the code now does |
| `WIP` | Should never reach durable history | Squash it before integration |
| `Tests` | Which tests, proving what? | `Cover failed OAuth refreshes in integration tests` |
| `Fix #123` | Meaningless once the tracker is gone | Put meaning in the subject; the ID belongs to the branch and the pull request title |
| `Fix typo and lint` | Two unrelated things | Two commits |

## Body

Optional, and most commits do not need one. Write one when the diff cannot explain:

- Why this approach rather than the obvious one.
- An invariant that now holds, or one that was deliberately preserved.
- A non-obvious cause, where the fix looks unrelated to the symptom.
- A compatibility constraint.
- A surprising side effect.
- Why a simpler-looking implementation is wrong.
- That a workaround is temporary, and what removes it.

**One test, applied to every line before you keep it: would the diff answer this?** If yes, cut the
line. This is the rule that fails most often in practice, and it fails quietly, because a body that
restates the diff still reads as thorough and informative. It is not. A body naming which files
changed, how many there were, what the new structure looks like, or which warning count went down
has failed the test outright.

**A routine fix gets no body, or one line.** Two or three lines is already the upper end, and it
should be carrying a cause rather than a description. If the body is running past that, check
whether the subject is really one change.

**Do not narrate the work.** That a file was missed in an earlier commit, which commit came first,
what you tried before this, how long it took, that review asked for it: none of that is durable
history. The reader in two years wants the state of the code and the reason for it.

Wrap near 72 columns, because Git does not wrap and neither does `git log` in a terminal.

No headings. No `Summary`, `Why`, `Testing`, `Notes`, or `Context` sections. A commit message is not a small pull request description. Short paragraphs.

**Bullets only where each item is a reason.** A list of *changes* is the diff restated, and it passes a naive reading of "genuinely a list" while adding nothing: three bullets naming three modified tests are three things the reader can already see. Three bullets giving three independent causes are worth the lines. When in doubt, prose.

Do not put routine verification in the body. The pull request owns that. The exception is when the verification *is* the point, such as a characterisation commit stating that it captures existing behaviour.

## Worked rewrites

**Narration into meaning**

> `Update webhook.ts and add migration`

becomes

> `Claim webhook deliveries inside the processing transaction`

**A body that earns its place**

> `Preserve the token family when refresh rotation races`
>
> Two concurrent refreshes could each rotate the family, and the second
> write silently invalidated the first client's token. The obvious fix,
> locking on the token row, does not help: the race is between families,
> not rows.
>
> Rotation now claims the family with a conditional update, so the loser
> reuses the winner's token instead of issuing a second one.

The subject states what is now true. The body says why the obvious fix fails, which is the one thing the diff cannot show.

**A multi-concern body, and what got cut**

A change with three genuine reasons still gets one short paragraph each, not a section per reason. Before, at thirty-six lines, it narrated the guard mechanics, the new fallback step, a cleared column, a rewritten test, and a sign-off note. After:

> `fix: let automation supersede a plain manual score`
>
> This rule existed once, in `DTM_FCT_SCORING`'s `BEST_MANUAL` CTE
> (21c5abd), and was lost when 4597cf5 moved precedence into
> `SP_REFRESH_SCORING` with only the older manual-always-wins guard
> (1de0e9e).
>
> Any row still reading `MANUAL` on one of the seven rule-mapped criteria
> is superseded on the first refresh after deploy.

Two paragraphs survive because two things are unrecoverable from the diff: the rule was implemented and silently lost, and the deploy changes data immediately. Everything cut was either the diff itself or already a comment in the code.

**This example is the ceiling, not the target.** It earns two paragraphs because a behavioural rule was lost in a refactor and the deploy mutates data on first run. A mechanical fix earns nothing like it. Do not read the length here as a model.

**The routine case, which is most commits**

> `fix: nest source \`freshness\` and \`loaded_at_field\` under \`config\``

No body. The subject names the change, the diff shows the two keys moving, and there is no cause to record: the tool deprecated the old position. A body here could only restate the diff.

Where one line genuinely helps, it is one line:

> `fix: nest \`node_color\` under \`docs\``
>
> dbt validates it at `config.docs.node_color`, not as a custom key.

That single line survives because it contradicts the obvious fix. The deprecation message points at `config.meta`, and following it would be wrong; the diff cannot tell you that.

**A body that does not**

> `Add the retry helper`
>
> This adds a helper function to handle retries. It is in retry.ts and is
> called from the webhook handler.

The diff already says all of that. No body.

## Reverts

Keep Git's generated subject, `Revert "<original subject>"`, so the relationship stays discoverable by search and by eye. Add a body when the reason is not obvious: why it is being reverted, whether it is temporary, and what follows.

Do not claim the reverted change was wrong when the reason was operational. "Reverting to unblock the release; the change itself is sound and returns in #45" is honest and more useful.

**Reverting a merge commit is a different operation.** See `rewriting-history`, and do not run it from here.

## Cherry-picks

Use `git cherry-pick -x` when moving a commit between branches, which appends the source SHA and preserves provenance.

Keep the original message. It already explains the change, and rewriting it to `Backport fix` destroys the reason someone will need on the branch they are reading. Add one line only when the target branch changes the context.

## Merges

For a routine pull request merge, GitHub's generated message is adequate and should be left alone. Under the default setting it produces `Merge pull request #N from ...` with the pull request title as the body, which carries the reference and the meaning already. Check which merge and squash settings the repository uses with `gh api repos/{owner}/{repo} --jq '{squash_merge_commit_title, squash_merge_commit_message, merge_commit_title, merge_commit_message}'`.

For a manual integration merge, the parents already record what was combined, so the body should not restate it. Write a body only when conflict resolution made a real choice: which behaviour was kept when both sides changed the same thing, or a constraint the integration introduced.

If resolving conflicts changed behaviour beyond mechanically choosing a side, say so. A semantic fix hidden inside a merge is invisible to review and to blame.

## Squash merges

Where a repository squashes, the squash commit may be the only durable record of the whole pull request, so its subject carries everything.

Under GitHub's `COMMIT_OR_PR_TITLE` setting, a pull request with exactly one commit takes its subject from **the commit**, and one with two or more takes it from the **pull request title**. Where the repository uses it, and a branch has one commit, write the commit subject and the pull request title as the same sentence.

Do not let a long pull request body become the permanent commit body. Someone reading `git log` wants the change, not the review notes. A pull request body carrying screenshots, a CI checklist, review discussion, and rollout notes is right for review and wrong for history: keep the part that explains the change, drop the rest, and edit the message in the merge box rather than accepting it whole.

Watch the body setting as well as the title. `COMMIT_MESSAGES` for the squash body concatenates the branch's commit messages. Where the repository uses it, a branch of twelve `WIP`, `fix tests`, and `oops` commits therefore produces a permanent commit body listing all twelve. Either curate the branch before merging, or replace the body at merge time.

## Conventional Commits

Use it only where something consumes it. Where commitlint, semantic-release, or changelog generation consumes the format, conform exactly, then ignore the format and write a good message anyway.

- **The type is not the description.** `fix(auth): bug` satisfies the linter and tells a future reader nothing. The description after the colon still has to name the change.
- **Omit the scope unless the repository documents its scopes.** The specification makes scope optional (clause 1: "the OPTIONAL scope"; clause 4: "A scope MAY be provided") and defines no list, so nothing validates what you invent. If a contributor cannot find the allowed scopes written down, there is no shared vocabulary and each author coins their own: `dq`, `scoring`, `models`, `dbt` all describing the same area. Look for a documented list in `CONTRIBUTING.md`, `commitlint.config.js` (`scope-enum`), or the changelog config before using one.
- **Where scopes are documented, a scope is still a stable architectural domain**, not a list of touched packages. `feat(auth)` is useful when `auth` is a durable boundary. `feat(foo-bar-baz-api-util-core)` is a file list wearing a scope, and a change spanning five packages usually wants no scope or a split.
- **`BREAKING CHANGE` describes a consumer-visible contract break**, not internal movement. Moving files, renaming private symbols, or restructuring modules is not breaking. Removing a public method, changing a response shape, or dropping a supported input is.
- Where the repository has no such tooling, do not introduce the syntax. It costs the first characters of every permanent subject and nothing reads it.

## References and trailers

Trailers go in a block at the end, after a blank line, as `Key: value`.

Established and useful: `Co-Authored-By`, which GitHub renders, where the environment's attribution policy asks for it.

**No tracker ID, in any position.** Not a trailer, not the subject, not the body. The pull request title carries it, as a suffix, and that is the only place it goes: not the branch name either, which takes a plain slug. A commit does not need it twice and the ID stops resolving the day the tracker is replaced. Cite a commit SHA instead when a message must reference prior work.

Do not use closing keywords in commit messages in this workflow. The pull request already carries the closing relationship, and a keyword in a commit can close an issue from the wrong branch or close it twice.

Do not invent custom trailers. A trailer must be read by a tool or a person to be worth the permanent line.
