//! The schema of `raw.db` (design 5.4.6). Raw frames are diagnostics that
//! can be thrown away, so `raw.db` is not migrated: [`prepare_raw`] empties
//! and rebuilds a database at another version than this host's, which it
//! keeps in `PRAGMA user_version`.

use rusqlite::{Connection, TransactionBehavior};

/// A whole schema of `raw.db`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawSchema {
    /// Goes up with every change to `sql`.
    pub version: u32,
    pub sql: &'static str,
}

/// This host's schema.
pub const RAW_SCHEMA: RawSchema = RawSchema {
    version: 1,
    sql: include_str!("raw.sql"),
};

/// What [`prepare_raw`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawDb {
    /// The database was at the schema's version: kept as it was.
    Kept,
    /// It was at version `from`, 0 when new: emptied and rebuilt.
    Rebuilt { from: i64 },
}

/// Brings `raw.db`, open on `conn`, to `schema` (the host passes
/// [`RAW_SCHEMA`]) before the connection goes to the writer thread. Blocking
/// I/O, for startup.
pub fn prepare_raw(conn: &mut Connection, schema: &RawSchema) -> rusqlite::Result<RawDb> {
    let from: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if from == i64::from(schema.version) {
        return Ok(RawDb::Kept);
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let tables: Vec<String> = tx
        .prepare(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    for table in tables {
        tx.execute_batch(&format!("DROP TABLE \"{}\"", table.replace('"', "\"\"")))?;
    }
    tx.execute_batch(schema.sql)?;
    tx.pragma_update(None, "user_version", schema.version)?;
    tx.commit()?;
    Ok(RawDb::Rebuilt { from })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testutil::{assert_same_schema, design_sql};

    #[test]
    fn store_schema_raw_db_is_design_5_4_2() {
        let design = Connection::open_in_memory().unwrap();
        design.execute_batch(&design_sql("raw.db")).unwrap();
        let mut ours = Connection::open_in_memory().unwrap();
        prepare_raw(&mut ours, &RAW_SCHEMA).unwrap();
        assert_same_schema(&ours, &design, 1);
    }
}
