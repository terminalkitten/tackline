# tackline

[Docs](https://terminalkitten.github.io/tackline/) ·
[crates.io](https://crates.io/crates/tackline) ·
[GitHub](https://github.com/terminalkitten/tackline)

> A tackline is the short length of line on a signal halyard that separates
> the flags. This tool hangs signals on lines of code for an agent to read.

A terminal app for steering coding agents. Browse your files or the git
diff, select a line or a span of code, and attach a prompt instruction to it.
Notes are grouped into named sessions, and a session compiles into one
Markdown prompt for Claude Code.

This is agent steering, not code review: notes are instructions and context
for an agent, not verdicts on the code.

![Files view: noted code tinted yellow, the note under the cursor in the bottom border](docs/site/img/files.svg)

## Install

```sh
cargo install tackline
```

This installs two commands that do the same thing: `tackline` and the shorter
`tack`. From a checkout, use `cargo install --path .`.

Run it inside a git repository (or any subdirectory of one). Outside a repo
the Files view still works, without the Diff view.

## Workflow

1. `tackline` opens on the Diff view if you have uncommitted changes,
   otherwise on the Files view.
2. Select code with `V` (lines), `v` (characters), Shift+arrows or a mouse
   drag, and press `c`. Write the instruction and press `Enter`.
   The first time, you name a session.
3. Press `3` to review the session: every note with its location, code and
   instruction, in prompt order (`J`/`K` reorder).
4. Press `y` to copy the compiled prompt and paste it into Claude Code, or
   skip the clipboard:

```sh
tackline export | claude -p
tackline export "Search speedup" > prompt.md
```

The compiled prompt looks like this:

````markdown
# Search speedup

1. `src/matcher.rs:3-7`
   ```rs
       for item in items {
           if item.contains(query) {
               out.push(item.clone());
           }
       }
   ```
   replace the loop with an iterator chain
````

Noted ("tacked") code is easy to spot: it's tinted light yellow, with `●`/`│`
markers in the gutter, and the bottom border of the code pane shows the start
of the note under the cursor. Press `Enter` there to read the whole note.

Notes follow the code: when lines are inserted above a note or the code moves
within the file, the note moves with it. If the annotated code is gone, the
note is marked stale (`⚠`) and left out of the prompt with a warning.

## Views

| | |
|---|---|
| ![Note popup](docs/site/img/note.svg) | ![Note viewer](docs/site/img/view-note.svg) |
| **Note popup**: location, code and target session (`Tab` cycles). | **Note viewer**: `Enter` on a noted line shows the whole note. |
| ![Diff view](docs/site/img/diff.svg) | ![Session view](docs/site/img/session.svg) |
| **Diff view**: notes attach to new-side lines. | **Session view**: the prompt, note by note, in order. |

## Keys

Everything is Ctrl or plain keys: macOS terminals don't pass Cmd shortcuts on
to terminal apps. Press `?` in the app for the full list.

| Key | Action |
|---|---|
| `1` / `2` / `3` | Files / Diff / Session view |
| `Ctrl+N` / `Ctrl+O` | New session / pick the active session |
| `Shift+Tab` | Switch to the next session |
| `Ctrl+P` / `Ctrl+F` | Find a file / search code in the repo |
| `Tab` | Switch between the list and the code |
| `j` `k` `h` `l`, arrows, `gg` `G`, `Ctrl+D` `Ctrl+U`, `0` `$` | Move |
| `/` then `n` / `N` | Search this file |
| `:<n>` | Go to line |
| `w` | Wrap long lines |
| `v` / `V` / Shift+arrows / drag | Select characters / lines |
| `c` | Note the selection (or the line); on a noted line, edit that note |
| `d` | Delete the note on this line (asks first) |
| `Enter` | Show the whole note on this line |
| `r` | Reload the diff |
| `y` (Session view) | Copy the compiled prompt |
| `e` / `d` / `J` `K` (Session view) | Edit / delete / reorder notes |
| `q` | Quit (everything is saved as you go) |
| Mouse | Click the tree to open files, click or drag in the code, scroll with the wheel |

In the note popup, `Enter` saves. For a newline, type `\` and press `Enter`,
or use `Shift+Enter` in terminals that report it (Ghostty, Kitty, WezTerm,
Alacritty, iTerm2 with "Report keys using CSI u").

## CLI

```
tackline [--base <rev>] [--theme <name>]
    Open the app. --base diffs against merge-base(HEAD, <rev>), e.g.
    --base upstream/main for a fork. --theme picks a syntax theme
    (default: base16-ocean.dark, or InspiredGitHub on light terminals).

tackline export [<session>]
    Print the compiled prompt (default: the active session).
    Warnings go to stderr.

tackline sessions
    List sessions with note counts; * marks the active one.
```

## Data

Sessions live in `.tackline/sessions/<name>.json` in the repo root, one
readable JSON file per session; `.tackline/state.json` remembers the active
session and the last file. Every change is saved immediately (atomic writes).
On first run tackline offers to add `.tackline/` to `.gitignore`.

## Terminal notes

- **Mouse**: tackline captures the mouse for clicks, drags and the wheel. To
  select terminal text instead, hold Option (iTerm2, Terminal.app) or Shift
  (most others) while dragging.
- **Clipboard over ssh / tmux**: `y` copies with OSC 52, so the prompt lands
  in your local clipboard. In tmux, enable `set -g set-clipboard on`.
- **Speed**: rendering is done by your terminal; GPU-accelerated terminals
  (Ghostty, WezTerm, Kitty, Alacritty, iTerm2) scroll smoothest.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test screenshots -- --ignored   # redraw docs/site/img/*.svg
```

### Releasing

Bump `version` in `Cargo.toml`, commit, then tag and push:

```sh
git tag -s v0.1.2 -m "tackline 0.1.2"
git push origin main v0.1.2
```

The tag starts `.github/workflows/release.yml`: it checks that the tag matches
`Cargo.toml` and runs the tests, publishes to crates.io (trusted publishing,
no stored token) and opens a draft GitHub release with the changes since the
previous tag. Preview those notes locally with `scripts/release-notes.sh v0.1.2`.

The usage docs are in [`docs/site/`](docs/site/index.md), built with
[dokk](https://github.com/terminalkitten/dokk):

```sh
dokk serve --root docs   # live preview
dokk build --root docs   # static site in docs/.dokk/build
``` Built on
[tui-realm](https://github.com/veeso/tui-realm), syntect with two-face, git2,
nucleo and ripgrep's libraries.
