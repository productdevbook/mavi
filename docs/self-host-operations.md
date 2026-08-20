# Self-host operations

The compose package is the supported self-host boundary. Keep the following
three values stable for the lifetime of an installation:

- `MAVI_SITE_ID` — the site identity in every site-scoped row;
- `MAVI_KEYS` — the keyring for provider credentials and security-sensitive
  mail bodies; and
- PostgreSQL data plus the API `/data` volume — the database and binary files.

Never copy a live `.env` into a backup or commit it. The backup tools include
the site ID and release label, but never include `DATABASE_URL`, `MAVI_KEYS`
or mail/provider tokens.

## Before an upgrade

Use an immutable release tag in `.env` rather than `latest`:

```dotenv
MAVI_VERSION=v0.1.0
```

Create and verify a backup before pulling the next release:

```bash
./scripts/mavi-backup.sh
sha256sum --check backups/mavi-*/SHA256SUMS
```

The backup command briefly stops the API and panel so the PostgreSQL dump and
`/data/files` archive describe the same application state. It leaves the
PostgreSQL service running and restarts API/panel services that were running
before the command.

Upgrade only after the backup has been copied to storage outside the host:

```bash
docker compose pull
docker compose up -d
docker compose ps
curl --fail --retry 30 --retry-delay 2 http://localhost/readyz
```

Mavi applies pending migrations before opening its listener. Migrations are
forward-only and each migration runs transactionally; there are no automatic
down migrations because silently guessing how to reverse user data is unsafe.

## Recovery

If the new release does not become ready, pin the previous tag and restart:

```bash
MAVI_VERSION=v0.1.0 docker compose up -d
curl --fail --retry 30 --retry-delay 2 http://localhost/readyz
```

If the database or file volume also needs recovery, use the explicit
confirmation flag. This replaces the target database and `/data/files`; it
refuses to restore a backup belonging to another `MAVI_SITE_ID`:

```bash
./scripts/mavi-restore.sh --yes backups/mavi-20260820T120000Z
```

Restore is a maintenance operation. Keep the API and panel inaccessible at the
proxy while it runs, and verify setup/login, a public page, a private media
download and the panel after `/readyz` returns successfully.

## Release smoke checks

Every release must pass these checks against a fresh fixed-site installation:

1. `/healthz` returns process liveness without a site host.
2. `/readyz` returns database readiness after migrations.
3. `POST /api/v1/setup` initializes one owner exactly once.
4. `/admin/` loads the panel and its generated client reaches the same API.
5. A public content read and authenticated private media read preserve the
   configured site boundary.
6. Restarting `api` preserves credentials, files and the site ID.

The operator may activate a cloud placement only after the Mavi runtime
manifest reports the exact release, API contract fingerprint and storage schema
version expected by that operator release. A source commit or moving `latest`
is not an activation contract.
