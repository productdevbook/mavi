# syntax=docker/dockerfile:1

FROM oven/bun:1-alpine AS builder
WORKDIR /app

# Dependencies as their own layer: they only change when these two files do.
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile

COPY . .
# Where the panel reports its errors, if anywhere — baked in, because the
# result is static files. Empty leaves reporting off, which is the default
# a self-hosted build should have.
ARG VITE_SENTRY_DSN=
ARG VITE_SENTRY_RELEASE=
ENV VITE_SENTRY_DSN=$VITE_SENTRY_DSN \
    VITE_SENTRY_RELEASE=$VITE_SENTRY_RELEASE
RUN bun run build

# Sourcemaps go to the error tracker, when one is named, and into the image
# never: a reader gets minified code either way, and a report gets real file
# and line only if this step ran. The token arrives as a build secret because
# an ARG would sit in the image history for anyone who pulls it. An upload
# failure is said out loud but does not stop the ship — the panel without
# resolved traces is the panel we had yesterday.
ARG SENTRY_ORG=
ARG SENTRY_PROJECT=
RUN --mount=type=secret,id=sentry-token \
    if [ -s /run/secrets/sentry-token ] && [ -n "$SENTRY_ORG" ]; then \
        export SENTRY_AUTH_TOKEN="$(cat /run/secrets/sentry-token)"; \
        for d in dist dist-learn dist-shop; do \
            bunx sentry@0.42.2 sourcemap inject "$d" --allow-empty \
            && bunx sentry@0.42.2 sourcemap upload "$d" --allow-empty \
            || echo "WARNING: sourcemap upload failed for $d; its reports stay minified"; \
        done; \
    fi \
    && find dist dist-learn dist-shop -name '*.map' -delete

FROM nginx:alpine AS runtime

# Under admin/ so that the paths the build wrote (/admin/assets/…) are the
# paths nginx finds on disk.
COPY --from=builder /app/dist /usr/share/nginx/html/admin
COPY --from=builder /app/dist-learn /usr/share/nginx/html/learn
COPY --from=builder /app/dist-shop /usr/share/nginx/html/shop
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
