# exec

```bash
# tar
clean-mount exec /path/to/project -- tar -czf /tmp/out.tar.gz .

# zip
clean-mount exec /path/to/project -- zip -r /tmp/out.zip .

# rsync
clean-mount exec /path/to/project -- rsync -avz . user@server:/deploy-path

# cp to a non-default location with extra flags
clean-mount exec /path/to/project -- cp -r . /tmp/my-copy
```

Quick peek at what would be copied:

```bash
clean-mount exec /path/to/project -- ls -la
```

The command runs **with the filtered view as its working directory** — use `.` for "everything here". Use `{MOUNT}` in arguments only when you need the absolute path explicitly:

```bash
clean-mount exec /path/to/project -- cp -r {MOUNT}/src /tmp/src-only
```
