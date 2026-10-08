//! Opening, migrating and copying an application's SQLite database.
//!
//! A schema is a frozen baseline at version 1 and a chain of arms above it.
//! A fresh database takes the baseline, the seed, and then the whole chain --
//! the same SQL, in the same order, that an existing database takes the tail
//! of -- so a test suite that builds its databases through
//! [`open_in_memory`] replays the chain on every run.

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::Path;

/// One edit to the schema, and the version it leaves a database at.
///
/// An arm names the schema as it stood when the arm was written, and must
/// survive replaying against an empty database, which is what a fresh
/// install is. A change of meaning with no change of shape is an arm with a
/// `data` half and empty `sql`.
#[derive(Clone, Copy)]
pub struct Migration {
    /// The version this arm leaves the database at: its position in the
    /// chain plus two. [`Schema::check_versions`] holds the two together.
    pub version: i64,
    /// Run as a batch, so several statements separated by `;` are fine.
    pub sql: &'static str,
    /// Run after `sql`, inside the same transaction: the column a data half
    /// writes to does not exist until its own `sql` has made room.
    pub data: Option<fn(&Connection) -> Result<()>>,
}

/// An application's schema: everything [`migrate`] needs to bring a database
/// from any version this build understands to the head.
#[derive(Clone, Copy)]
pub struct Schema<'a> {
    /// The schema at version 1. Never edited to describe a change: editing
    /// it would give a fresh database a schema no existing one can reach.
    pub baseline: &'a str,
    /// Rows a database being created starts with, written against the
    /// version-1 schema, so it runs before any arm. Empty for none.
    pub seed: &'a str,
    /// Every change since version 1, in order. The head is one plus its
    /// length, so appending an arm is the whole change.
    pub chain: &'a [Migration],
    /// What else the owner can do with a database this build will not
    /// migrate, after "open it with the build that wrote it" or "restore the
    /// most recent backup": `delete the file and re-run the import`.
    pub remedy: Option<&'a str>,
}

impl Schema<'_> {
    /// The version this schema's chain leaves a database at.
    pub const fn head(&self) -> i64 {
        1 + self.chain.len() as i64
    }

    /// Fail naming the first arm whose declared version is not the one its
    /// position gives it, for an application's own test of its chain.
    pub fn check_versions(&self) -> Result<()> {
        for (index, arm) in self.chain.iter().enumerate() {
            let expected = index as i64 + 2;
            anyhow::ensure!(
                arm.version == expected,
                "the arm at index {index} declares version {}, not {expected}",
                arm.version
            );
        }
        Ok(())
    }
}

/// Open (creating if needed) the database at `path`, creating its parent
/// directory if missing, and bring it up to `schema`'s head with foreign keys
/// enforced and the journal in WAL mode.
pub fn open(path: &Path, schema: &Schema) -> Result<Connection> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let conn = Connection::open(path)
        .with_context(|| format!("opening database at {}", path.display()))?;
    prepare(&conn, schema).with_context(|| format!("preparing database at {}", path.display()))?;
    Ok(conn)
}

/// [`open`], in memory: what every test's database is built through, so each
/// one replays the whole chain.
pub fn open_in_memory(schema: &Schema) -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    prepare(&conn, schema)?;
    Ok(conn)
}

fn prepare(conn: &Connection, schema: &Schema) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    migrate(conn, schema)
}

/// Copy the database at `src` to `dest`, which must not exist yet: what
/// `backup` and `scratch::copy` take as their snapshot.
///
/// `VACUUM INTO` reads one consistent snapshot, WAL included, while another
/// connection has the file open. `src` is opened without the create flag, so
/// a wrong path is an error rather than an empty database copied as though it
/// were the real one, and without [`migrate`], so a copy keeps the version
/// the original has.
pub fn snapshot(src: &Path, dest: &Path) -> Result<()> {
    let dest = dest
        .to_str()
        .with_context(|| format!("{} is not valid UTF-8", dest.display()))?;
    let conn = Connection::open_with_flags(
        src,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("opening database at {}", src.display()))?;
    conn.execute("VACUUM INTO ?1", [dest])
        .with_context(|| format!("snapshotting to {dest}"))?;
    Ok(())
}

/// Bring `conn` from whatever version it is at to `schema`'s head, in one
/// transaction.
///
/// Version 0 is an empty file: it takes the baseline, the seed and every
/// arm. A version above the head was written by a later build and is refused
/// rather than half-understood. `PRAGMA user_version` is transactional, so a
/// failure partway leaves the database at the version it came in at.
///
/// The chain runs with foreign keys off, since an arm that rebuilds a table
/// drops one that others reference, and `PRAGMA foreign_key_check` stands in
/// for them at the end. That costs `ON DELETE CASCADE`: an arm that deletes a
/// parent deletes its children itself. The pragma is a no-op inside a
/// transaction, so this refuses to run inside one, and puts enforcement back
/// however the chain ends.
pub fn migrate(conn: &Connection, schema: &Schema) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let head = schema.head();
    if current == head {
        return Ok(());
    }
    if current > head {
        anyhow::bail!(
            "database is at schema version {current}, newer than this build ({head}); \
             open it with the build that wrote it{}",
            or_remedy(schema)
        );
    }
    let enforcing: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    conn.pragma_update(None, "foreign_keys", false)?;
    // SQLite ignores the pragma inside a transaction without an error, so
    // the switch is read back.
    let off: bool = !conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    anyhow::ensure!(
        off,
        "the schema chain cannot run inside a transaction; open the database \
         through sqlite::open or sqlite::open_in_memory, which migrate before \
         anything else touches the connection"
    );
    let migrated = run_chain(conn, schema, current, head);
    let restored = conn.pragma_update(None, "foreign_keys", enforcing);
    // The chain's own failure first: it says what went wrong.
    migrated?;
    restored?;
    Ok(())
}

fn run_chain(conn: &Connection, schema: &Schema, current: i64, head: i64) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    if current == 0 {
        tx.execute_batch(schema.baseline)?;
        tx.execute_batch(schema.seed)?;
    }
    for arm in schema.chain.iter().filter(|arm| arm.version > current) {
        tx.execute_batch(arm.sql)?;
        if let Some(data) = arm.data {
            data(&tx)?;
        }
    }
    check_references(&tx, schema, head)?;
    tx.pragma_update(None, "user_version", head)?;
    tx.commit()?;
    Ok(())
}

/// Refuse a chain that has left a row pointing at one that is not there.
/// Whole-database scope, so an orphan written before this check existed
/// stops the next upgrade too: the message names the way out.
fn check_references(conn: &Connection, schema: &Schema, head: i64) -> Result<()> {
    let orphan: Option<(String, String)> = conn
        .query_row(
            "SELECT \"table\", \"parent\" FROM pragma_foreign_key_check",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((table, parent)) = orphan {
        anyhow::bail!(
            "migrating to schema version {head} would leave a row in {table} naming a \
             {parent} that is not there; the database is unchanged. Restore the most \
             recent backup{}",
            or_remedy(schema)
        );
    }
    Ok(())
}

fn or_remedy(schema: &Schema) -> String {
    schema
        .remedy
        .map(|r| format!(", or {r}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const BASELINE: &str = "CREATE TABLE t (a INTEGER)";

    const ONE_ARM: &[Migration] = &[Migration {
        version: 2,
        sql: "ALTER TABLE t ADD COLUMN b INTEGER",
        data: None,
    }];

    fn schema(chain: &[Migration]) -> Schema<'_> {
        Schema {
            baseline: BASELINE,
            seed: "",
            chain,
            remedy: None,
        }
    }

    fn version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    fn columns(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM pragma_table_info('t')")
            .unwrap();
        let found = stmt.query_map([], |r| r.get(0)).unwrap();
        found.collect::<rusqlite::Result<Vec<String>>>().unwrap()
    }

    fn foreign_keys(conn: &Connection) -> bool {
        conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap()
    }

    /// A database at version 1 with the baseline and one row in it.
    fn at_version_one() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(BASELINE).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn
    }

    /// A fresh directory named for the test, so tests running at once in one
    /// process never share one.
    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "finance_utils_sqlite_{label}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn an_empty_database_takes_the_baseline_and_then_the_whole_chain() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn, &schema(ONE_ARM)).unwrap();
        assert_eq!(version(&conn), 2);
        assert_eq!(columns(&conn), ["a", "b"]);
    }

    #[test]
    fn an_empty_database_takes_the_seed_before_any_arm() {
        let conn = Connection::open_in_memory().unwrap();
        let seeded = Schema {
            seed: "INSERT INTO t (a) VALUES (7)",
            ..schema(ONE_ARM)
        };
        migrate(&conn, &seeded).unwrap();
        let a: i64 = conn.query_row("SELECT a FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(a, 7);
    }

    #[test]
    fn a_database_at_the_head_version_is_left_alone() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn, &schema(ONE_ARM)).unwrap();
        migrate(&conn, &schema(ONE_ARM)).unwrap();
        assert_eq!(version(&conn), 2);
        assert_eq!(columns(&conn), ["a", "b"]);
    }

    #[test]
    fn a_database_at_version_one_takes_the_chain_and_neither_the_baseline_nor_the_seed() {
        let conn = at_version_one();
        let seeded = Schema {
            seed: "INSERT INTO t (a) VALUES (7)",
            ..schema(ONE_ARM)
        };
        migrate(&conn, &seeded).unwrap();
        assert_eq!(version(&conn), 2);
        assert_eq!(columns(&conn), ["a", "b"]);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused_and_left_unwritten() {
        let conn = at_version_one();
        conn.pragma_update(None, "user_version", 3).unwrap();
        let err = migrate(&conn, &schema(ONE_ARM)).unwrap_err().to_string();
        assert!(err.contains("version 3"), "{err}");
        assert!(err.contains("newer than this build"), "{err}");
        assert!(err.ends_with("the build that wrote it"), "{err}");
        assert_eq!(version(&conn), 3);
    }

    #[test]
    fn a_refusal_names_the_remedy_the_schema_gives() {
        let conn = at_version_one();
        conn.pragma_update(None, "user_version", 3).unwrap();
        let with_remedy = Schema {
            remedy: Some("delete the file and re-import"),
            ..schema(ONE_ARM)
        };
        let err = migrate(&conn, &with_remedy).unwrap_err().to_string();
        assert!(
            err.ends_with("the build that wrote it, or delete the file and re-import"),
            "{err}"
        );
    }

    #[test]
    fn the_chain_refuses_to_run_with_a_transaction_already_open() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.execute_batch("BEGIN").unwrap();
        let err = migrate(&conn, &schema(ONE_ARM)).unwrap_err().to_string();
        assert!(err.contains("cannot run inside a transaction"), "{err}");
    }

    fn double_a_into_b(conn: &Connection) -> Result<()> {
        conn.execute("UPDATE t SET b = a * 2", [])?;
        Ok(())
    }

    #[test]
    fn an_arms_data_half_runs_after_its_sql() {
        const WITH_DATA: &[Migration] = &[Migration {
            version: 2,
            sql: "ALTER TABLE t ADD COLUMN b INTEGER",
            data: Some(double_a_into_b),
        }];
        let conn = at_version_one();
        conn.execute("INSERT INTO t (a) VALUES (21)", []).unwrap();
        migrate(&conn, &schema(WITH_DATA)).unwrap();
        let b: i64 = conn.query_row("SELECT b FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(b, 42);
    }

    #[test]
    fn an_arm_that_fails_leaves_the_version_and_the_schema_untouched() {
        const SECOND_ARM_IS_BROKEN: &[Migration] = &[
            Migration {
                version: 2,
                sql: "ALTER TABLE t ADD COLUMN b INTEGER",
                data: None,
            },
            Migration {
                version: 3,
                sql: "ALTER TABLE nonexistent ADD COLUMN c INTEGER",
                data: None,
            },
        ];
        let conn = at_version_one();
        migrate(&conn, &schema(SECOND_ARM_IS_BROKEN)).unwrap_err();
        assert_eq!(version(&conn), 1);
        assert_eq!(columns(&conn), ["a"]);
    }

    #[test]
    fn foreign_key_enforcement_is_put_back_however_the_chain_ends() {
        const BROKEN: &[Migration] = &[Migration {
            version: 2,
            sql: "ALTER TABLE nonexistent ADD COLUMN c INTEGER",
            data: None,
        }];
        let conn = at_version_one();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        migrate(&conn, &schema(ONE_ARM)).unwrap();
        assert!(foreign_keys(&conn));

        let conn = at_version_one();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        migrate(&conn, &schema(BROKEN)).unwrap_err();
        assert!(foreign_keys(&conn));
    }

    #[test]
    fn an_arm_may_rebuild_a_table_that_others_reference() {
        const BASE: &str = "CREATE TABLE p (id INTEGER PRIMARY KEY); \
                            CREATE TABLE c (p_id INTEGER REFERENCES p(id)); \
                            INSERT INTO p (id) VALUES (1); INSERT INTO c (p_id) VALUES (1);";
        const REBUILD: &[Migration] = &[Migration {
            version: 2,
            sql: "CREATE TABLE p_new (id INTEGER PRIMARY KEY, name TEXT); \
                  INSERT INTO p_new (id) SELECT id FROM p; \
                  DROP TABLE p; ALTER TABLE p_new RENAME TO p;",
            data: None,
        }];
        let conn = open_in_memory(&Schema {
            baseline: BASE,
            ..schema(&[])
        })
        .unwrap();
        migrate(
            &conn,
            &Schema {
                baseline: BASE,
                ..schema(REBUILD)
            },
        )
        .unwrap();
        assert_eq!(version(&conn), 2);
    }

    #[test]
    fn a_chain_that_leaves_an_orphan_is_refused_and_taken_back() {
        const BASE: &str = "CREATE TABLE p (id INTEGER PRIMARY KEY); \
                            CREATE TABLE c (p_id INTEGER REFERENCES p(id)); \
                            INSERT INTO p (id) VALUES (1); INSERT INTO c (p_id) VALUES (1);";
        const ORPHANS: &[Migration] = &[Migration {
            version: 2,
            sql: "DELETE FROM p",
            data: None,
        }];
        let conn = open_in_memory(&Schema {
            baseline: BASE,
            ..schema(&[])
        })
        .unwrap();
        let err = migrate(
            &conn,
            &Schema {
                baseline: BASE,
                remedy: Some("re-import"),
                ..schema(ORPHANS)
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("a row in c naming a p"), "{err}");
        assert!(err.ends_with("most recent backup, or re-import"), "{err}");
        assert_eq!(version(&conn), 1);
        let parents: i64 = conn
            .query_row("SELECT COUNT(*) FROM p", [], |r| r.get(0))
            .unwrap();
        assert_eq!(parents, 1);
    }

    #[test]
    fn an_arm_declaring_a_version_its_position_does_not_give_it_is_reported() {
        const SKIPS: &[Migration] = &[Migration {
            version: 3,
            sql: "",
            data: None,
        }];
        assert!(schema(ONE_ARM).check_versions().is_ok());
        let err = schema(SKIPS).check_versions().unwrap_err().to_string();
        assert!(err.contains("declares version 3, not 2"), "{err}");
    }

    #[test]
    fn opening_a_file_creates_its_parent_directory_and_enforces_foreign_keys() {
        let dir = scratch("parent");
        let path = dir.join("nested").join("app.db");
        let conn = open(&path, &schema(ONE_ARM)).unwrap();
        assert!(path.exists());
        assert!(foreign_keys(&conn));
        assert_eq!(version(&conn), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_error_opening_a_file_names_its_path() {
        let dir = scratch("newer");
        let path = dir.join("app.db");
        drop(open(&path, &schema(ONE_ARM)).unwrap());
        let err = open(&path, &schema(&[])).unwrap_err();
        assert!(
            format!("{err:#}").contains(&path.display().to_string()),
            "{err:#}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_snapshot_holds_writes_still_in_the_wal_of_an_open_database() {
        let dir = scratch("snapshot");
        let path = dir.join("app.db");
        let conn = open(&path, &schema(ONE_ARM)).unwrap();
        conn.execute("INSERT INTO t (a, b) VALUES (1, 2)", [])
            .unwrap();
        let copy = dir.join("copy.db");
        snapshot(&path, &copy).unwrap();
        let rows: i64 = Connection::open(&copy)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshotting_a_path_with_no_database_is_an_error_rather_than_an_empty_copy() {
        let dir = scratch("missing");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(snapshot(&dir.join("missing.db"), &dir.join("copy.db")).is_err());
        assert!(!dir.join("missing.db").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
