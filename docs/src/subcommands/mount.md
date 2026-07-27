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
