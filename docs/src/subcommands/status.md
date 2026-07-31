# status

```bash
clean-mount status
```

Lists all active daemon mounts. Dead PIDs are filtered out automatically.

```text
$ clean-mount status
  PID  SOURCE                    MOUNTPOINT                               UPTIME
12345 /home/user/project       /tmp/mirror                               2h 15m
```

Each entry corresponds to a mount started with [`mount --daemon`](mount.md). The registry
is stored at `$XDG_RUNTIME_DIR/clean-mount/mounts/<pid>.mount`.
