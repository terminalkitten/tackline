---
title: Getting started
description: Install tackline and turn your first note into a prompt.
---

## Install

tackline is a Rust program. With a [Rust toolchain](https://rustup.rs) installed:

```sh
cargo install tackline
```

The crate is [tackline on crates.io](https://crates.io/crates/tackline).

That puts two commands on your `PATH` that do the same thing: `tackline`, and `tack` for short.
Every example in these docs works with either. From a checkout of the repository, use
`cargo install --path .` instead.

:::note
If another program already installed a `tack` command, cargo stops and says so. Remove that one,
or keep using `tackline`.
:::

## First run

Start it inside a git repository, from the root or any subdirectory:

```sh
cd ~/code/my-project
tackline
```

:::steps
1. **Pick a view.** tackline opens on the [Diff view](guides/diff.md) when you have uncommitted
   changes, otherwise on the Files view. Switch any time with :kbd[1] (Files), :kbd[2] (Diff) and
   :kbd[3] (Session).

2. **Answer the `.gitignore` question.** The first time, tackline asks whether to add `.tackline/`
   (where it keeps your sessions) to `.gitignore`. Press :kbd[y] to add it, or :kbd[n]; it won't
   ask again either way.

3. **Open a file.** Click it in the tree, or move with :kbd[j] and :kbd[k] and open folders and
   files with :kbd[Enter]. :kbd[Tab] moves between the tree and the code. :kbd[Ctrl+P] finds a file
   by name.

4. **Select some code.** Press :kbd[V] and move down with :kbd[j] to select whole lines, or hold
   :kbd[Shift] and use the arrow keys, or drag with the mouse.

5. **Write a note.** Press :kbd[c]. Since there's no session yet, tackline first asks for a session
   name. Then type the instruction and press :kbd[Enter] to save.

   ::image[The note popup: the code being annotated, the target session and the instruction]{src="img/note.svg"}

6. **Copy the prompt.** Press :kbd[3] for the Session view, check the notes, and press :kbd[y]. The
   compiled prompt is on your clipboard, ready to paste into Claude Code.
:::

Everything is saved as you go: quit with :kbd[q] whenever you like.

## Outside a git repository

tackline also runs in a plain folder. The Files view, notes, sessions and search all work; only the
Diff view needs git.

## Next

- [Workflow with Claude Code](workflow.md): getting good results from your notes.
- [Notes](guides/notes.md): everything you can do with a note.
- [Keys](reference/keys.md): press :kbd[?] in the app for the same list.
