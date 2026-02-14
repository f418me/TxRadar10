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
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            txid                TEXT NOT NULL,
            score               REAL NOT NULL,
            alert_level         TEXT NOT NULL,
            rule_scores         TEXT, -- JSON
            to_exchange         INTEGER NOT NULL DEFAULT 0,
            total_input_value   INTEGER NOT NULL DEFAULT 0,
            fee_rate            REAL NOT NULL DEFAULT 0.0,
            coin_days_destroyed REAL,
            block_height_seen   INTEGER NOT NULL DEFAULT 0,
            created_at          TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_signals_score ON signals(score DESC);
        CREATE INDEX IF NOT EXISTS idx_signals_created ON signals(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_utxo_cache_height ON utxo_cache(block_height);

        CREATE TABLE IF NOT EXISTS address_tags (
            address     TEXT PRIMARY KEY,
            entity      TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            confidence  REAL DEFAULT 0.5,
            source      TEXT,
            updated_at  TEXT
        );
        ",
    )?;

    // Add columns if they don't exist (migration for existing DBs)
    let cols = [
        "to_exchange INTEGER NOT NULL DEFAULT 0",
        "total_input_value INTEGER NOT NULL DEFAULT 0",
        "fee_rate REAL NOT NULL DEFAULT 0.0",
        "coin_days_destroyed REAL",
        "block_height_seen INTEGER NOT NULL DEFAULT 0",
    ];
    for col_def in &cols {
        let _col_name = col_def.split_whitespace().next().unwrap();
        let sql = format!("ALTER TABLE signals ADD COLUMN {col_def}");
        // Ignore error if column already exists
        match conn.execute_batch(&sql) {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    // Ignore — column already exists
                }
                let _ = msg;
            }
        }
    }

    Ok(())
}
