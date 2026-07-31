# stop

```bash
# By PID
clean-mount stop --pid 12345

# By mountpoint
clean-mount stop /tmp/mirror
```

Unmounts a running daemon mount. Internally runs `fusermount3 -u` (or `umount` as fallback)
against the resolved mountpoint. The daemon process detects the unmount and exits cleanly.

Use [`clean-mount status`](status.md) to find the PID of a running daemon mount.
