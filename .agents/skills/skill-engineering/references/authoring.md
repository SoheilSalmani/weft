# Authoring a skill

## Contents

- Naming
- Layout
- Frontmatter
- Descriptions and triggering
- Structure budgets
- References and scripts
- The defect catalogue
- Evaluations

## Naming

The name is chosen before anything else, and it is what an agent matches against. Two families, used consistently here:

| Family | Shape | Answers | Examples |
| --- | --- | --- | --- |
| **Task** | verb-ing plus object | What will this do for me | `writing-commits`, `tracking-work-in-linear`, `triaging-github-issues` |
| **Domain** | noun phrase | What does this know about | `linear-conventions`, `agent-portability`, `work-orchestration` |

The test: does the skill **do** a recognisable task, or does it **know** a domain. A skill that does both is usually two skills.

- **Name the task, not the tool.** `writing-commits`, not `git-helper`. The exception is when the tool *is* the domain, as in `linear-conventions`.
- **Use the words a user would say out loud.** `writing-pull-requests`, not `pr-tool`. The name has to survive being spoken and searched.
- **Never encode the host, the file format, or the word "skill".** `claude-commit-skill` is wrong three times over: it will outlive the host, and every skill is a skill.
- Mechanical constraints are in the frontmatter table below, and the name must match its directory.

## Layout

**Skills are flat.** One directory per skill, directly under the canonical store, with no category folders.

The specification is silent on nesting, which means host behaviour is unspecified rather than permitted. A host or validator that reads the store one level deep sees a category folder as a skill with no `SKILL.md`. And the agent never sees the path anyway: it routes on the description, so folders buy nothing for discovery. Verb-first names already cluster in a listing, and they keep clustering in the flat list an agent is shown.

If the count ever makes browsing painful, verify nesting against every target host and every tool that reads the store first. Do not nest on the assumption that it works.

## Frontmatter

The specification defines exactly six fields. A canonical skill uses only these.

| Field | Required | Constraint |
| --- | --- | --- |
| `name` | Yes | 1-64 chars, lowercase letters, digits, hyphens. No leading, trailing, or doubled hyphen. **Must match the directory name** |
| `description` | Yes | 1-1024 chars. What it does and when to use it |
| `license` | No | Name, or a reference to a bundled file |
| `compatibility` | No | ≤500 chars. Environment requirements. Most skills need none |
| `metadata` | No | String-to-string map for client-specific properties |
| `allowed-tools` | No | Space-separated tool list. **Experimental**, support varies |

Treat `allowed-tools` as declaring intent, never as a security control.

Host extensions, which must not carry semantics: `when_to_use`, `argument-hint`, `arguments`, `disable-model-invocation`, `user-invocable`, `disallowed-tools`, `context` (Claude Code); `paths` (Cursor). A non-spec field causes a hard error when a skill is packaged or uploaded outside its host, so a leaked field breaks the skill rather than degrading it.

`disable-model-invocation` is documented by two vendors, which makes it the most portable of the extensions, but it is still an extension.

## Descriptions and triggering

The description is the routing interface. At startup a host loads only names and descriptions, so this string decides whether the skill is ever seen.

It must say **what the skill does and when to use it**. A description that only describes will not trigger.

Front-load the distinguishing case, because descriptions get truncated in large listings. Where two skills could both match, name the boundary in the description itself: *"For X, use other-skill instead."*

Write in the third person. Include the words a person would actually type.

| Weak | Why | Better |
| --- | --- | --- |
| `Helps with databases` | No task, no trigger | `Reviews database migrations for lock risk, rollback safety, and backward compatibility. Use when a change adds or alters a migration.` |
| `Database migration skill` | Names itself, not the task | as above |
| `Use this for all database work` | Triggers on everything | Narrow to the class of task it actually handles |

### The three corpora

A skill is unfinished without all three.

- **Positive**: requests that must activate it, in varied phrasings, including indirect ones.
- **Negative**: adjacent requests that must not. This is the corpus people skip, and it is where trigger collisions surface.
- **Boundary**: genuinely ambiguous requests, where either answer is defensible and the skill should say which it chose and why.

## Structure budgets

- `SKILL.md` under 500 lines; instructions under about 5000 tokens.
- Metadata is roughly 100 tokens, loaded always. Everything else loads on demand.
- References **one level deep** from `SKILL.md`. Never a reference that points to another reference.
- Every reference is pointed at from `SKILL.md`, and every pointer resolves. An unreachable reference is dead weight that still costs review attention.

## References and scripts

A reference earns its place when the content is needed on some invocations and not most: catalogues, tables, worked examples, edge cases. It does not earn its place by existing.

Give each reference a stated loading purpose in `SKILL.md`, so the agent knows when to open it: *"Read `x.md` when the call is not obvious."*

A script earns its place when the operation is deterministic, repeated, and its output is checkable. Weigh it against the 2.12x vulnerability multiplier, keep it dependency-light, and make its failure modes explicit.

`assets/` is for artefacts the skill actually produces from, such as templates. Not for decoration.

## The defect catalogue

Check a draft against these. Research on real corpora found over 99% of skills carry at least one, and that they persist once introduced.

| Defect | Detection |
| --- | --- |
| Should have been AGENTS.md | It is relevant on most tasks |
| Description without a trigger | It says what, never when |
| Trigger collision | Two skills' descriptions both match the same request |
| Reference material in `SKILL.md` | The body carries tables or catalogues used occasionally |
| Orphan reference | Nothing in `SKILL.md` points at it |
| Reference chain | A reference sends the reader to another reference |
| Prose reimplementing logic | Deterministic steps written as instructions |
| Extension carrying semantics | Removing a host field breaks the skill |
| No negative triggers | Nothing says when not to load it |
| No evaluations | Nothing defines success |
| Invented authority | Claims about a platform that were not verified |

## Evaluations

`evals/evals.json`, in the format the ecosystem tooling expects:

```json
{
  "skill_name": "example",
  "evals": [
    {
      "id": 1,
      "prompt": "a realistic request in the user's own words",
      "expected_output": "what success looks like",
      "assertions": ["objectively checkable statement", "..."]
    }
  ]
}
```

Assertions must be checkable by reading the output. Cover the no-op case and the already-good-input case explicitly, because those are the two a skill most often fails: doing something when nothing was needed, and rewriting something that was already fine.
