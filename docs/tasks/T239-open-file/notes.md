# T239 notes

## How Codex opens a path

Read only, from the local ChatGPT app's `Contents/Resources/app.asar` (the
Codex Electron shell): fixed-string searches and the bytes around the hits,
nothing extracted or run.

- File manager (Finder): `r6(e)` returns `["-R", e]` on darwin, so
  `open -R <path>` selects the path in Finder.
- Terminal: `args: e => ["-a", appName, es8(e)]` with `/usr/bin/open`, where
  `es8` is the path when it is a directory, else its `dirname`. A terminal
  opens the path's directory.
- Editors: the VS Code family runs the app's own CLI
  (`…/Contents/Resources/app/bin/code`) with `--goto <path>[:line:col]`.

## cupper

The rules of T126 stay: only the built-in lists, `cupper-sys::spawn`, no
shell, an empty environment, `app_open`. With `path`:

| target | command |
|---|---|
| `editor` | `open -a <editor> <realpath>` |
| `terminal` | `open -a <terminal> <realpath, or its directory for a file>` |
| `finder` | `open -R <realpath>` |

- No editor CLI with `--goto`: it is not on the list, and there are no lines
  to go to.
- A terminal gets the directory: `open -a Terminal <file>` runs the file as a
  script.
- `path` is refused (`invalid_params`) when it is empty or absolute, has a
  `..` part, is not there, or leads out of the workspace once its links are
  resolved. `open` gets the real path checked, not the path given.
