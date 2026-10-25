---
title: Command line
description: tackline, tackline export and tackline sessions, with their options.
---

tackline finds the repository root from the current folder, so every command works from any
subdirectory.

## tackline

```text
tackline [--base <rev>] [--theme <name>]
```

Opens the app, on the Diff view when there are uncommitted changes, otherwise on the Files view.

| Option           |                                                                                                                                                                 |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--base <rev>`   | Diff against `merge-base(HEAD, <rev>)` instead of `HEAD`: a branch, tag or commit, e.g. `upstream/main`. An unknown revision stops with git's error.            |
| `--theme <name>` | Syntax highlighting theme. Without it, tackline picks `base16-ocean.dark`, or `InspiredGitHub` on a light terminal. An unknown name lists the available themes. |

## tackline serve

```text
tackline serve [--port <PORT>] [--no-open]
```

Opens tackline in the browser, on this machine only. See [Web version](../guides/web.md).

## tackline export

```text
tackline export [<session>]
```

Prints the compiled prompt of the active session, or of `<session>` (its name, its file name or its
id). Warnings, such as skipped stale notes, go to stderr.

```sh
tackline export | claude -p
tackline export "Search speedup" > prompt.md
```

## tackline sessions

```text
tackline sessions
```

Lists the sessions with their note counts; `*` marks the active one.

```text
* Search speedup (3 notes)
  Docs pass (1 note)
```
