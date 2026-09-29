# Where documentation belongs

One authoritative home per fact. Everywhere else summarises and links.

## What a repository is allowed to hold

A repository holds `README`, `CONTRIBUTING`, the agent instruction file, decision records,
and skills. Product documentation as well, when it is genuinely repository-local. Nothing
else.

The rule that follows from that, and the one most often broken: **when product
documentation lives externally, the repository gets no Markdown beyond that set.** A
repository-local copy of external documentation is a fork of it, and the copy nobody
publishes is the one that goes stale.

A document that is neither product documentation nor one of those five is a signal, not a
file. Ask what it actually holds:

| It holds | It belongs |
| --- | --- |
| Setup, commands, environment variables, a runbook | `CONTRIBUTING` |
| Why a skill works the way it does | That skill |
| Why the system is shaped this way | A decision record |
| Findings from a one-time investigation | The issue or tracker item that asked |
| Work nobody has done yet | The tracker |
| A thought that is not yet work | The ideation vault, outside any repository |
| What agents need on most tasks | The agent instruction file |

Applying this deletes documents. Route the substance first, then delete, and say what went
where. Deleting before routing loses information that exists nowhere else, which the
"move, do not delete" rule below forbids.

## The decision

```
Is it needed to decide whether to use the project?
├─ yes → README first screen
└─ no
   Is it needed to get a first result?
   ├─ yes → README quick start, shortest working path only
   └─ no
      Does it change with the code, revision by revision?
      ├─ yes → docs/ in this repository, reviewed in the same pull request
      └─ no
         Does the forge have a semantic file for it?
         ├─ yes → that file: SECURITY, SUPPORT, LICENSE, CODE_OF_CONDUCT, CITATION
         └─ no
            Can it be generated from source?
            ├─ yes → generate it and link it, never hand-maintain it
            └─ no
               Is the audience end users across several repositories?
               ├─ yes → an external documentation site
               └─ no → docs/ in this repository
```

## Homes

| Information | Authoritative home |
| --- | --- |
| What this is, who it is for | README first screen |
| Category and discovery terms | Repository description and topics |
| Shortest path to a first result | README quick start |
| Full setup, commands, environment variables | A getting-started document under `docs/` |
| Architectural decisions and their rationale | The decision log, conventionally `docs/adr/` |
| Operational procedures and runbooks | `CONTRIBUTING.md` |
| How work is routed between the tracker, issues, pull requests and records | The `work-orchestration` skill |
| Contribution workflow | `CONTRIBUTING.md` |
| Vulnerability reporting | `SECURITY.md` |
| Support channels | `SUPPORT.md` |
| Licence terms | `LICENSE` |
| Behaviour expectations | `CODE_OF_CONDUCT.md` |
| Release history | The forge's releases, or `CHANGELOG.md` |
| API, CLI, or SDK reference | Generated, or an external documentation site |
| Agent operating instructions | The canonical skill store |
| Which artefacts a piece of work needs | The `work-orchestration` skill |
| A thought that is not yet work | The ideation vault, outside any repository |
| Findings from a one-time investigation | The issue that asked, pinned to its commit |

## Repository-local or external

**Local wins** when the content must version with the code, describes the local build,
records a decision, is generated from source, or should be reviewed in the same pull
request as the change it describes.

**External wins** when the audience is end users, the content spans several repositories,
it needs search and navigation of its own, or it has an independent release cycle.

## Rules

**Never fork instructions.** A second copy of a setup sequence is a future contradiction,
and the copy nobody runs is the one that goes stale.

**The test.** For any instruction, ask: *if this changes tomorrow, how many places must be
edited?* One is correct. Two is a defect waiting to surface. More than two means nobody
will find them all, and the repository will start contradicting itself.

**Summarise and link.** A summary says what the destination contains and when to use it,
in one line. That is enough context to decide whether to follow the link.

**Name the destination in the link text.** `Production deployment guide`, not `here`,
`docs`, or `more info`.

**Move, do not delete.** When content leaves a file, it goes somewhere. If there is no
destination yet, say so and propose one rather than dropping information that exists
nowhere else.

**Fix the authoritative copy.** When a summary disagrees with its source, correct the
source or the link, not the summary into a third version.
