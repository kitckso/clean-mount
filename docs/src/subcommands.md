# Subcommands

| Subcommand | What it does |
|---|---|
| `mount SOURCE [MOUNTPOINT]` | Mount (omit mountpoint for auto temp dir + print path) |
| `open SOURCE` | Mount + open in file manager |
| `cp SOURCE DEST` | Mount, `cp -a` the filtered view to DEST, unmount |
| `exec SOURCE -- <command>` | Mount, run any command against the filtered view, unmount |

All subcommands accept the same common options (`--hide-git`, `--ignore-file`, etc.).
