# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Commands

```bash
cargo build --all-features
cargo test --all-features
cargo fmt                                                   # pre-commit runs `cargo fmt --check`
cargo clippy --all-targets --all-features -- -D warnings    # CI treats warnings as errors
for f in money config report backup scratch cli tui test-support; do cargo check --no-default-features --features $f || break; done
```

## What this is

Code shared by Paychecker, MisterManager and Funder, consumed by each as an unpinned git
dependency: a change to `main` reaches all three on their next `cargo update`, so `main` must
always build for all of them. Features are default-off and additive, and each application enables
only what it uses (Funder writes no report). What differs between the applications is passed in:
the application's name and key stem as a `backup::Spec`, the report's file name, the snapshot as a
closure. Nothing here names any application.

## No real data in the repository

The repository is public; the owners' finances are not. **Nothing committed here may carry a real
figure, a real employer, a real bucket name, or a name that identifies a real person** — not in
source, tests, docs, commit messages, or PR text. Every money literal is invented.

## Conventions

- `#![warn(missing_docs)]` plus `-D warnings`: every public item needs a doc comment. Document the
  *why* a caller cannot infer.
- `rusqlite` is never a dependency: the applications own their databases, and `backup` takes the
  snapshot as a closure.
- `minify_html` is named only in `src/report.rs`; `aws_config`, `aws_sdk_s3`, `aws_smithy_types`
  and `tokio` only in `src/backup/s3.rs`; `zstd` only in `src/backup/mod.rs`; `serde` and `toml`
  only in `src/config.rs` and `src/backup/state.rs`.
- AWS crates take `default-features = false` and ring-based rustls (see `Cargo.toml`).

## Backup invariants

- The IAM user each application declares may only `PutObject`, and only with `If-None-Match: *`,
  which `s3::upload` sends. The key is long-lived and unattended, so the policy bounds it: it can
  add a backup but never replace one. Restores use the owner's own identity.
- `s3::upload` authenticates with the profile's keys alone: `ProfileFileCredentialsProvider`, so an
  exported `AWS_ACCESS_KEY_ID` cannot substitute another identity.
- No key prefix: `Spec::key_for` and each IAM policy's `<bucket arn>/*` would otherwise have to
  spell it identically, with `AccessDenied` as the only sign they drifted.
- The schedule reads `Utc::now()`, never an application's simulated date, and callers run the
  scheduled check only on their default database. An explicit backup command is exempt.
- The state file is advisory: unreadable means a warning and one redundant upload. It is written
  only after a successful upload, and the snapshot is removed on both paths.
- The snapshot directory's leaf is created non-recursively with mode 0700; see
  `private_dir::create`, which `scratch::copy` creates its directory with too. Anything here that
  writes a copy of a database to disk goes through it.
- `interval_days` is clamped to ten years before it reaches `TimeDelta::days`, which panics
  outside chrono's calendar.

## Testing conventions

Test names are full sentences describing the scenario. Unit tests live in `mod tests` at the
bottom of the file under test. Temp paths carry the test's label and the pid, since tests in one
process run at once.
