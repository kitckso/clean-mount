# zip

```bash
clean-mount zip /path/to/project /tmp/project.zip
```

Creates a zip archive of the filtered view. Internally: mount → `zip -r` → unmount.
