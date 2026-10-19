//! Migrations of `host.db` (design 5.4.6, plan P-4). The scripts are
//! `migrations/NNNN_<name>.sql`, compiled in as [`MIGRATIONS`]. Script `NNNN`
//! brings the database to `schema_version` `NNNN`. Until T160, `0001` is the
//! only script and changes in place (P-4).

/// One migration script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    /// The file name without `.sql`: `NNNN_<name>`, where `NNNN` is the
    /// script's place in [`MIGRATIONS`], from 1.
    pub name: &'static str,
    /// Statements without transaction control.
    pub sql: &'static str,
}

/// The scripts compiled into the host, in order.
pub const MIGRATIONS: &[Migration] = &[Migration {
    name: "0001_initial",
    sql: include_str!("../../migrations/0001_initial.sql"),
}];

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use rusqlite::Connection;

    use super::*;
    use crate::store::testutil::{assert_same_schema, design_sql};

    #[test]
    fn store_schema_host_db_after_all_scripts_is_design_5_4_2() {
        let design = Connection::open_in_memory().unwrap();
        design.execute_batch(&design_sql("host.db")).unwrap();
        let migrated = Connection::open_in_memory().unwrap();
        for script in MIGRATIONS {
            migrated.execute_batch(script.sql).unwrap();
        }
        assert_same_schema(&migrated, &design, 16);
    }

    #[test]
    fn store_schema_scripts_are_numbered_in_order_and_all_listed() {
        let mut listed = Vec::new();
        for (place, script) in (1..).zip(MIGRATIONS) {
            let (number, name) = script.name.split_once('_').expect("`NNNN_<name>`");
            assert_eq!(number, format!("{place:04}"), "{}", script.name);
            assert!(!name.is_empty(), "{}", script.name);
            listed.push(format!("{}.sql", script.name));
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort();
        assert_eq!(files, listed);
    }
}
