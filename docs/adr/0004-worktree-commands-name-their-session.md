---
status: Accepted
date: 2026-09-30
decision-makers: [Soheil Salmani]
---

# ADR-0004: Take a worktree command's session from where it runs or from its name

## Context

A template being edited holds any number of recording sessions. Each session has its own worktree and its own staging index, and a worktree carries a pointer back to its template and session, which the tool finds by walking up from the working directory. The session-management commands (new, adopt, list, path, move, scope, refresh and end) act on sessions as a whole. Five commands work inside one session's worktree: status, add, reset, diff and commit. Add and reset take path patterns.

Up to 2026-09-29 those five commands chose their session in this order: a session named on the command line, the session whose worktree contains the working directory, and otherwise the only session the template held. With several sessions and neither of the first two, they failed and listed the sessions. Path patterns were resolved against the working directory inside a worktree and against the worktree root everywhere else.

Running them at the template root with one session had three effects. Shell completion offered the template's own files and the paths under the sessions directory, while the command read every pattern relative to the worktree root, so a template file and a worktree file with the same name were confused. A completed path matched nothing, the command reported that nothing was staged and exited successfully, and the next commit recorded the whole worktree, because commit takes the whole worktree when nothing is staged. The same command stopped working as soon as a second session existed, for example when an agent's session sits beside the author's.

Git refuses the equivalent commands in a bare repository, and resolves paths against the worktree root when a work tree is named from outside it.

## Options considered

### A. Keep the fallback and refuse patterns that match nothing

Scripts and tests that name only the template keep working. A pattern typed at the template root still means a worktree path, and whether a command runs still depends on how many sessions exist.

### B. Refuse only where the template root is found from the working directory

The fallback remains when the template is named explicitly. One command then behaves differently with and without a template argument that names the directory it already runs in, and scripts that name only the template still stop working when another session appears.

### C. Take the session only from the working directory or an explicit name

The five worktree commands stop falling back to the only session. Scripts and tests that name only the template must add a session name.

## Decision

The system will take the session of status, add, reset, diff and commit from an explicit session name, or from the worktree that contains the working directory, and from nothing else (option C). When a session is named from outside its worktree, path patterns are relative to the worktree root. When no session can be taken, the command fails, lists the template's sessions with their worktrees, and names the command that enters one and the flag that names one.

The session-management commands keep falling back to the template's only session. They take no path patterns, and refreshing a session right after editing the manifest at the template root is their common use.

Option A was not chosen because it keeps a meaning for paths at the template root that differs from everywhere else, and keeps behaviour that changes with the number of sessions. Option B was not chosen because it gives one command two behaviours for the same template and leaves template-only scripts dependent on other sessions.

## Consequences

A worktree command's target is visible in its arguments or in its working directory, and a script that names its session keeps working while other sessions start and end. The template root behaves the same with one session as with several.

Every script, test and printed instruction that named only the template for these commands has to add a session name. The tool's own end-to-end tests needed that change in seventy-three places, and the authoring instructions the tool prints for agents now enter the worktree instead. The change is not backward compatible and has no deprecation period.

Status run outside a worktree now fails instead of reporting that the template has no session.

Two resolution rules exist, one for the worktree commands and one for the session-management commands. A new command has to be placed in one group or the other, and a command placed in the wrong group brings back the confusion this record removes.

Add and reset also refuse a pattern that matches no file, independently of this decision, so a pattern resolved against the wrong root fails instead of leaving the stage empty.
