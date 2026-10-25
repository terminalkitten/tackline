---
title: Web version
description: tackline in your browser, for people who'd rather click than learn keys.
---

`tackline serve` opens tackline in your browser. It's the same tool on the same files: notes you
add in the browser show up in the terminal app and in `tackline export`, and the other way round.
Nothing leaves your machine.

```sh
cd ~/code/my-project
tackline serve
```

tackline prints a link and opens it in your browser:

```text
tackline is running at http://127.0.0.1:7878/?token=…
Press Ctrl+C to stop.
```

::image[The web version: tacked code tinted yellow, the note marker in the margin]{src="../img/web-files.png"}

## Add a note

:::steps
1. **Open a file** from the tree on the left, or type in the search box at the top.
2. **Select lines**: click a line number, then shift-click another to select a block.
3. **Write the instruction** in the panel that appears and press **Save note** (or
   :kbd[Ctrl+Enter]). The first time, give the session a name.
:::

::image[Adding a note to two selected lines]{src="../img/web-add-note.png"}

Tacked lines are tinted yellow with `●` in the margin; hover a tacked line to read its note, or
click the `●` to edit it, move it to another session, or delete it. :kbd[Esc] cancels a selection.

## Diff

**Diff** shows your uncommitted changes, compared to `HEAD` (or the `--base` you started with).
Select added or unchanged lines to add notes, the same way; removed lines can't have notes.

::image[The Diff tab]{src="../img/web-diff.png"}

## Session and the prompt

**Session** lists the notes of the active session in prompt order. Move them with ↑ and ↓, edit or
delete them, and press **Copy prompt** to put the compiled prompt on your clipboard, ready to paste
into your agent. **Show the prompt** previews it.

::image[The Session tab]{src="../img/web-session.png"}

Switch the active session with the **Session** menu at the top, or make a new one with
**New session**.

## Options

```text
tackline serve [--port <PORT>] [--no-open] [--theme <NAME>] [--base <REV>]
```

| Option | |
|---|---|
| `--port` | Port on 127.0.0.1 (default 7878; `0` picks a free one). |
| `--no-open` | Print the link without opening the browser. |
| `--theme` | Code colors, as in the terminal app (see [Themes](../reference/themes.md)). |
| `--base` | Compare the Diff tab against `merge-base(HEAD, <rev>)`. |

The page itself follows your browser's light or dark setting; the code uses the theme's colors.

## Safety

The server only listens on `127.0.0.1`, so other machines can't reach it. The link contains a
random token that's swapped for a cookie on first visit; without it the server refuses every
request, so other websites open in your browser can't read your code or change your notes. Stop
the server with :kbd[Ctrl+C].

:::caution[One at a time]
The terminal app keeps sessions in memory while it runs. If you change notes in the browser while
the terminal app is open on the same repository, its next save can overwrite them. Use one at a
time, or restart the terminal app after working in the browser.
:::

For scripts and other tools, the same server has a small [JSON API](../reference/api.md).
