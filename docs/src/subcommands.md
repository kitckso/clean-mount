# Subcommands

| Subcommand | What it does |
|---|---|
| `mount SOURCE [MOUNTPOINT]` | Mount (omit mountpoint for auto temp dir + print path) |
| `open SOURCE` | Mount + open in file manager |
| `cp SOURCE DEST` | Mount, `cp -a` the filtered view to DEST, unmount |
| `list SOURCE` | Preview the filtered view without mounting (flat listing, no summary) |
| `exec SOURCE -- <command>` | Mount, run any command against the filtered view, unmount |
| `tar SOURCE OUTPUT` | Mount, create tarball of the filtered view, unmount (compression from suffix) |
| `zip SOURCE OUTPUT` | Mount, create `.zip`  of the filtered view, unmount |
| `complete [SHELL]` | Generate shell completion script (bash, zsh, fish, elvish, powershell). Use `--install` to auto-add the eval line to your shell rc file. |

`tar`, `zip`, `cp`, `mount`, `open`, `exec`, and `list` accept the same common options (`--hide-git`, `--ignore-file`, etc.). `complete` does not need them.
