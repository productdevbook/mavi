# syntax=docker/dockerfile:1

# The rewrite, as an image. Smaller than the one beside it in `old/` in every
# way that matters: no git, no bun, no browser — because nothing here builds a
# site's own project yet, and installing what a thing does not do is installing
# something for an attacker to find.

FROM rust:1.97-slim-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# How many compilers to run at once.
#
# Unset, cargo takes every core it can see, which is right on a build machine
# and wrong on one that is also serving: twelve rustc processes put the load
# average past thirty and everything else — the API, the proxy, kubelet —
# started queueing behind them. Whoever builds somewhere quiet can raise it.
ARG JOBS=4
ENV CARGO_BUILD_JOBS=$JOBS

ARG INCREMENTAL=0
ENV CARGO_INCREMENTAL=$INCREMENTAL

COPY . .

# The compiler's output outlives the layer it was made in: a cache mount is not
# part of the layer, so `COPY . .` does not throw it away and cargo rebuilds
# only what changed. `sharing=locked` because two builds writing one target
# directory is a corrupted target directory.
#
# The binary is copied out inside the same RUN, because a cache mount is gone
# by the time the next instruction runs.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,id=mavi-next-target,sharing=locked \
    cargo build --release --bin mavi \
    && mkdir -p /out \
    && cp target/release/mavi /out/

FROM debian:bookworm-slim AS runtime
WORKDIR /app

# Certificates, because everything this reaches — a mail host, a payment
# provider, somebody's webhook receiver — is reached over TLS. Nothing else:
# what is not installed cannot be run by whoever finds a way to run something.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --shell /usr/sbin/nologin mavi \
    && chown mavi:mavi /app

COPY --from=builder /out/mavi /usr/local/bin/mavi

# A fresh named volume inherits the ownership of the mount point in the image.
# Without this directory Docker creates it as root and the unprivileged process
# cannot write what somebody uploads to it.
RUN mkdir -p /data && chown mavi:mavi /data
VOLUME /data

USER mavi

EXPOSE 8080

# Everything this is told, with the one that has no sensible default left out:
# `DATABASE_URL`. A process that starts without knowing where its rows are and
# fails on the first request is a healthy pod answering five hundreds.
ENV FILES=/data \
    LISTEN=0.0.0.0:8080

ENTRYPOINT ["mavi"]
