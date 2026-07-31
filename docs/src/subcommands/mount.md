# mount

```bash
# Auto temp dir: prints mount path, Ctrl+C to unmount
clean-mount mount /path/to/project

# Or mount at a specific directory
mkdir -p /tmp/mirror
clean-mount mount /path/to/project /tmp/mirror
```

Use another terminal to inspect the filtered view:

```bash
ls -la /tmp/mirror
cat /tmp/mirror/src/main.rs
cd /tmp/mirror && zip -r ~/filtered.zip .
```

## Daemon mode

Run the mount as a background daemon (requires an explicit mountpoint):

```bash
clean-mount mount /path/to/project /tmp/mirror --daemon
# Returns immediately, prints PID
```

The process forks and detaches from the terminal. The mount stays alive until:

  - The daemon is stopped with [`clean-mount stop --pid <PID>`](stop.md)
  - The mountpoint is unmounted manually (`fusermount3 -u /tmp/mirror`)
  - The system is rebooted

Use [`clean-mount status`](status.md) to see all active daemon mounts.
