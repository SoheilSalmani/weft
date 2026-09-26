# What belongs in one commit

## Contents

- The tests
- Build integrity
- Split when
- Do not split when
- The mechanical-change threshold
- Ordering a multi-commit branch

## The tests

Ordered by how often each settles the question.

| Test | Question | Weight |
| --- | --- | --- |
| **Revert** | Would reverting this undo exactly one decision, leaving a state someone would want? | Decisive |
| **Dependency** | Would splitting create a broken intermediate state? | Decisive against splitting |
| **Title** | Can one clear subject describe it? | Strong |
| **Bisect** | If this introduced a bug, does landing on it locate one change? | Strong |
| **Review** | Would a reviewer see why these belong together? | Supporting |
| **Cherry-pick** | Could this be reused on another branch alone? | Weak signal, never required |

The title test needs care. A subject containing "and" is a hint, not a verdict. `Add the constraint and backfill existing rows` may be one decision that cannot be half-applied. `Fix the retry race and correct a typo in the README` is two.

Size is not a test. A commit may touch two hundred files and be one change. A commit may touch one file and be three.

## Build integrity

Commits should normally leave the repository buildable and testable, because `git bisect` depends on it: a broken intermediate commit makes bisect blame the wrong one.

Two legitimate exceptions. A deliberate failing test committed first to demonstrate a bug, where the failure is the point and the message says so. And a migration whose intermediate states genuinely cannot build, which is an argument for one larger commit rather than for shipping broken ones.

## Split when

| Case | Reason |
| --- | --- |
| An unrelated fix found during feature work | Different decisions, different revert targets |
| A mechanical rename plus a semantic change | The rename hides the semantics in review and in blame |
| A large formatter run or codemod plus behaviour | Same reason, more acute |
| A refactor plus the feature it enables | Lets a reviewer confirm the refactor changed nothing |
| Several independent bugs | Each wants its own revert and its own bisect landing |
| A dependency upgrade plus non-trivial adaptation | The upgrade may need reverting alone |

## Do not split when

| Case | Reason |
| --- | --- |
| Implementation and the test that proves it | Reverting one alone leaves an inconsistent state |
| A source schema and its generated artifact | They must be synchronised at every commit |
| A rename and its call sites | Half a rename does not compile |
| An interface change and its required consumers | Same |
| A migration whose halves cannot exist independently | Splitting invents a broken state |
| One invariant implemented across many files | One decision, however many files |

Never one commit per file, and never one commit per coding step.

## Tests, specifically

Keep tests with the behaviour they verify. That is the default, and it is what makes the commit revertable as a unit.

Three legitimate reasons to separate them:

- A **regression test committed first**, failing, to demonstrate an existing bug. The message says the test is expected to fail.
- **Characterisation tests** capturing current behaviour before a refactor, so the refactor commit can show them still passing. The message says it captures existing behaviour rather than asserting desired behaviour.
- **Test infrastructure** that is independently meaningful, such as a new fixture harness other tests will use.

None of these is a ritual. Do not produce an implementation commit and a test commit for every change.

## The mechanical-change threshold

Not a line count. **Split when the mechanical part would hide the semantic part** in review or in `git blame`.

Twenty lines of incidental formatter output alongside a fix is noise not worth a second commit. Two thousand lines of codemod alongside the same fix buries it, and anyone running `git blame` afterwards lands on the codemod instead of the decision.

## Ordering a multi-commit branch

Order so each commit is comprehensible on its own and the sequence builds: prerequisite refactor, then schema, then behaviour, then cleanup.

A curated branch presents logical history rather than the order keystrokes happened, and that is legitimate. It stops being legitimate when producing it requires rewriting history that is already shared. Safety outranks narrative.
