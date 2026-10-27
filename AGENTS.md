# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Commands

```bash
cargo build --all-features
cargo test --all-features
cargo fmt                                                   # pre-commit runs `cargo fmt --check`
cargo clippy --all-targets --all-features -- -D warnings    # CI treats warnings as errors
for f in money config report backup scratch cli sqlite tui test-support; do cargo check --no-default-features --features $f || break; done
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
- `rusqlite` is named only in `src/sqlite.rs`, behind the `sqlite` feature. The applications own
  their schemas and their queries; `sqlite` opens, migrates and snapshots. `backup` and `scratch`
  still take the snapshot as a closure, so neither needs the feature.
- `minify_html` is named only in `src/report/mod.rs`; `aws_config`, `aws_sdk_s3`, `aws_smithy_types`
  and `tokio` only in `src/backup/s3.rs`; `zstd` only in `src/backup/mod.rs`; `serde` and `toml`
  only in `src/config.rs` and `src/backup/state.rs`.
- `text_enum!`, `row_id!` and `impl_scroll!` are `#[macro_export]`ed, so they sit at the crate
  root; they reach their dependencies through `$crate` paths, so an application names none of the
  crates behind them to use them.
- AWS crates take `default-features = false`; aws-sdk-s3 uses `default-https-client` (rustls 0.23), not the legacy `rustls` feature (see `Cargo.toml`).

## Report invariants

- Two page shells live in `report::html`. `page` and the `Tab` radios are the script-free form; `app_page`
  is the script-capable one, and neither names any application.
- `app_page` carries `shell.css` and `shell.js` (`include_str!`), then the application's own `style`
  and `script`. The shell sets the colour tokens (`--bg --fg --mut --rule --soft --acc`, redefined
  under `prefers-color-scheme: dark`) and sets no table metrics: those are each application's.
- The tab bar has two forms, chosen by the tabs themselves: `nav.tabs.icons` when every `NavTab`
  has an icon (a fixed bottom bar under `48rem`, a sticky top row above it), `nav.tabs.wrap`
  otherwise (always a top row of buttons at least 44px tall that wraps, never scrolls sideways).
- The JSON block comes before both scripts and must pass through `script_json`, which writes every
  `<` as `\u003c` so no string can end the element.
- `report::minify` runs with `minify_js`: the scripts are plain ES2020 with no libraries, and the
  page makes no external request of any kind.

## Backup invariants

- The IAM user each application declares may only `PutObject`, and only with `If-None-Match: *`,
  which `s3::upload` sends. The key is long-lived and unattended, so the policy bounds it: it can
  add a backup but never replace one. Restores use the owner's own identity.
- `s3::upload` authenticates with the profile's keys alone: `ProfileFileCredentialsProvider`, so an
  exported `AWS_ACCESS_KEY_ID` cannot substitute another identity.
- No key prefix: `Spec::key_for` and each IAM policy's `<bucket arn>/*` would otherwise have to
  spell it identically, with `AccessDenied` as the only sign they drifted.
- The schedule reads `Utc::now()`, never an application's simulated date, and the scheduled check
  runs only on the default database, which `CommonArgs::scheduled_backup` enforces. An explicit
  backup command is exempt.
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
