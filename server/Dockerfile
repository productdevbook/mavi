# syntax=docker/dockerfile:1

# The release image is built from the clean workspace. The legacy `server/`
# workspace is intentionally not part of this context or this image.
FROM rust:alpine AS builder
ARG SCCACHE_VERSION=0.17.0
ARG TARGETARCH
RUN apk add --no-cache ca-certificates curl musl-dev tar \
    && case "$TARGETARCH" in \
         amd64) SCCACHE_ARCH=x86_64-unknown-linux-musl ;; \
         arm64) SCCACHE_ARCH=aarch64-unknown-linux-musl ;; \
         *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
       esac \
    && curl -fsSL "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/sccache-v${SCCACHE_VERSION}-${SCCACHE_ARCH}.tar.gz" \
       | tar -xz --strip-components=1 -C /usr/local/bin "sccache-v${SCCACHE_VERSION}-${SCCACHE_ARCH}/sccache" \
    && chmod 0755 /usr/local/bin/sccache \
    && printf '%s\n' \
         '#!/bin/sh' \
         'set -u' \
         'if [ -e /tmp/mavi-sccache-disabled ]; then exec "$@"; fi' \
         'if /usr/local/bin/sccache "$@"; then exit 0; fi' \
         'touch /tmp/mavi-sccache-disabled' \
         'exec "$@"' \
       > /usr/local/bin/mavi-rustc-wrapper \
    && chmod 0755 /usr/local/bin/mavi-rustc-wrapper

WORKDIR /app
COPY . .
RUN --mount=type=secret,id=sccache-env,required=false \
    if [ -r /run/secrets/sccache-env ] \
       && sh -eu -c '. "$1"' sh /run/secrets/sccache-env >/dev/null 2>&1; then \
      set -a; . /run/secrets/sccache-env; set +a; \
      export RUSTC_WRAPPER=/usr/local/bin/mavi-rustc-wrapper; \
    elif [ -r /run/secrets/sccache-env ]; then \
      echo 'WARNING: invalid sccache environment; building without remote cache' >&2; \
    fi; \
    export CARGO_INCREMENTAL=0; \
    cargo build --locked --release --bin mavi

FROM alpine:latest AS runtime
RUN apk add --no-cache ca-certificates tzdata \
    && addgroup -S mavi \
    && adduser -S -G mavi mavi \
    && mkdir -p /data/files \
    && chown -R mavi:mavi /data

WORKDIR /app
COPY --from=builder /app/target/release/mavi /usr/local/bin/mavi

ENV LISTEN=0.0.0.0:8080 \
    MAVI_FILES_DIR=/data/files
USER mavi

EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/mavi"]
