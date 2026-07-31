# Unmount

| Method                | Command                             |
| --------------------- | ----------------------------------- |
| Foreground process   | Press `Ctrl+C`                      |
| Daemon mount         | `clean-mount stop --pid <PID>`      |
| Daemon mount         | `clean-mount stop /tmp/mirror`      |
| Manual (Linux)        | `fusermount3 -u /tmp/mirror`        |
| Manual (macOS)        | `umount /tmp/mirror`                |
| Force unmount         | `fusermount3 -uz /tmp/mirror`       |

