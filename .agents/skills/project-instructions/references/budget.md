# The persistent-context budget

## The principle

Always-loaded instructions are paid for on every task. A skill that never triggers costs nothing; an instruction that is irrelevant to today's task still costs its tokens and still competes for attention. Worse, a file full of rarely-relevant rules dilutes the ones that matter. Vendors report that adherence falls as instruction files grow, which means adding a marginal rule can make the important rules **less** likely to be followed.

So the question is never "is this true?" It is:

> Is this useful often enough to deserve being present during most coding tasks?

## Four inputs, not one

Frequency alone is the wrong test. Weigh:

1. **Frequency.** What share of ordinary tasks does this affect?
2. **Cost of omission.** What happens when an agent does not know? Wasted minutes, or a silently destroyed file?
3. **Discoverability.** Could the agent find this cheaply and reliably (one `ls`, one config read), or does it need archaeology across many files?
4. **Stability.** Will this still be true next month? Volatile facts decay into confident lies.

High frequency with trivial cost of omission is often not worth it: the agent will discover it immediately anyway. **Low frequency with catastrophic, silent cost of omission usually is worth it.** That asymmetry is why "do not edit this generated directory" earns permanent space while "we use tabs" usually does not.

## Worked cases

| Candidate | Verdict | Why |
| --- | --- | --- |
| `Use pnpm, never npm or yarn` | **Include** | Affects nearly every task; wrong choice corrupts the lockfile; one line |
| `src/generated/ is generated from schema/foo.yaml, do not edit` | **Include** | Rare but silent and destructive; not discoverable without reading the generator |
| `Only the billing service writes billing state. See ADR-012` | **Include, compressed** | Broad constraint, plausibly violated unknowingly, stable one-liner |
| Full ADR-012 rationale | **Exclude** | The decision is the instruction; the reasoning is retrieved when questioned |
| `All TypeScript is formatted with Prettier` | **Exclude, route to tooling** | Already enforced. Keep only if there is a non-obvious operational fact, such as a path the formatter skips |
| 80-line pull request procedure | **Exclude, route to skill** | Recognisable task class, needed on a minority of tasks |
| Release checklist, migration runbook | **Exclude, route to skill** | Lifecycle procedures, rarely relevant |
| `We are implementing the Q3 auth migration` | **Exclude** | Transient. Durable files are the wrong home for status; it will be wrong by Q4 and believed anyway |
| `Write clean code and add tests` | **Exclude** | Zero information. Changes no decision |
| Full directory tree | **Exclude** | Cheaply discoverable, expensively wrong |
| Dependency list | **Exclude** | `package.json` is the source of truth and cannot drift from itself |
| `pnpm test:openrouter makes billable API calls` | **Include** | Low frequency, high consequence, not visible from the script name |
| `There is no CI, so local runs are the only verification` | **Include** | Changes what "verified" means on most tasks; absence is not discoverable by looking |
| `Prefer const over let` | **Exclude** | Obvious from surrounding code, and a linter's job |

## The two-sided test

Run both directions before finalising a file.

**Remove it.** Imagine an ordinary implementation session with no instruction file. What essential repository knowledge is now missing? Whatever is important, frequently applicable, and risky to rediscover is a genuine candidate. Anything you would not miss was decoration.

**Load only it.** Imagine an ordinary task unrelated to commits, pull requests, ADRs, releases, migrations, or issue management. Read the file as if you were that agent. How many lines were irrelevant? Everything task-specific belongs in a skill.

A well-tuned file survives both: nothing essential is missing, and almost nothing in it was wasted.

## On line limits

Do not encode folklore such as "always under 100 lines". Optimise for relevance and density instead. Where a vendor publishes actual guidance, follow it and record where you read it. Claude Code documents a 200-line target per file and notes that longer files reduce adherence. A hard byte cap, where one exists, is a compatibility fact rather than a style preference: content past it is silently dropped, so a file that fits one host may be truncated on another.
