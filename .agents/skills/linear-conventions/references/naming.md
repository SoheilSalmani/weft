# Naming Linear objects

## Contents

- Why names carry more than they look like they do
- Rules
- Form by object
- The rewrite ladder
- Worked judgements
- When an implementation title is correct

## Why names carry more than they look like they do

A title is read where nothing else is: search results, notifications, the command menu, roadmap bars, cross-links, pull request titles, and agent tool results. In all of those, the properties are invisible. A title that only makes sense next to its label and its project is a title that fails everywhere it is actually read.

That gives two requirements that pull against each other. A title must stand alone, and it must not restate anything a property already holds. What is left is the change itself.

## Rules

- **Name the change, at the object's altitude.** The altitude is the only difference between an issue and a project on the same subject.
- **Do not repeat properties.** No type prefix, no priority, no team, no status, no `[Bug]`, no quarter.
- **No dates in names.** Start and target dates are properties, and they carry their own precision, down to a quarter or half-year when the day is not known.
- **Sentence case. No trailing full stop. No em dashes.**
- **Prefer the concrete noun to the category noun.** "Enterprise SSO" beats "authentication" beats "auth".
- **Length is a consequence, not a target.** A title long enough to be specific is better than a short one that could mean four things.

## Form by object

| Object | Form | Good | Bad |
| --- | --- | --- | --- |
| Initiative | Durable outcome, no dates | `Make enterprise sign-in dependable` | `H2 Auth Strategy` |
| Project | An outcome that can become true, usually verb first | `Reduce failed enterprise SSO logins` | `Q3 Auth Project` |
| Milestone | A declarative state, past participle preferred | `All existing tenants migrated` | `Backend` |
| Issue | Imperative verb phrase, one outcome | `Add schema validation to skill generation` | `Fix auth` |
| Bug issue | Declarative statement of the wrong behaviour | `SSO login fails for accounts with two identity providers` | `Fix SSO bug` |
| Parent issue | Imperative, naming the whole outcome | `Migrate tenants to the new token store` | `SSO work` |
| Sub-issue | Imperative, readable without the parent | `Backfill tokens for tenants created before 2025` | `Step 3` |
| Document | Noun phrase naming what it holds | `Token exchange contract` | `SSO doc v2` |
| View | The question, stated as a condition | `Blocked, no owner` | `View 1` |
| Label group | Singular noun naming the dimension | `Source` | `Types` |
| Label | Singular noun naming one value | `Source/Customer` | `type:customer-reported-bug` |
| Status | Two words at most, naming a present state | `In review` | `Waiting on Soheil` |
| Template | The situation it serves | `Bug report` | `Standard issue template v2` |

A bug title is declarative rather than imperative because the fix is not known when the title is written. `Fix the token refresh race` presumes a cause. If the cause turns out to be something else, the title is now a lie that survives in search forever.

## The rewrite ladder

Apply in order and stop at the object's altitude. Issues usually stop at step 3, projects at step 5, initiatives one step above.

1. **Topic**: `Auth`
2. **Topic with a verb**: `Fix auth`
3. **Bounded change**: `Fix SSO login failures`
4. **Outcome with direction**: `Reduce SSO login failures`
5. **Outcome with population**: `Reduce failed enterprise SSO logins`

**The ladder is a diagnostic, not a generator.** It tells you which rung a weak title is stuck on. It does not license climbing past what you actually know.

It goes wrong in two ways. It smuggles claims: "users seem confused setting up SSO" becomes `Reduce failed SSO setups`, which asserts that setups fail, which nobody said. Climb only as far as the evidence reaches, and leave the rest to the description.

It also breeds a house dialect, where every project is `Reduce X for Y` until the roadmap reads as though one person wrote it in an afternoon. A plain statement of the outcome is often better. A title that already names its outcome is finished, whichever rung it sits on.

Improve a title without being asked when it is vague, unbounded, redundant with a property, prefixed, or a bare noun. Say what you changed and why in one line. Do not rewrite a title that is already specific just because you would have phrased it differently.

## Worked judgements

| Candidate | Verdict |
| --- | --- |
| `Auth` | Fails at every altitude. A topic has no end state, so nothing can ever be true of it. |
| `Fix auth` | Fails. A verb attached to a topic. Nothing can be checked against it. |
| `Authentication improvements` | Fails as a project. "Improvements" is a direction, not a destination, so the project can never close. This is a view. |
| `Reduce failed enterprise SSO logins` | Good project. Direction, population, and an observable phenomenon. The baseline and target go in the summary, not the title, because they change. |
| `Q3 Auth Project` | Fails. A date, a type, and a topic. Nothing about the intended change survives. |
| `Implement OAuth token cache` | Good issue title. Bad project name, because a mechanism can be delivered in full while the problem remains. |

## When an implementation title is correct

When the object exists to deliver a mechanism, step 3 of the ladder is the top of it, and climbing further invents a benefit.

The rule is that the title names the outcome at the object's altitude. When the technical artefact **is** the outcome, an implementation title is the outcome title. These are correct as written:

- `Add schema validation to skill generation`
- `Split the run orchestrator into planning and execution`
- `Move the schema source of truth to Drizzle`
- `Publish the dbt template`

Each names one reviewable result, and no product-level rewrite would be more honest. Do not translate technical investment into invented user benefit. `Improve reliability for users` is worse than `Split the run orchestrator into planning and execution`, because it claims an effect that has not been measured.

The distinction is mechanism against result, not technical against non-technical. A mechanism title is wrong when the object exists to solve a problem the mechanism might not solve. It is right when the object exists to deliver the mechanism.
