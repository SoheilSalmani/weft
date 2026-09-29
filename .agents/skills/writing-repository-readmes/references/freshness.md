# Verifying before writing

A README is the most-read and least-tested file in a repository. Everything in it drifts, and nothing fails loudly when it does.

## Contents

- Commands
- Versions and requirements
- Links
- Claims about behaviour
- What to do when something is wrong

## Commands

Read the scripts rather than recalling them:

```bash
python3 -c "import json;print(json.load(open('package.json')).get('scripts'))"
cat README.md | grep -nE '^\s*(npm|pnpm|yarn|npx|docker|make|cargo|go|uv|pip) '
```

Then check each command in the README exists as written. A script that was renamed, a flag that was dropped, or a package manager that changed leaves a command that fails on the reader's first attempt, which is the worst possible moment.

For a command-line tool, run `--help` and compare rather than trusting the prose.

## Versions and requirements

Check the stated runtime and package-manager versions against `package.json` fields such as `engines` and `packageManager`, the lockfile, and any version file. A README claiming Node 20 while the repository pins Node 24 sends people to an error they cannot diagnose.

Do not state a supported-version range that nothing tests.

## Links

Relative links are rewritten by GitHub to the current branch, so they keep working for people who clone the repository. Prefer them for anything inside the repository.

Check that every relative target exists:

```bash
grep -oE '\]\(([^)]+\.md[^)]*)\)' README.md | sed -E 's/^\]\(//; s/\)$//' | while read -r p; do
  [ -f "${p%%#*}" ] || echo "MISSING: $p"
done
```

For external links, prefer a stable canonical URL over a deep link that a documentation site will reorganise. When the external documentation is versioned, check that the version it describes matches the repository, and pin the URL if it does not.

## Claims about behaviour

Anything the README asserts about what the software does should be checkable against the code, the tests, or the configuration. Defaults, model names, port numbers, file locations, and environment variable names all drift.

Where a claim cannot be verified, either verify it or remove it. Do not carry it forward because it was already there.

## When the authoritative source has disappeared

An external documentation site that has gone means the README is now pointing readers at nothing, which is worse than having no link.

Three outcomes, in order of preference:

1. **Find where it moved.** Sites reorganise more often than they vanish. Check the project's current domain, its repository, and its package page before concluding it is gone.
2. **Restore the necessary content locally**, in `docs/`, if the project still needs it and nothing else holds it. This is a real cost and should be a deliberate decision, not a reflex.
3. **Remove the link**, and the sentence that depended on it, if the content is genuinely obsolete.

Never guess a replacement URL, and never leave a dead link in place because removing it would leave a gap. A gap is honest; a link that 404s tells the reader the project is unmaintained.

## What to do when something is wrong

Fix it, and say what you fixed. Do not preserve an incorrect statement for historical continuity: Git already holds every previous version of the file, and a reader has no way to know which sentence stopped being true.

When the authoritative copy lives elsewhere, such as a documentation site, correct the authoritative copy or the link rather than editing the local summary into disagreement with it.
