# syntax=docker/dockerfile:1

FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev

WORKDIR /app
COPY . .

RUN cargo build --release --bin mavi

FROM alpine:latest AS runtime
RUN apk add --no-cache ca-certificates tzdata

WORKDIR /app
COPY --from=builder /app/target/release/mavi /usr/local/bin/mavi

EXPOSE 3000

CMD ["mavi"]
