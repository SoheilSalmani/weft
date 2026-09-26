#!/usr/bin/env python3
"""PreToolUse hook for ExitPlanMode: require a plan to walk the artefact roster.

Every plan states a verdict for all six systems, including the ones that do not
change, because a system never considered and one deliberately skipped look
identical once the plan is written.

ExitPlanMode accepts no plan content, so the plan cannot be read from tool_input.
It is read from the newest file in ~/.claude/plans/ instead, which is the active
plan because it was just written.

Fails open on anything unexpected. A hook that cannot read the plan must never be
the reason plan mode stops working.
"""

import os
import re
import sys
import time

PLANS = os.path.expanduser("~/.claude/plans")
MAX_AGE = 6 * 60 * 60

# Docs aliases are wide on purpose: the row covers README, CONTRIBUTING, the
# community health files and product docs, so "Docs / README" still matches.
ROSTER = [
    ("Linear", r"linear|trackers?"),
    ("GitHub issue", r"github|issues?"),
    ("PR / commits", r"pull requests?|prs?|commits?"),
    ("ADR", r"adrs?|decision records?"),
    ("Docs", r"docs|documentation|readmes?|contributing"),
    ("Ideas", r"ideas?|gardens?"),
]

# A table row: "| Linear | ENG-214 |" - the verdict cell must not be empty.
TABLE = r"^\|?[ \t]*(?:{})\b[^|\n]*\|[ \t]*[^|\s]"
# A bullet: "- Linear: none - no product story" - so a short plan need not draw a table.
BULLET = r"^[-*][ \t]*(?:{})\b.*?[:—-][ \t]*\S"

REMINDER = """\
This plan does not walk the artefact roster.

Use the `work-orchestration` skill to decide, then state a table before the first
implementation step with a verdict for all six systems: Linear, GitHub issue,
PR / commits, ADR, Docs, Ideas.

`none` is a verdict rather than a gap, and it carries its reason. Docs covers every
documentation surface: README, CONTRIBUTING, community health files and any docs site.

Missing rows: {missing}"""


def newest_plan():
    entries = []
    for name in os.listdir(PLANS):
        if not name.endswith(".md"):
            continue
        path = os.path.join(PLANS, name)
        try:
            entries.append((os.path.getmtime(path), path))
        except OSError:
            continue
    if not entries:
        return None
    mtime, path = max(entries)
    if time.time() - mtime > MAX_AGE:
        return None
    return path


def missing_rows(plan):
    missing = []
    for label, aliases in ROSTER:
        patterns = (TABLE.format(aliases), BULLET.format(aliases))
        if not any(re.search(p, plan, re.IGNORECASE | re.MULTILINE) for p in patterns):
            missing.append(label)
    return missing


def main():
    try:
        sys.stdin.read()
    except Exception:
        pass

    try:
        path = newest_plan()
        if path is None:
            return 0
        with open(path, encoding="utf-8") as handle:
            plan = handle.read()
    except Exception:
        return 0

    try:
        missing = missing_rows(plan)
    except Exception:
        return 0

    if not missing:
        return 0

    sys.stderr.write(REMINDER.format(missing=", ".join(missing)) + "\n")
    return 2


if __name__ == "__main__":
    sys.exit(main())
