//! Load Tessera partitions into DuckDB relations.
//!
//! Uses DuckDB's `httpfs` to range-read the presigned Parquet directly; the
//! returned connection carries a `tessera` view that can be filtered and
//! aggregated in SQL with predicate pushdown.

use duckdb::Connection;

use super::ResolvedPartition;
use crate::error::TesseraError;

/// Quote a value as a SQL string literal.
fn sql_str(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Build an in-memory DuckDB connection with a `tessera` view over the
/// resolved partitions.
///
/// For multi-partition reads each leaf is unioned with a `coin` and `month`
/// column identifying the source partition. Query it with
/// `SELECT ... FROM tessera`.
///
/// # Errors
///
/// Returns [`TesseraError::Network`] when the connection or view creation fails.
pub fn build_relation(
    parts: &[ResolvedPartition],
    columns: Option<&[&str]>,
) -> Result<Connection, TesseraError> {
    let connection =
        Connection::open_in_memory().map_err(|err| TesseraError::Network(err.to_string()))?;

    // Recent DuckDB autoloads httpfs for https paths; load explicitly but
    // don't fail if the environment can't reach the extension repository.
    let _ = connection.execute_batch("INSTALL httpfs; LOAD httpfs;");

    let multi = parts.len() > 1;
    let select_cols = columns.map_or_else(|| "*".to_string(), |columns| columns.join(", "));
    let selects: Vec<String> = parts
        .iter()
        .map(|(partition, url)| {
            let projection = if multi {
                format!(
                    "{}, {} AS coin, {} AS month",
                    select_cols,
                    sql_str(&partition.coin),
                    sql_str(&partition.month)
                )
            } else {
                select_cols.clone()
            };
            format!("SELECT {projection} FROM read_parquet({})", sql_str(url))
        })
        .collect();
    let query = selects.join("\nUNION ALL\n");
    connection
        .execute_batch(&format!("CREATE OR REPLACE TEMP VIEW tessera AS {query}"))
        .map_err(|err| TesseraError::Network(err.to_string()))?;
    Ok(connection)
}
