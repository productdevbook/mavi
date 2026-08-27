# The only process in Mavi that owns the official Hatchet SDK.
FROM golang:1.26-alpine AS builder

WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY main.go ./
ARG TARGETARCH
RUN CGO_ENABLED=0 GOOS=linux GOARCH=$TARGETARCH go build -trimpath -ldflags="-s -w" -o /out/mavi-hatchet-worker .

FROM alpine:3.22
RUN addgroup -S mavi && adduser -S -G mavi mavi
COPY --from=builder /out/mavi-hatchet-worker /usr/local/bin/mavi-hatchet-worker
USER mavi

EXPOSE 8090
ENTRYPOINT ["/usr/local/bin/mavi-hatchet-worker"]
