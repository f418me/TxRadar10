use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS utxo_cache (
            txid        TEXT NOT NULL,
            vout        INTEGER NOT NULL,
            value       INTEGER NOT NULL,
            script_type TEXT NOT NULL,
            block_height INTEGER NOT NULL,
            block_time  INTEGER NOT NULL,
            PRIMARY KEY (txid, vout)
        );

        CREATE TABLE IF NOT EXISTS signals (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            txid        TEXT NOT NULL,
            score       REAL NOT NULL,
            alert_level TEXT NOT NULL,
            rule_scores TEXT, -- JSON
            created_at  TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_signals_score ON signals(score DESC);
        CREATE INDEX IF NOT EXISTS idx_signals_created ON signals(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_utxo_cache_height ON utxo_cache(block_height);
        ",
    )?;
    Ok(())
}
