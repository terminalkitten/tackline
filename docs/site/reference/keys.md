---
title: Keys
description: Every key in tackline. Press ? in the app for the same list.
---

Everything uses plain keys or :kbd[Ctrl]: macOS terminals keep :kbd[Cmd] shortcuts for themselves,
so tackline never uses them. Press :kbd[?] in the app for a quick list.

## Anywhere

| Key                         | Action                             |
| --------------------------- | ---------------------------------- |
| :kbd[1] / :kbd[2] / :kbd[3] | Files / Diff / Session view        |
| :kbd[Ctrl+N]                | New session                        |
| :kbd[Ctrl+O]                | Pick the active session            |
| :kbd[Shift+Tab]             | Switch to the next session         |
| :kbd[Ctrl+P]                | Find a file                        |
| :kbd[Ctrl+F]                | Search code in the repo            |
| :kbd[?]                     | Help                               |
| :kbd[q]                     | Quit (everything is already saved) |
| :kbd[Esc]                   | Close a popup                      |

## File tree and diff file list

| Key               | Action                                                               |
| ----------------- | -------------------------------------------------------------------- |
| :kbd[j] / :kbd[k] | Down / up                                                            |
| :kbd[Enter]       | Tree: open a file, open or close a folder. Diff list: go to the diff |
| :kbd[l] / :kbd[h] | Tree: open / close a folder                                          |
| :kbd[Tab]         | Switch to the code                                                   |
| :kbd[r]           | Diff list: reload the diff                                           |

## Code

| Key                                                       | Action                                                          |
| --------------------------------------------------------- | --------------------------------------------------------------- |
| :kbd[j] :kbd[k] :kbd[h] :kbd[l], arrows                   | Move                                                            |
| :kbd[g] :kbd[g] / :kbd[G]                                 | Top / bottom                                                    |
| :kbd[Ctrl+D] / :kbd[Ctrl+U]                               | Half a page down / up                                           |
| :kbd[0] / :kbd[$]                                         | Start / end of the line                                         |
| :kbd[:] then a number                                     | Go to line                                                      |
| :kbd[/] then :kbd[n] / :kbd[N]                            | Search this file, next / previous match                         |
| :kbd[w]                                                   | Wrap long lines on / off                                        |
| :kbd[V] / :kbd[v]                                         | Select lines / characters (again: stop)                         |
| :kbd[Shift+↑] :kbd[Shift+↓] / :kbd[Shift+←] :kbd[Shift+→] | Select lines / characters                                       |
| :kbd[Esc]                                                 | Clear the selection                                             |
| :kbd[c]                                                   | Note the selection or the line; on a noted line, edit that note |
| :kbd[d]                                                   | Delete the note on this line (asks first)                       |
| :kbd[r]                                                   | Diff view: reload the diff                                      |
| :kbd[Tab]                                                 | Switch to the tree or file list                                 |

## Note popup

| Key                         | Action                                        |
| --------------------------- | --------------------------------------------- |
| :kbd[Enter]                 | Save                                          |
| `\` then :kbd[Enter]        | New line                                      |
| :kbd[Shift+Enter]           | New line, where the terminal reports it       |
| :kbd[Tab] / :kbd[Shift+Tab] | Next / previous session; moves an edited note |
| :kbd[Esc]                   | Cancel                                        |

## Session view

| Key               | Action                       |
| ----------------- | ---------------------------- |
| :kbd[j] / :kbd[k] | Select a note                |
| :kbd[Enter]       | Jump to the note's code      |
| :kbd[e]           | Edit the note                |
| :kbd[d]           | Delete the note (asks first) |
| :kbd[J] / :kbd[K] | Move the note down / up      |
| :kbd[y]           | Copy the compiled prompt     |

## Find and search

| Key                                            | Action                                                 |
| ---------------------------------------------- | ------------------------------------------------------ |
| typing                                         | The query                                              |
| :kbd[↑] / :kbd[↓], :kbd[Ctrl+P] / :kbd[Ctrl+N] | Previous / next result                                 |
| :kbd[Enter]                                    | Open the result (repo search: search first, then open) |
| :kbd[Esc]                                      | Close                                                  |

In the session picker (:kbd[Ctrl+O]), :kbd[j] / :kbd[k] or the arrows move and :kbd[Enter] picks.

## Mouse

| Action                      | Effect                                              |
| --------------------------- | --------------------------------------------------- |
| Wheel or trackpad           | Scroll the code (in the Session view: select notes) |
| Click in the code           | Place the cursor                                    |
| Drag in the code            | Select characters                                   |
| Click in the diff file list | Show that file                                      |
