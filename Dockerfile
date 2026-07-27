FROM rust:1.97 AS builder

WORKDIR /build

RUN apt-get update && \
    apt-get install -y --no-install-recommends libfuse3-dev pkg-config && \
    rm -rf /var/lib/apt/lists/*

COPY . .

RUN cargo build --release

FROM debian:trixie-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends fuse3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/clean-mount /usr/local/bin/clean-mount

RUN mkdir -p /mnt

ENTRYPOINT ["/usr/local/bin/clean-mount"]
