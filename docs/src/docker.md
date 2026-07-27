# Docker

Build the image:

```bash
docker build -t clean-mount .
```

Run:

```bash
docker run --rm -it \
  --device /dev/fuse \
  --cap-add SYS_ADMIN \
  --security-opt apparmor=unconfined \
  -v "$PWD/project:/source:ro" \
  clean-mount \
  mount /source /mnt
```

> **Limitation:** FUSE mounts are per-mount-namespace — they happen inside the container and are **not visible from the host**. To inspect the filtered view from another terminal:
>
> ```bash
> docker exec -it <container-id> ls /mnt
> ```
>
> The primary use of the Docker image is building and testing in CI/CD pipelines.
