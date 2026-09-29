# Does this need a skill?

## Contents

- The decision tree
- The two traps
- Worked cases
- When research is required

## The decision tree

```
Is it relevant on most tasks in this repository?
├─ yes → AGENTS.md, kept short. A scoped AGENTS.md for one directory
└─ no
   Is it deterministic, and must it behave identically every time?
   ├─ yes → a script. A skill only if choosing when to run it needs judgement
   └─ no
      Does it need access to an external system or API?
      ├─ yes → MCP server or real tool. Never prose pretending to have access
      └─ no
         Must it happen at a lifecycle moment regardless of what the agent decides?
         ├─ yes → a hook. Host-specific, and not portable
         └─ no
            Does it need isolated context or a different toolset?
            ├─ yes → subagent or custom agent
            └─ no
               Is the audience human rather than agent?
               ├─ yes → README for visitors, CONTRIBUTING for contributors
               └─ no
                  Will it recur, on a recognisable class of task?
                  ├─ yes → an Agent Skill
                  └─ no → nothing
```

## The two traps

**The AGENTS.md trap.** Always-relevant guidance placed in a skill will not load when it is needed, because skills load on match. "Use tabs" and "British spelling" are not skills. If it applies to most tasks, it belongs in persistent context.

**The script trap.** A skill that describes deterministic logic in prose asks the model to re-derive it each time, and it will get it subtly wrong eventually. But a bundled script raises the measured vulnerability rate by 2.12 times, so the script must be worth it. The bar: the logic is exact, repeated, and its output is checkable.

## Worked cases

| Request | Answer |
| --- | --- |
| "Agents should use tabs, not spaces" | AGENTS.md. Always relevant |
| "Run the formatter before every commit" | A hook or CI. Not a skill; the agent should not be able to choose |
| "Review database migrations for lock risk and rollback" | **Skill.** Recurring, needs judgement, has negative cases |
| "Create a Jira ticket from this" | Skill **plus** an MCP server or CLI. A skill alone would be prose pretending to have API access |
| "Rename this variable" | Nothing. One-off |
| "Validate our SKILL.md frontmatter" | A script. Deterministic, exact, checkable |
| "Explain our release process" | Skill if it is a procedure with judgement. CONTRIBUTING if contributors need it. Both, split by audience, if both are true |
| "Our lint config bans default exports" | AGENTS.md if agents need to know, and the linter already enforces it |
| "Investigate why staging is slow" | Nothing persistent. This is a task, not a capability |

## When research is required

Skip external research when the domain is entirely local and already understood: this repository's conventions, its file layout, its own commands.

Do the research when the skill depends on anything that changes underneath it: a product's current behaviour, an API surface, a specification, a standard, a regulation, or an ecosystem's terminology. A skill built on remembered platform behaviour will encode something that was true once.

The test: **would this skill be wrong if the vendor shipped a release next week?** If yes, verify against current documentation before writing it, and record what you checked and when.
