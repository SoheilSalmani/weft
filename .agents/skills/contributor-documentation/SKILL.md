---
name: contributor-documentation
description: Writes CONTRIBUTING and routes content to the community health files GitHub recognises, such as SECURITY, SUPPORT, LICENSE, and CODE_OF_CONDUCT. Keeps agent operating instructions out of human contributor documentation. Use when creating or improving contribution guidance, or when deciding which community file content belongs in.
---

# Contributor documentation

`CONTRIBUTING.md` exists to shorten one path: from *I want to contribute* to *I have a working environment and know how to submit good work.*

Everything that does not serve that path belongs somewhere else.

## What goes in directly

A contributor needs these immediately, so they are in the file rather than behind a link:

- Whether contributions are welcome, and what kind.
- Prerequisites, with versions.
- Setup, as commands that work.
- How to run tests, linting, and type checking.
- Where work is tracked, and whether to open an issue first.
- The branch and pull request workflow, in a few lines.

## What goes behind a link

Coding standards, architecture, decision records, the code of conduct, the release process, and detailed troubleshooting. Each gets one line saying what it covers and a descriptive link.

## The boundary with agent skills

Where a repository also holds agent skills or instruction files with detailed operating instructions, **none of that belongs in `CONTRIBUTING.md`.** A human contributor should not have to read an agent handbook to submit a fix.

> `CONTRIBUTING.md` states the externally observable contract, which is what a contributor must do. The skills hold the operational detail, which is how an agent does it.

So the file can say that commit subjects are imperative and name the change, and that a pull request explains what changed and how it was verified. It does not reproduce the message patterns, the cohesion tests, or the accuracy rules. One line may point at `.claude/skills/` for contributors who use Claude Code, and that is the whole relationship.

If a convention only exists to steer an agent, it is not a contributor convention and does not appear here.

## Community health files

GitHub recognises specific filenames and surfaces them in its own interface. Content in the wrong file is content the reader will not find.

| Content | File |
| --- | --- |
| How to report a vulnerability | `SECURITY.md`, never CONTRIBUTING. See the note below |
| Where to ask questions | `SUPPORT.md`, or a documented channel |
| Licence terms | `LICENSE`. Name it in the README, do not paraphrase it |
| Behaviour expectations | `CODE_OF_CONDUCT.md`, linked from CONTRIBUTING |
| How to cite the work | `CITATION.cff`, when academically relevant |
| Funding links | `.github/FUNDING.yml` |

Lookup order for each is the `.github` folder, then the repository root, then `docs`, and a repository's own file always beats an inherited default.

### Vulnerability reporting, specifically

Two mechanisms, and they are not the same thing. **Private vulnerability reporting** is a GitHub feature that lets anyone submit a report privately, and it is available **on public repositories** once an owner or administrator enables it. `SECURITY.md` is a file describing the policy. GitHub is explicit that private reporting "is separate from a repository's `SECURITY.md` file": reporters follow the file only when private reporting is not enabled.

So for a public repository, the strongest arrangement is to enable private reporting **and** keep a short `SECURITY.md` pointing at it. Never route vulnerabilities to public issues or pull requests.

For a private repository neither applies in the usual sense, and a named contact is the honest instruction.

Check before writing anything: `gh api repos/{owner}/{repo} --jq '{private, has_discussions}'`. Do not describe a channel the repository does not have.

**Account-level defaults.** A public repository named `.github` on the account supplies `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, `FUNDING.yml`, `GOVERNANCE.md`, `SECURITY.md`, `SUPPORT.md`, and issue and pull request templates to every repository on that account that lacks its own. For a maintainer with many public repositories that is one file set instead of many. **A licence cannot be defaulted** and has to exist in each repository.

## Duplication with the README

The README carries one inviting line and a link. This file carries the detail.

When setup commands appear in both, the README copy goes stale first and nothing catches it. One authoritative location, everywhere else summarises.

## Mode

A private or internal repository still benefits from this file, because the audience is a colleague or a future self. What changes is the framing: no invitation to outside contributors, no code of conduct obligation, and no funding links.

An archived or maintenance-only repository says so at the top, along with what it accepts, which is often only security fixes.

## Verify the commands

Every command in this file will be run by someone with no context. Read `package.json` and the actual scripts rather than writing from memory, and check that the setup sequence works from a clean checkout.

## Before you finish

- A newcomer could get a working environment from this file alone.
- No agent-only instruction leaked in.
- Security reporting is in `SECURITY.md`, not here.
- Nothing duplicates the README beyond a one-line invitation.
- Every command was verified against the repository.
- Every link says what is behind it and resolves.
