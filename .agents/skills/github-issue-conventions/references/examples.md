# Worked examples

Weak version first, then the rewrite, then the rule it teaches.

## Contents

- The issue that should not exist
- A known bug
- An intermittent bug
- A refactor with no stated value
- An issue drowning in logs
- A speculative plan presented as a requirement
- An investigation
- A migration
- An issue that is already fine

## The issue that should not exist

**Request**: "File an issue to rename `AGENT_TIMEOUT_MS` to `RUN_TIMEOUT_MS`, it is used in three places."

**Right answer**: no issue.

> This does not need an issue. It is one pull request, nothing about it needs to survive that pull request, and the diff shows the change better than a description would. Say the word if you want it tracked anyway.

Filing work nobody will read costs more than it saves. The decision to create nothing is a real output, not a refusal.

## A known bug

**Weak**

> **Title**: Fix payment bug
>
> ## Overview
> There is a bug in the payment flow.
>
> ## Background
> Users are experiencing issues.
>
> ## Acceptance criteria
> - [ ] Bug is fixed
> - [ ] Tests pass

**Better**

> **Title**: Retry after a failed charge submits the payment twice
>
> Clicking Retry after a declined charge sends a second authorisation before the first has settled, so the customer is charged twice.
>
> `Hypothesis`: the retry handler does not carry the original idempotency key. Confirmed only that both requests reach the processor with different keys.
>
> Done when a retry reuses the original key and a duplicate submission is rejected at the gateway, covered by a test that replays a retry within the settlement window.
>
> Linear: ENG-41

Three headings holding no information became five lines holding all of it. The suspected cause is marked, so nobody builds on it as fact.

## An intermittent bug

**Better**

> **Title**: Webhook deliveries occasionally processed twice under concurrent retries
>
> Two production deliveries in the last week produced duplicate downstream records. Not reliably reproducible; both occurred while the sender was retrying a timed-out request.
>
> `Hypothesis`: two workers pass the dedupe check before either writes. Confirming would need a concurrent replay against staging, which does not exist yet.
>
> `Unknown`: how often this happens. There is no metric on duplicate downstream records, and adding one may be the first task.
>
> <details><summary>Both delivery IDs and their timestamps</summary>
>
> ```
> ...
> ```
> </details>

"Not reliably reproducible" is stated as a fact rather than apologised for, the frequency is an explicit unknown rather than an invented rate, and the evidence is present without dominating the issue.

## A refactor with no stated value

**Weak**: `Refactor the auth module`

**Better**: `Extract token validation so the mobile client can stop importing the web session module`

> The mobile client imports `web/session` only to reach `validateToken`, which pulls the cookie parser and the CSRF middleware into a build that uses neither.
>
> Done when the mobile client imports token validation without any web session code, and the bundle no longer contains the CSRF middleware.

A refactor issue must name what the current structure prevents. Without that, nothing can ever make it urgent, and it sits open forever.

## An issue drowning in logs

**Weak**: 2,000 words of stack traces, a full query plan, and three failed attempts pasted in sequence.

**Better**: the one frame that identifies the failure, in the body. Everything else in a collapsed block or a comment, with a line saying what it is.

> Connection pool exhausts under sustained load; the first failure is always `pool timeout after 30000ms` in `db/pool.ts`.
>
> <details><summary>Full trace and query plan from 2026-08-11</summary>...</details>

Nothing is deleted. The evidence stops being the issue and goes back to being evidence.

## A speculative plan presented as a requirement

**Weak**

> Add a Redis-backed cache in `src/cache/redis.ts`. Create a `CacheClient` class with `get`, `set`, and `invalidate`. Wire it into the invoice service and add a TTL of 300 seconds.

**Better**

> Invoice generation recomputes the same tax tables on every request, which dominates p95 at 2.4s measured in staging on 2026-08-11.
>
> `Must`: cached values cannot outlive a tax-table change, which happens on deploy.
>
> `Suggested`: a Redis-backed cache keyed by table version. Not verified against the current invoice code, so discard it if the code suggests better.
>
> Done when p95 for invoice generation is under 500ms in staging with the same request mix, and a tax-table change is reflected immediately.

The first version fixed a class name, a file path, and a TTL before anyone read the code. The second states the constraint that actually matters and marks the approach as discardable.

## An investigation

**Better**

> **Title**: Can the legacy session-token path be removed without breaking mobile clients?
>
> The legacy path is maintained on every auth change and we do not know whether anything still uses it.
>
> This unblocks the decision on whether to remove it, which is currently blocking the token refactor.
>
> `Unknown`: whether mobile builds older than 4.2 still authenticate through it, and how many such installs are active.
>
> Output: a comment stating what still uses the path and a recommendation. If the answer is "nothing", the removal is a separate issue.

The deliverable is knowledge. The issue closes on an answer, and it says explicitly that implementation would be separate work, so it cannot silently become a commitment.

## A migration

**Better**

> **Title**: Move all skill records from `SCHEMA_SQL` provisioning to Drizzle migrations
>
> Two schema definitions exist with no enforcement that they agree, so a change to one silently diverges.
>
> `Decided`: Drizzle is the source of truth. See `docs/adr/0003-schema-source-of-truth.md`.
>
> `Must`: existing databases keep working through the transition, since there is no downtime window.
>
> Sequence: generate a baseline migration matching current production, switch provisioning, then delete `SCHEMA_SQL`.
>
> Done when no code path reads `SCHEMA_SQL` and a fresh database provisions from migrations alone.
>
> Rollback: provisioning can fall back to `SCHEMA_SQL` until the final step lands.

Source, target, compatibility, sequencing, rollback, and a completion criterion that is a query rather than a feeling.

## An issue that is already fine

**Given**

> **Title**: Reject expired refresh tokens at the gateway
>
> Expired refresh tokens currently reach the auth service, which returns 500 rather than 401. The gateway already parses the token, so it can reject them before the hop.
>
> Done when an expired refresh token returns 401 from the gateway and the auth service records no request.

**Right answer**: change nothing.

> This one is already good. Objective, reason, and a completion condition that can be checked, in three lines.

An issue-writing skill that always rewrites is worse than none. If the only change available is one you would have phrased differently, there is no finding.
