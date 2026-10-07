# finance-utils

[![Status Badge](https://github.com/jluszcz/finance-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/jluszcz/finance-utils/actions/workflows/ci.yml)

Code shared by Paychecker and MisterManager, two local finance applications with the same shape: a
ratatui front end over SQLite, an HTML page of the figures written on quit, and a scheduled S3
backup. An application names itself once, as a `backup::Spec` and a report file name, and every
path, key and profile that differs between the two is derived from that.

Every feature is off by default and additive:

```toml
jluszcz_finance_utils = { git = "https://github.com/jluszcz/finance-utils", features = ["money", "report", "backup", "cli", "tui"] }
```

| Feature | Adds | Dependencies |
|---|---|---|
| *(always)* | `human_bytes` | `anyhow` |
| `money` | `money::{Cents, ParseMoneyError}` | `thiserror` |
| `config` | `config::{default_path, state_path, load, ReportConfig, BackupConfig}` | `serde`, `toml` |
| `report` | `report::{write, write_if_enabled, minify, escape, is_due, Written, Outcome}` | `config`, `chrono`, `minify-html` |
| `backup` | `backup::{Spec, run_if_due, is_due, next_due, Outcome, state, s3, cli}` | `config`, `chrono`, `clap`, `aws-config`, `aws-sdk-s3`, `aws-smithy-types`, `tokio`, `zstd` |
| `scratch` | `scratch::copy` | `chrono` |
| `cli` | `cli::CommonArgs` | `config`, `scratch`, `chrono`, `clap` |
| `tui` | `tui::{centered, is_press, text, date, help}` | `ratatui`, `chrono` |
| `test-support` | `tui::testing` | `tui` |

### `human_bytes` (always)

`human_bytes(bytes: u64) -> String`: whole KiB rounded up below a MiB, whole MiB from one up.

### `money`

`Cents(pub i64)`, the only money type.

- `Cents::ZERO`, `from_dollars`, `dollars` (floors), `floor_to_dollar`, `trunc_to_dollar`,
  `ceil_to_hundred_dollars` (saturating), `to_whole_dollars`.
- `Add`, `Sub`, `Neg`, `AddAssign`, `Sum`.
- `Display`: `$` omitted, thousands grouped, two decimals, sign before the digits.
- `FromStr`: strips `$`, `,`, `_` and spaces; one optional leading `-`; at most two decimals; `.5`
  and `5.` accepted; overflow is an error rather than a wrap. The error is `ParseMoneyError`.

### `config`

- `default_path(app) -> Result<PathBuf>`: `$XDG_CONFIG_HOME/<app>/config.toml`, or `~/.config`
  when unset or empty.
- `state_path(app, file) -> Result<PathBuf>`: `$XDG_STATE_HOME/<app>/<file>`, or `~/.local/state`.
- `load<T: DeserializeOwned + Default>(path) -> Result<T>`: a missing file is `T::default()`; a
  file that does not parse is an error naming the path.
- `ReportConfig { dir }`: `new(dir)`, and `dir()` expands a leading `~` or `~/` and refuses a
  relative path.
- `BackupConfig { bucket, profile, interval_days }`: `interval_days` defaults to 7, and
  `profile_or(default)` supplies the application's default profile.

Each application keeps its own top-level `Config`, holding `Option<ReportConfig>` and
`Option<BackupConfig>` beside anything of its own. An absent section is "off", a key nothing reads
is ignored, and `dir` and `bucket` have no default so that a typo is an error.

### `report`

Writes an HTML page to a synced directory: minified, atomically, and only when it is due.

- `minify(page) -> Vec<u8>`: `minify-html` with CSS minified and no JavaScript minifier.
- `write(dir, file_name, page) -> Result<Written>`: creates `dir`, writes a temporary file beside
  the page with the existing page's permissions, and renames it onto `file_name`. The temporary
  file is removed on every error path.
- `is_due(last_written, today, wrote_rows) -> bool`.
- `write_if_enabled(cfg, skip, file_name, today, wrote_rows, render) -> Result<Outcome>`:
  `Disabled` with no section, `Skipped` when the caller says `skip`, `Unchanged` when the page was
  written today and no rows were, else `Written`. `render` runs only when a page will be written.
- `escape(text) -> String`: `&`, `<`, `>`, `"`.
- `Written { path, bytes }` and `Outcome { Disabled, Skipped, Unchanged, Written(Written) }`.

### `backup`

`Spec { app, stem }` names an application once. The state file, default AWS profile and snapshot
directory derive from `app`; the object key and snapshot file name from `stem`.

- `Spec::key_for(now)` and `Spec::state_path()`.
- `is_due(last, now, interval_days)` and `next_due(last, interval_days)`; `interval_days` is
  clamped to ten years before it reaches `TimeDelta::days`.
- `run_if_due(spec, db_path, cfg, state_path, now, force, snapshot) -> Result<Outcome>`, where
  `snapshot` copies the database at the first path to the second. The caller supplies it, so the
  crate never depends on `rusqlite`. The snapshot is zstd-compressed before upload, so the key
  ends `.db.zst` and a restore runs `zstd -d` on the download.
- `Outcome { Disabled, NotDue { next }, BackedUp { bucket, key, bytes } }`; `bytes` is the
  compressed size.
- `state::{State, read, write}`: `read` gives `Ok(None)` for a missing file and `Err` for an
  unreadable one.
- `s3::upload(profile, bucket, key, file)`: a current-thread runtime for the one call, credentials
  from the named profile alone, and `If-None-Match: *` so a backup can be added but never replaced.
- `cli`:
  - `BackupArgs` (`clap::Args`): `--force`, and `--status` conflicting with it.
  - `command(spec, db_path, cfg, args, snapshot)`: the `backup` subcommand; a failed run is an
    error exit.
  - `scheduled(spec, db_path, cfg, snapshot)`: the check after every run. It never fails; it
    prints to stdout only when it uploaded, and a failure goes to stderr.
  - `describe(&Outcome) -> String` and `status(spec, cfg, state) -> String` build the text the two
    print.

Neither `command` nor `scheduled` opens the database. The caller runs the scheduled check only on
its default database. The IAM policy and bucket stay in each application's Terraform, which must
allow `PutObject` only, and only with `If-None-Match`.

### `scratch`

`copy(app, src, snapshot) -> Result<PathBuf>`: snapshots the database at `src` into
`<temp dir>/<app>-scratch-<timestamp>-<pid>/`, mode 0700, and returns the copy's path, for a run
that must not touch the real database. `snapshot` is the closure `backup` takes. A missing `src` is
an error rather than an empty copy. The directory is removed if the snapshot fails and left behind
if it succeeds, so the copy can be inspected after the run.

### `cli`

`cli::CommonArgs` is the `--db`, `--scratch`, `--today`, and `--config` flags, flattened into an
application's own `Cli`. `db_path` resolves `--scratch` through `scratch::copy`; `is_default_db`
says whether the backup schedule applies to the run.

### `tui`

- `centered(area, width, height) -> Rect` and `is_press(&KeyEvent) -> bool`.
- `text::{TextBuffer, Edit, edit_key, is_bare}`: a line of text with a caret, and the Ctrl-key
  editing shared by every text box. `TextBuffer` has `value`, `caret`, `len`, `is_empty`, `set`,
  `clear`, `insert`, `backspace`, `delete`, `step`, `start`, `end`, `delete_word_back`,
  `kill_to_start` and `kill_to_end`.
- `date::{iso, parse_shorthand, parse, Step}`: `iso` formats `YYYY-MM-DD`; `parse_shorthand`
  resolves `M/D` against a reference day, where the year turns on the month alone. `date::parse`
  reads `YYYY-MM-DD` or `M/D`; `date::Step` is what `←`/`→` (a day), `Shift` with them (a week),
  and `[`/`]` (a month) do to a date field, via `Step::from_key`.
- `help::Entry` tables, each entry's footer `Label`, drive both the footer (`footer_items`, joined
  by the application) and the `?` panel (`render_panel`); `duplicate_keys` finds keys a table
  binds twice.

### `test-support`

`tui::testing::{key, shift, ctrl, draw_buffer, draw, buffer_text}`, for an application's TUI tests.
Enable it from `[dev-dependencies]` only.

## Development

See `AGENTS.md`.
