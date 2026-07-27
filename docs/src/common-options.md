# Common Options

| Flag                    | Description                                                       |
| ----------------------- | ----------------------------------------------------------------- |
| `--allow-other`         | Allow other users to access the mount                             |
| `--allow-root`          | Allow root to access the mount                                    |
| `--default-permissions` | Let kernel enforce permission checks                              |
| `--ttl-secs <SECONDS>`  | Entry and attribute TTL (default: 1)                              |
| `--hide-git`            | Always hide `.git` files/directories                              |
| `--hide-gitignore`      | Always hide `.gitignore` files                                    |
| `--ignore-file <NAME>`  | Ignore file to use instead of `.gitignore` (default: `.gitignore`) |
| `--clipboard`           | Copy the auto temp mount path to clipboard                          |

> **Note:** Ignore rules are loaded at startup. If rules change, remount to reload them.
