# Screening

Cheapest eliminations first. Trademark and international screening are the expensive stages and must never run on a candidate that failed a free test.

## Depth tiers

Match ceremony to stakes.

| Tier | For | Screens |
| --- | --- | --- |
| **Lightweight** | Internal tool, small utility | Web search, GitHub, one registry, optional domain |
| **Standard** | Public tool or product | The above plus domains, main registries, USPTO and EUIPO sanity check, linguistic quality |
| **Deep** | Company or commercial platform | The above plus target-jurisdiction trademark screening, international linguistic screening, domain history, and a recommendation to obtain professional clearance |

Do not put a fifty-line utility through deep clearance.

## Collision severity

Graded by prominence and category adjacency, never by exact match.

| Severity | Looks like |
| --- | --- |
| **Critical** | Famous brand, or any name where users would assume affiliation |
| **High** | Active project in the same category with real adoption |
| **Moderate** | Active project, different category |
| **Low** | Obscure or abandoned |
| **Negligible** | Incidental, unrelated field |

A 50k-star project with the same name kills a developer-tool candidate regardless of trademark status. A three-star abandoned repository does not.

## Search ownership

Distinct from SEO, and where many otherwise excellent names die.

Run the realistic query set: `<name>`, `<name> cli`, `<name> github`, `<name> docs`. If the name needs a descriptor in every query to surface, that is a permanent cost. Sometimes acceptable, always deliberate.

Zero search results is not required. A distinctive arbitrary word can coexist with its dictionary meaning. Domination by a famous entity is the disqualifier.

## Trademark

Search exact, close spellings and phonetic variants, in the classes that match the actual goods and services. Do not sweep unrelated classes, and do not treat class difference as safety: likelihood of confusion turns on sound, appearance, meaning **and** relatedness of goods.

Current authoritative tools: **USPTO Trademark Search at `tmsearch.uspto.gov`** (TESS is retired), WIPO Global Brand Database, and EUIPO or TMview for the EU. Choose jurisdictions from project context: where customers are, where the entity is incorporated, where distribution happens, whether ambition is international. Never from the user's nationality.

USPTO publishes dedicated guidance on why searching for **similar** marks matters. Exact-match-only screening is not screening.

Five verdicts, never a binary:

- no obvious conflict found in the screened classes and jurisdictions
- potential conflict, relatedness unclear
- material conflict in a related class, or a famous mark
- requires professional clearance
- screening unavailable

**Never "legally safe" and never "trademark available."** State the disclaimer once per session.

## Domains

No blind `.com` requirement. Rank by archetype: `.dev`, `.sh`, `.io` are credible for developer tools; `.com` carries extra weight for a commercial platform. But **exact `.com` held by a famous adjacent company is a serious collision signal**, which is different from it merely being taken.

Five states, not two: registered, unregistered, reserved, premium, unclear.

`scripts/availability.mjs` resolves RDAP servers through the IANA bootstrap at `data.iana.org/rdap/dns.json`. Hand-written endpoints fail silently, and a failed request is not availability. HTTP 200 is registered with dates; 404 is no registration record, which registry policy can still override.

**DNS is not an availability signal.** A registered domain may have no A record and a free one may resolve through a wildcard.

Prefix domains (`get-`, `use-`, `-hq`) are a last resort. Hyphens and domain hacks are discouraged.

For deep finalists, check history: prior malware, scam, adult, gambling or a failed company attached to the domain. Report material risk only; do not surface every obscure trace.

## Registries

Query only what the project ships to.

Two normalisation behaviours that catch people, both verified against the live services:

- **PyPI** collapses case and separators per PEP 503, so `Flask-SQLAlchemy`, `flask_sqlalchemy` and `FLASK-SQLALCHEMY` are one project.
- **crates.io** collapses `-` and `_`, and returns **403 to any request without a User-Agent**, for free and taken names alike. Reading that 403 as "taken" is a false positive; reading it as "free" is worse.

npm requires lowercase for new packages. Scoped names are a legitimate fallback when the bare name is taken.

Typosquat adjacency to a popular package is a hazard even when the exact name is free.

## GitHub

Repository names are unique per owner, so **"I can create the repo" is not availability.** What matters is prominence in search: same-category projects, activity, and whether a user searching the name would find the wrong thing. Stars are one signal, not the criterion.

## International

Scope to markets that matter. Check negative meanings, profanity, cultural sensitivity, transliteration and pronunciation drift. **Report which languages were checked** and never claim worldwide clearance from a shallow pass.

An unfortunate meaning in a language nobody in the market speaks is a note, not a rejection.

## When a source fails

Continue the other checks and report the gap. A screen that did not run is reported as not run.
