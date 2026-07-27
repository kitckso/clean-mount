<div align="center">

# clean-mount

**A read-only FUSE filesystem that mirrors a directory while hiding files matched by `.gitignore`.**

</div>

> Make ignored files appear nonexistent to `ls`, `find`, `zip`, `tar`, `rsync`, editors, and AI agents.

---

clean-mount mounts a **read-only** FUSE filesystem over any directory. Files and directories matched by `.gitignore` rules are invisible — they return `ENOENT` as if they never existed. Nested `.gitignore` files are respected.
