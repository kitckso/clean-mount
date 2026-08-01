# Common Options

| Flag                    | Description                                                       |
| ----------------------- | ----------------------------------------------------------------- |
| `--allow-other`         | Allow other users to access the mount                             |
| `--allow-root`          | Allow root to access the mount                                    |
| `--default-permissions` | Let kernel enforce permission checks                              |
| `--ttl-secs <SECONDS>`  | Entry and attribute TTL (default: 1)                              |
| `--hide-git`            | Always hide `.git` files/directories                              |
| `--hide-gitignore`      | Always hide `.gitignore` files                                    |
| `--ignore-file <NAME>`  | Ignore file to use instead of `.gitignore` (default: `.gitignore`); errors if not found |
| `--no-ignore`           | Disable ignore-file processing entirely (show all files); pair with `--exclude` to filter ad-hoc without an ignore file |
| `--exclude <PATTERN>`   | Extra gitignore-style pattern(s) to hide; overrides the ignore file and `--include`. Repeatable |
| `--include <PATTERN>`   | Gitignore-style pattern(s) to keep visible even if the ignore file hides them; overridden by `--exclude`. Repeatable |
| `--clipboard`           | Copy the auto temp mount path to clipboard                          |

> **Note:** Ignore rules are loaded at startup. If rules change, remount to reload them. A missing default `.gitignore` produces a warning; a missing explicitly requested `--ignore-file` is an error.

`--exclude` and `--include` take gitignore-style glob patterns and may be repeated. Precedence (highest to lowest): `--hide-git`/`--hide-gitignore`, `--exclude`, `--include`, then the ignore file. `--no-ignore` disables only the ignore-file rules, so it pairs naturally with `--exclude` (or `--include`) for one-off filtering when no `.gitignore` exists.

Note that `--include` follows git semantics: including a file inside an ignored directory has no effect unless the directory itself (and any other ignored ancestors) is also included. To keep `secret/keep.env` visible when `.gitignore` hides `secret/`, pass both `--include secret/ --include secret/keep.env` (or `--include secret/` alone, which reopens the whole directory).

```bash
# Ad-hoc filtering without any ignore file
clean-mount list /path/to/project --no-ignore --exclude '*.log' --exclude .venv

# Hide extra paths on top of the ignore file
clean-mount cp /path/to/project /tmp/copy --exclude build/

# Keep a gitignored file visible
clean-mount tar /path/to/project /tmp/project.tar.gz --include keep.env
```
