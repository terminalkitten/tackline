---
title: Terminals
description: Shift+Enter, the mouse, the clipboard over ssh and tmux, light and dark themes.
---

tackline runs in any modern terminal. A few features depend on what the terminal supports.

## Shift+Enter

Most terminals send the same thing for :kbd[Shift+Enter] as for :kbd[Enter], so an app can't tell
them apart. tackline asks for the [kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/),
which fixes that, in terminals that have it:

| Terminal                           | :kbd[Shift+Enter] for a new line                    |
| ---------------------------------- | --------------------------------------------------- |
| Ghostty, Kitty, WezTerm, Alacritty | Works                                               |
| iTerm2                             | Turn on *Report keys using CSI u* (Profiles → Keys) |
| macOS Terminal.app, others         | Use `\` then :kbd[Enter]                            |

`\` then :kbd[Enter] works everywhere; :kbd[Alt+Enter] does too where Option sends Meta. The note
popup's title shows the newline key that works in your terminal.

## Mouse

tackline takes over the mouse for scrolling, clicking and dragging. To select terminal text the
usual way while it runs, hold a key while dragging: :kbd[Option] in iTerm2 and Terminal.app,
:kbd[Shift] in most others.

## Clipboard over ssh and tmux

:kbd[y] normally uses your system clipboard. Over ssh, that clipboard belongs to the remote machine,
so tackline asks your terminal to copy instead (OSC 52), and the prompt lands on your own computer.
The same happens if the system clipboard isn't available.

In tmux, allow it in `~/.tmux.conf`:

```text
set -g set-clipboard on
```

## Light and dark

Code colors come from a syntax theme. tackline picks a dark one, or a light one when the terminal
says its background is light (the `COLORFGBG` variable). Choose any theme with `--theme`:

```sh
tackline --theme InspiredGitHub
tackline --theme nope        # lists the available themes
```

Borders, the status bar and markers use your terminal's own color palette, so they follow its
light or dark scheme. Diff backgrounds pick a light or dark pair to match the syntax theme.

## Speed

Drawing is done by your terminal. GPU-accelerated terminals (Ghostty, WezTerm, Kitty, Alacritty,
iTerm2) scroll smoothest; tackline itself handles every key press and wheel step in well under a
frame.
