---
name: writing-pull-requests
description: Writes and updates pull request titles and descriptions that someone outside the work can understand, and decides whether a branch is ready for review. Covers the required Summary and the optional Verification, Notes and What's next sections, writing for a reader who has never opened the code, putting measurements in a table, cutting anything the diff or CI already shows, what belongs in the body rather than in the commits, linking commits, title rules where CI or a changelog consumes the title, and draft state. Use when opening a pull request, rewriting its description, or asking whether a change is reviewable.
license: MIT
---

# Writing pull requests

Every pull request here uses the same shape, so a reviewer learns it once and then reads every
description the same way.

**One section is required. The rest earn their place or stay out.** A description that says one true
thing clearly beats a complete one nobody finishes.

The body carries what the diff and the commits cannot: what was wrong, what is different now, and
anything that would otherwise be a question. It never carries the file list, the commit subjects, or
a retelling of the diff.

**Read the diff and the commit messages before writing a word.** A branch name, a ticket title and
the prompt that started the work are clues, not the change. When the ticket and the diff disagree,
the diff wins: describe what is there, link the ticket for the original intent, and spend one line
on why the approach changed.

```bash
git log --oneline <base>...HEAD
git diff --stat <base>...HEAD
git diff <base>...HEAD
```

## The template

```markdown
## Summary

<what was wrong, as a person would notice it>
<why it was happening, in one plain sentence>
<what is different now>

## Verification        <- optional, and rare

<the one thing CI cannot show>

## What's next         <- optional

<work this depends on or unblocks, one or two lines>

## Notes               <- optional

- <a risk, a blocker, where to look first, or why not the obvious approach>
```

**`## Summary` is the only required section.** The other three are omitted unless they carry something
a reviewer cannot get elsewhere. Three sections of real content beat four with one padded.

The minimal form is one section. A typo fix does not need a heading count:

```markdown
## Summary

The README heading had a typo. This fixes it.
```

## Write for someone outside the work

**Assume a reader who knows the product but has never opened this corner of the code.** A reviewer from
another team, or you in six months. This is the rule that most often goes wrong, because the author
knows too much to notice.

**Open with the problem as a person would notice it**, not as the code expresses it. "Pages were taking
about six seconds" comes before any mention of query planning.

**Give the cause in one plain sentence before naming the fix.** Plain wording reaches a far larger
audience at no cost in accuracy:

> The database was not reading too much data. It was spending its time working out *how* to read it,
> over and over.

against the same fact written for the author:

> Portfolio reads are compilation-bound rather than scan-bound.

**A term that would need a glossary is a term to replace.** Keep an identifier only where a reviewer
needs it to navigate to something.

**The test, before posting:** could someone who has never seen this repository say what was wrong and
why this helps? If not, rewrite it. Do not add detail — detail is usually what broke it.

## Put measurements in a table

Two or more numbers go in a table with a before and after column. Prose buries exactly the comparison
the reader came for, and a row of bullets is a table someone has to assemble in their head.

```markdown
| Planning time | Before | After |
| --- | --- | --- |
| Project metadata | 182ms | 83ms |
| Scoring | 312ms | 155ms |
```

A single number can stay inline.

## Cut anything the diff or CI already shows

Before posting, delete every line that a reviewer could get from the files changed tab, a green check,
or a commit body. In practice that means the file list, test names, lint and parse results, build
success, row counts an assertion already guards, and any command the pipeline runs.

What survives is what none of those can tell them: why it was slow, why this approach, what is still
missing.

## How the sections read

These apply to every section you keep.

**A heading is never followed straight by bullets.** Where a section uses a list, one sentence above it
says what the list is, so a reader is not assembling the point from fragments.

**Enumerate in a list.** Three test names or three affected tables run together in a sentence force
a reader to parse commas as structure. One item per line, introduced by the sentence above it.

**Write prose, not notation.** `OVERRIDDEN > automated > MANUAL` is a diagram, not a sentence: say
which one wins and in what order. Arrows, comparison operators and abbreviations a reviewer has to
expand all cost more than the characters they save.

**Name the effect, not the symbol.** The body is read by people who will not open the diff, so an
identifier lifted out of the code is dead weight to most of them. "Test argument warnings" carries
what `MissingArgumentsPropertyInGenericTestDeprecation` does, in a quarter of the space and without
a reader having to decode it. Keep a file or config name only where a reviewer needs it to navigate,
never as evidence that work happened. If a body is mostly backticks, it is written for the author.

**Nothing about deploying, releasing or rolling out.** A pull request is a change under review, and
what happens after it merges belongs to whatever owns the deployment.

Then run the body through `humanizer` before posting. Description prose attracts the same tics as
any other writing, and an em dash standing in for a colon is the most common one here.

## Summary

Three short paragraphs, or bullets where the change is a list of unrelated outcomes. **Under 150 words.**
Problem, cause, what is different now, in that order, because that is the order a reader needs them.

- Outcome, not mechanism: "retries no longer double-charge", not "add an idempotency key column".
- Not the file list, not the commit subjects. The interface renders both.
- Where a point needs a caveat to be accurate, the caveat goes in `## Notes`, not in brackets.

## Verification

**Omit this section by default.** It exists for the rare case where something load-bearing is invisible
to CI, and it is the section most likely to fill with noise.

Include it only for:

- a measured result that is the point of the change,
- a correctness argument CI cannot express, such as showing two versions return identical rows,
- a gap worth stating plainly, such as a path never exercised against a real environment.

**Never include any of these, because a green check already says it:** lint, parse, unit tests, schema
tests, a successful build, row counts an assertion already guards, or any command the pipeline runs.
Listing them buys nothing and costs the reader the lines where the real content should have been.

Never turn "should pass" into "passes", and never write "no regressions" or "safe" as an inference
from intent. **A stated gap beats a claim**, because a reviewer can act on it: "nothing was run
against production" is more useful than silence, and honest where "tested" would not be.

## What's next

Optional, one or two lines. Use it when the change **depends on work elsewhere to deliver anything**, or
when it obviously unblocks something. Name the system or repository, not a ticket ID, so the line still
reads years later.

> Nothing changes until the backend reads this model instead of the five relations it queries today.
> That work is in the web services repository.

This is not a rollout plan. Deployment, release and environment promotion stay out, as below.

## Notes

Omit the section when there is nothing. **A padded section is worse than an absent one**, and an
invented risk costs a reviewer more than silence.

What is worth a note, roughly in order of value:

- **A blocker.** Something that must happen before merge and that the interface cannot show.
- **A risk.** What breaks, what changes in production, what is hard to reverse.
- **Where to look**, when the diff is large or lopsided. One or two places.
- **Why not the obvious approach**, when a reviewer would otherwise ask.

## Length

**Under 200 words**, and stop at 400. Reviewers skim past roughly 400 lines of anything and find
fewer defects per line, so a long description transfers less information, not more. The shape is a
floor on structure, never a licence to fill it.

When you are over, cut a whole section rather than trimming every sentence. The first candidates are
always the same: anything CI proves, anything the diff shows, and any sentence explaining the change
to yourself rather than to the reader.

## Do not restate the commits

Where the merge keeps the commit messages, everything in a commit body is already permanent history.
Repeating it doubles the reading, and the two copies drift. Check what the merge keeps:

```bash
gh api repos/{owner}/{repo} --jq '{squash_title: .squash_merge_commit_title, squash_body: .squash_merge_commit_message}'
```

Point at a commit instead of re-explaining it, and **link it rather than quoting the hash alone**,
because a bare hash is not clickable and a reviewer will not run `git show` to read it:

```markdown
both are explained in [`5b73107`](https://github.com/<owner>/<repo>/commit/5b73107)
```

## Title

Usually permanent, so treat it as a commit subject: imperative, sentence case, no trailing full
stop, specific enough to stand alone.

Check what the repo enforces and what reads it before writing one, rather than assuming: a CI title
check, a changelog generator keyed on a type prefix, a tracker reading a suffix. Where a changelog
publishes the title it has a second audience who does not know the codebase, so plain beats precise.

**Never invent a scope.** Conventional-commit tooling usually makes the scope optional, which means a
title check passing tells you nothing about whether the scope you chose is one the project recognises.
Use a scope only where the project has written down which scopes exist: a `scope-enum` in a commitlint
config, an allowed list in a PR-title action, or an explicit list in the contributing or agent docs.

A scope invented on the spot reads as though it were a shared convention. The next author copies it,
picks a different one for the same area, and the prefix now sorts a changelog into categories nobody
agreed. Omit it and let the description carry the specificity:

> `perf: halve view planning time on the portfolio read path`

rather than `perf(dwh):` where no list of scopes exists to make `dwh` mean anything.

**Backtick code identifiers, and do not count the backticks toward the length target.** Package,
table, column, flag and config names get backticks, exactly as in a commit subject, and the title is
measured with them stripped.

Know what this costs, because it is not free the way it is in a commit subject: **GitHub does not
render markdown in a pull request title**, so the backticks appear as literal characters on the pull
request page. They are worth it anyway where a changelog generator republishes the title, because
`CHANGELOG.md` is rendered markdown and the identifier reads as code there, which is the audience
that outlives the pull request page. Where nothing republishes the title, this is a style call rather
than a functional one.

**A tracker ID belongs in the title suffix, and nowhere else.** Not the branch name, not a commit
message, not the body.

The suffix carries it once, and it survives where it matters: the title becomes the permanent squash
subject, so the ID stays reachable from history after the branch is deleted. A second copy in the
body is a dead reference to any reader without tracker access. Say "left for follow-up" rather than
naming the ticket, and where the body needs to point at related work, **point at another pull
request**, which is clickable, readable without a second system, and still resolves years later.

**Branch names carry a slug, not an ID**: `fix/dbt-parse-warnings`, not
`fix/PROJ-123-dbt-parse-warnings`. The ID in a branch name is redundant with the title and unhelpful
in `git branch` output.

**Renaming a branch that has an open pull request is not free.** GitHub's rename can close the pull
request rather than retarget it, and a closed one cannot be reopened while its head ref is missing.
Name the branch correctly when you create it; once a pull request is open, leave it alone.

## Mutation safety

Each of these needs its own authorisation. Drafting text is free. Creating a pull request or editing
its title or body means showing the exact text and waiting. Converting draft to ready, requesting
reviewers, merging and approving are separate asks, and approving is never an agent's decision.

## Before you finish

- **Someone who has never opened this code could say what was wrong and why this helps.** If not, nothing else on this list matters.
- `## Summary` is present and leads with the problem, then the cause in plain words, then what changed.
- Every optional section present is carrying something. `## Verification`, `## What's next` and `## Notes` are all absent by default.
- Verification, if present, holds no lint, parse, test, build or row-count result. A green check already says those.
- Two or more measurements are in a table, not in prose.
- No raw class, error or warning name where plain words carry it. The body is not mostly backticks.
- No tracker ID in the body. The title suffix carries it; related work is linked as a pull request.
- Under 200 words, or there is a stated reason it is not.
- Nothing in the body also appears in a commit body, the diff, or the interface.
- Any commit referenced is a link, not a bare hash.
- No em dashes, and nothing about how the change gets deployed.
- The title works as a permanent commit subject, and passes whatever the repo enforces.
- The title carries a scope only if the project has written down which scopes exist. Otherwise there is no scope.
