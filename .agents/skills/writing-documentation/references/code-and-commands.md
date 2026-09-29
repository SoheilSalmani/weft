# Code, commands, and output

In developer documentation the code is often most of the page, and it is the part the reader copies. These rules apply in every mode.

Drawn from the Google developer documentation style guide, the Kubernetes and MDN code style guides, and DigitalOcean's technical writing guidelines.

## Names and values

**Use the values a real user would type.** A template called `python-service`, a project called `my-shop`, a customer called `Acme Shop`. The reader should recognise their own work in the example. `tpl`, `proj`, `cfg` and `svc` make them translate every line before they can read it.

**Spell words out.** An abbreviation saves the writer a few keystrokes and costs every reader a moment. Keep only the abbreviations the product itself uses.

**One name per thing.** When a directory and the thing inside it can share a name, they do. A directory called `tpl` holding a template named `hello` makes the reader carry a mapping for the rest of the page.

**No throwaway values.** `x`, `foo` and `test` tell the reader the value does not matter. If any value would do, use the realistic one the page already uses.

**A placeholder is the exception, and must look like one.** A value the reader has to replace is written in capitals, such as `PROJECT_ID`, and the sentence after the block says what to put there. A value the reader types as it stands must look real. Each half needs the other: a placeholder that looks real gets pasted unchanged, and an example value that looks fake gets "corrected".

## Code in sentences

**Inline code is for names**: commands, identifiers, file names, paths, flags, and the short values a sentence mentions.

**Anything the reader has to type goes in a block.** A command or a line of a file written inside a sentence cannot be copied cleanly, and whitespace that matters, such as a leading tab, is invisible there.

**No escaped code in prose.** A JSON fragment full of `\"` inside a sentence is unreadable. Show the lines in a block, or say in words what they mean.

## Blocks

**Introduce every block with a sentence.** End it with a colon when the block follows directly. Two blocks never touch: without a sentence between them the reader cannot tell where one job ends and the next begins.

**One job per block**: commands to run, the contents of a file, or output to compare. Never a command and its output in the same block.

**Say where each command runs**, in the sentence, in the block title, or with a `cd`. A command run in the wrong directory fails with an error the page does not predict.

**Title a file block with its path.** The reader must never have to guess which file they are editing.

**Show a short file whole**, with the changed lines highlighted where the site supports it, so the reader can compare their file with the page. In a longer file, show the region around the change and mark what is left out with a comment in the file's own language, not an ellipsis.

**A block the reader has to scroll is doing several jobs.** Split it where the jobs change, and put a sentence between the pieces.

**Never show code as an image.** It cannot be copied, searched, or read aloud, and it is unreadable when zoomed.

## Files the reader writes

**The reader writes files in their editor.** Say which file to open and where the change goes, then show the block:

> Open `config/settings.toml` in your editor and add the following lines to the end:

Do not create or change those files with `printf`, `echo >>`, heredocs, or `sed -i`. A shell one-liner hides the edit, cannot be read at a glance, and is not how anyone edits a file they care about. Use a command only when it is what a real user would run: a generator, a package manager, `git`, or `cp` of a file they already have.

## Output

**Show output only when it earns its place**: the reader needs to confirm a step worked, to copy a value from it, or to notice something the page relies on next. Output that repeats the previous block, or confirms what the command plainly did, is noise.

**Introduce it and point at it.** `The output is similar to the following:`, then one sentence on what to notice.

**Paste it from a real run.** Trim it and mark the trimmed lines, but never type it from memory. An example that has drifted from the product is worse than no example, because a reader will trust it.

## Flags

**Show the command a person would type.** Flags that exist because no person is present, such as `--non-interactive`, `--quiet`, `--no-color` and `--no-input`, belong in scripts, in continuous integration, and in pages about automation. Everywhere else the reader copies them without knowing why.

**A flag that skips a prompt hides the prompt.** If the prompt teaches something, show it and say what to answer. Use a flag like `--yes` only where the prompt teaches nothing.

**Repeat a checking command only when the reader learns from it.** A check that proves the page is still true belongs in the test that runs the page.

## Language

- `Open app/config.toml in your editor and add the following lines to the end:`
- `In the my-shop directory, run the following command:`
- `The output is similar to the following:`
- `Notice that the second line names the file that changed.`
- `Replace PROJECT_ID with the ID of your project.`

## Detection tests

**Do two blocks touch?** Look for a closing fence followed by an opening one with nothing but blank lines between. Each is a missing sentence.

**Is a file written by the shell?** Search the blocks for `printf`, `echo >`, `cat <<`, `tee`, and `sed -i`. Each one is a file the reader should have written in their editor, unless the command is the point.

**Would a newcomer recognise every name?** Read the example names aloud. Any that needs expanding, such as `tpl`, `proj` or `wt`, is an abbreviation to spell out. Any that could be anything, such as `x`, is a throwaway to replace.

**Is every flag one the reader needs?** For each flag, ask what would happen without it in a terminal. If nothing would, it is there for a script.

**Is every output block introduced, and does the page use it?** Output nobody is told to look at can go.

**Does the reader always know where they are?** Follow the page and note the directory before each command. Any command whose directory you had to work out is missing a sentence or a title.
