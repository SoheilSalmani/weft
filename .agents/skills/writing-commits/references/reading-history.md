# Reading history

Writing good history and reading it are the same model from two sides. The commands below are what the messages are written for, and knowing them is what makes it obvious which messages were worth writing.

## Contents

- When to look
- The commands
- What good history gives back

## When to look

Consult history before changing code when the code is **surprising**: a guard that looks unnecessary, a value that is set twice, an ordering that looks arbitrary, a workaround with no comment. Those are the shapes that usually have a reason, and the reason is usually in a commit body.

Do not perform archaeology for trivial changes. Adding a field, renaming a local, or writing a new function needs no history lookup, and running one wastes the turn.

The trigger is not the size of the change. It is whether you are about to remove or invert something someone deliberately put there.

## The commands

```bash
git log --oneline -20 -- path/to/file      # what has happened here
git log -p -- path/to/file                 # with diffs, when the change is small
git blame -w -M path/to/file               # ignore whitespace, detect lines moved
                                           # or copied within the file
git blame -w -M -C path/to/file            # also detect lines moved from other
                                           # files changed in the same commit
git show <sha>                             # the full commit behind a blame line
git log -S'someSymbol' --oneline           # pickaxe: commits that added or removed
                                           # an occurrence of the string
git log -G'regex' --oneline                # commits whose diff matches a pattern
git log --follow -- path/to/file           # history across renames
```

`git blame -w -M` matters more than plain `blame`. Without those flags a reformat or a block moved within the file makes lines look last-changed by whoever moved them, which sends you to the wrong commit. Add `-C` when code was moved between files in the same commit, which is common after an extraction.

For a regression, `git bisect` locates the commit that introduced it. It only works if commits build, which is why build integrity is a rule in `cohesion.md` rather than a preference.

## What good history gives back

When you land on a commit through blame and its message says *why the obvious fix fails*, you have your answer in one step. When it says `Fix bug`, you now have to reconstruct the reasoning from the diff, the surrounding code, and whatever the tracker still holds.

That asymmetry is the entire argument for commit bodies, and it is worth remembering while writing one: the reader will arrive here through a line of code, not through the branch that produced it.
