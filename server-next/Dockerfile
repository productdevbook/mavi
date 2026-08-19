# syntax=docker/dockerfile:1

# The release image is built from the clean workspace. The legacy `server/`
# workspace is intentionally not part of this context or this image.
FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev

WORKDIR /app
COPY . .
RUN cargo build --locked --release --bin mavi

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
