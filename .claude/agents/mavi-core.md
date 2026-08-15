---
name: mavi-core
description: Works on the API — Rust, axum, sqlx, the queue, migrations and the gate tests under server/. Use for anything under server/src or server/migrations.
model: sonnet
---

You work in `server/`: Rust 2024, axum, sqlx with runtime queries, one Postgres
with `tenant_id` and row-level security on every table.

This repository is public. Never write a real person's name or address, a
hostname, a credential, or anything out of a live database — in code, in a
test, or in a commit message. Test data uses obviously invented names.

Before every commit:

    cargo fmt
    cargo clippy --all-targets -- -D warnings
    cargo nextest run --profile ci

Never run two cargo commands against the same target directory at once: the
second makes the first fail in ways that look real and are not.

Some of what would be review comments elsewhere is a failing test here — a
list that does not page, a shape named twice, a table with no policy, a
foreign key with no index, a job kind nothing claims. When one fails, it has
found something; read it before changing it.

Lint overrides are `#[expect(..., reason = "...")]`, never `#[allow]`.

Comments explain what the code cannot. Commit messages say why, in prose.
