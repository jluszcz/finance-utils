# finance-utils

[![Status Badge](https://github.com/jluszcz/finance-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/jluszcz/finance-utils/actions/workflows/ci.yml)

Code shared by Paychecker, MisterManager and Funder, three local finance applications with the
same shape: a ratatui front end over SQLite, a scheduled S3 backup, and (for the first two) an HTML
page of the figures written on quit. An application names itself once, as a `backup::Spec` and a
report file name, and every path, key and profile that differs between them is derived from that.

Every feature is off by default and additive:

```toml
jluszcz_finance_utils = { git = "https://github.com/jluszcz/finance-utils", features = ["money", "report", "backup", "cli", "tui"] }
```

| Feature | Adds | Dependencies |
|---|---|---|
| *(always)* | `human_bytes`, `text_enum!` | `anyhow` |
| `money` | `money::{Cents, ParseMoneyError, dollar_sign}` | `thiserror` |
| `config` | `config::{default_path, state_path, data_path, load, ReportConfig, BackupConfig}` | `serde`, `toml` |
| `report` | `report::{write, write_if_enabled, minify, escape, is_due, Written, Outcome, html, cli}` | `config`, `chrono`, `clap`, `minify-html` |
| `backup` | `backup::{Spec, run_if_due, is_due, next_due, Outcome, state, s3, cli}` | `config`, `chrono`, `clap`, `aws-config`, `aws-sdk-s3`, `aws-smithy-types`, `tokio`, `zstd` |
| `scratch` | `scratch::copy` | `chrono` |
| `cli` | `cli::{CommonArgs, name_defaults, parse}` | `config`, `scratch`, `chrono`, `clap` |
| `sqlite` | `sqlite::{open, open_in_memory, migrate, snapshot, Schema, Migration, row_id!}` | `rusqlite` (bundled) |
| `tui` | `tui::{centered, is_press, step_index, next_in, app, status, text, date, help}` | `ratatui`, `chrono` |
| `test-support` | `testing`, `tui::testing` | `tui` |

### `human_bytes` (always)

`human_bytes(bytes: u64) -> String`: whole KiB rounded up below a MiB, whole MiB from one up.

### `text_enum!` (always)

`text_enum!` generates what an enum behind a `TEXT` column needs from one list of variants: `ALL`,
`as_str` (the stored token), `index` (a variant's place in `ALL`) and `FromStr`, which searches
`ALL` through `as_str`. Its noun names the column in the refusal for an unknown token. A display
label is not generated, since it is prose free to change without a migration. The enum derives
`Copy` and `PartialEq`.

### `money`

`Cents(pub i64)`, the only money type.

- `Cents::ZERO`, `from_dollars`, `dollars` (floors), `floor_to_dollar`, `trunc_to_dollar`,
  `ceil_to_hundred_dollars` (saturating), `to_whole_dollars`.
- `usd` is `Display` with a dollar sign (`-$1,234.56`); `usd_whole` drops the cents first, so a
  loss under a dollar reads `$0`. `money::dollar_sign(text)` puts the `$` after any leading minus
  of text already formatted, for a figure an application has turned into digits its own way.
- `Add`, `Sub`, `Neg`, `AddAssign`, `Sum`.
- `Display`: `$` omitted, thousands grouped, two decimals, sign before the digits.
- `FromStr`: strips `$`, `,`, `_` and spaces; one optional leading `-`; at most two decimals; `.5`
  and `5.` accepted; overflow is an error rather than a wrap. The error is `ParseMoneyError`.

### `config`

- `default_path(app) -> Result<PathBuf>`: `$XDG_CONFIG_HOME/<app>/config.toml`, or `~/.config`
  when unset or empty.
- `state_path(app, file) -> Result<PathBuf>`: `$XDG_STATE_HOME/<app>/<file>`, or `~/.local/state`.
- `data_path(app, file) -> Result<PathBuf>`: `~/.local/share/<app>/<file>`, the database's home.
  Fixed under `$HOME`; `$XDG_DATA_HOME` does not move it.
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
- `html`: the page around the figures and a tab bar switched by CSS alone, so the page carries no
  script. `Tab::new(id, label)`; `tab_inputs(tabs, open)` are the radios, which go ahead of the nav
  and every panel; `tab_nav(tabs)` is the bar; `tab_rules(tabs)` is the CSS showing the open
  panel (`<id>-panel`), lighting its label and placing the focus ring; `page(title, style, body,
  stamp)` is the document, with the footer stamp.
- `Written { path, bytes }` and `Outcome { Disabled, Skipped, Unchanged, Written(Written) }`.
- `cli::ReportArgs` (`--dir`), flattened into an application's `report` subcommand;
  `cli::dir(args, scratch_dir, cfg, config_path)` picks `--dir`, then a scratch run's directory,
  then `[report] dir`, and with none is an error naming the config file. `cli::describe(&Written)`
  is the `wrote 12 KiB to …` line; `cli::after_quit(result)` prints it, warns on an error, and is
  silent otherwise. `cli::on_quit(skip, scratch_dir, write, configured)` decides the page a quit
  writes: none when `skip`, a `--scratch` run's own directory when there is one, else
  `configured`'s decision.

### `backup`

`Spec { app, stem }` names an application once. The state file, default AWS profile and snapshot
directory derive from `app`; the object key and snapshot file name from `stem`.

- `Spec::key_for(now)` and `Spec::state_path()`.
- `is_due(last, now, interval_days)` and `next_due(last, interval_days)`; `interval_days` is
  clamped to ten years before it reaches `TimeDelta::days`.
- `run_if_due(spec, db_path, cfg, state_path, now, force, snapshot) -> Result<Outcome>`, where
  `snapshot` copies the database at the first path to the second. The caller supplies it
  (`sqlite::snapshot` fits), so `backup` needs no `sqlite` feature. The snapshot is zstd-compressed before upload, so the key
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
  - With `cli` on too, `CommonArgs::refuse_scratch_backup()` refuses `--scratch` with the
    `backup` subcommand, and `CommonArgs::scheduled_backup(spec, db_path, cfg, snapshot)` is
    `scheduled` on the default database only.

Neither `command` nor `scheduled` opens the database. The scheduled check belongs to the default
database alone, which `scheduled_backup` enforces. The IAM policy and bucket stay in each application's Terraform, which must
allow `PutObject` only, and only with `If-None-Match`.

### `scratch`

`copy(app, src, snapshot) -> Result<PathBuf>`: snapshots the database at `src` into
`<temp dir>/<app>-scratch-<timestamp>-<pid>/`, mode 0700, and returns the copy's path, for a run
that must not touch the real database. `snapshot` is the closure `backup` takes. A missing `src` is
an error rather than an empty copy. The directory is removed if the snapshot fails and left behind
if it succeeds, so the copy can be inspected after the run.

### `cli`

`cli::CommonArgs` is the `--db`, `--scratch`, `--today`, and `--config` flags, flattened into an
application's own `Cli`. `db_path` takes the default path as a closure, called only under
`--scratch` or with no flag, and resolves `--scratch` through `scratch::copy`; `is_default_db`
says whether the backup schedule applies to the run. `config_path` is `--config` or the
application's default, and `today_or_local` is `--today` or the local date. `is_scratch_session`
says the run is on another database or another day; it is also true for `--today` alone, so it is
not the complement of `is_default_db`.

`scratch_dir(db)` is the directory a `--scratch` copy sits in, where that run writes what it would
otherwise write beside the real database, and `None` for any other run.

The flags' help would otherwise be the doc comments on `CommonArgs`, so `--db` would not name the
application's default path. `name_defaults(cmd, app, db_file, writes_report)` rewrites the help of
`--db`, `--config` and `--scratch` to say where this application's files are, written as `~/...`
because help is read on machines with other homes; `writes_report` adds where a scratch run's page
goes. `parse::<Cli>(app, db_file, writes_report)` is `Cli::parse()` with it applied. Both panic
when the `Cli` does not flatten `CommonArgs`. The application's `Cli` should set its own `about`:
otherwise `CommonArgs`'s doc comment becomes the `--help` description.

### `sqlite`

- `Schema { baseline, seed, chain, remedy }`: the frozen version-1 SQL, rows a new database starts
  with, the `Migration { version, sql, data }` arms above it, and what else the owner can do with a
  database this build will not migrate. `head()` is one plus the chain's length; `check_versions`
  is for an application's test that each arm declares the version its position gives it.
- `open(path, &schema)` creates the parent directory, turns on foreign keys and WAL, and migrates;
  `open_in_memory(&schema)` is the same for tests, so each replays the whole chain.
- `migrate` runs the chain in one transaction with foreign keys off, checks
  `pragma_foreign_key_check` at the end, and refuses a database newer than the build.
- `snapshot(src, dest)` is `VACUUM INTO` without migrating, for `backup` and `scratch::copy`.
- `row_id!(AccountId, "account")` defines a typed id for one table, so ids of different tables
  cannot stand in for one another. It binds and reads as its `i64`, displays as the bare number, and
  expands to `rusqlite` through this module, so the caller never names it.

### `tui`

- `centered(area, width, height) -> Rect` and `is_press(&KeyEvent) -> bool`.
- `step_index(index, len, step)` steps through `len` choices, wrapping, and stays at zero for an
  empty list; `next_in(order, focus, step)` is the same around a form's tab order over its field
  enum, counting from the first when `focus` is not in `order`, and panicking on an empty `order`.
- `app::{App, run}`: `run` owns the terminal and the loop that draws and reads keys, and hands
  the `App` back on quit. It draws only when a key press, a resize, an expired status message
  (`expire_status`) or deferred work (`run_deferred`, which defaults to none) changed something,
  and checks for expiry every quarter second.
- `status::StatusLine`: the footer's message, error or not. With no modal open it lasts until the
  next key or `status::TTL` (four seconds); under a modal, until the modal closes. The application
  brackets each key with `begin_key`/`end_key` and passes whether a modal is open.
- `text::{TextBuffer, Edit, edit_key, is_bare}`: a line of text with a caret, and the Ctrl-key
  editing shared by every text box. `TextBuffer` has `value`, `caret`, `len`, `is_empty`, `set`,
  `clear`, `insert`, `backspace`, `delete`, `step`, `start`, `end`, `delete_word_back`,
  `kill_to_start` and `kill_to_end`.
- `date::{iso, parse_shorthand, parse, Step}`: `iso` formats `YYYY-MM-DD`; `parse_shorthand`
  resolves `M/D` against a reference day, where the year turns on the month alone. `date::parse`
  reads `YYYY-MM-DD` or `M/D`; `date::normalized` writes a date out as `YYYY-MM-DD`
  and is `None` for text that is not one; `resolved` is that only when it differs from what was
  typed; `stepped(raw, today, step)` nudges a date already there and is `None` otherwise;
  `parse_opt` is `parse` where blank is `Ok(None)`. `date::Step` is what `←`/`→` (a day), `Shift` with them (a week),
  and `[`/`]` (a month) do to a date field, via `Step::from_key`.
- `help::Entry` tables, each entry's footer `Label`, drive both the footer (`footer_items`, joined
  by the application) and the `?` panel (`render_panel`); `duplicate_keys` finds keys a table
  binds twice.

### `test-support`

- `testing::{day, unreachable_aws}`: `day(y, m, d)` is a `NaiveDate` that panics on one that does
  not exist; `unreachable_aws(&mut Command)` gives a child process an AWS environment that reaches
  nothing, for a test of a backup command that must fail before uploading.
- `tui::testing::{key, shift, ctrl, draw_buffer, draw, buffer_text, press, type_text, screen,
  inside}`: `press` and `type_text` feed an `App` keys, `screen` draws it at a size, and `inside`
  is the rows inside a bordered screen, the border's sides and trailing spaces removed.

For an application's tests. Enable it from `[dev-dependencies]` only.

## Development

See `AGENTS.md`.
