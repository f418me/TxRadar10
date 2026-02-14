pub mod schema;

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct Database {
    conn: Connection,
}

/// Thread-safe wrapper around Database.
#[derive(Clone)]
pub struct SharedDatabase {
    inner: Arc<Mutex<Database>>,
}

impl SharedDatabase {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let db = Database::open(path)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(db)),
        })
    }

    /// Look up cached UTXO metadata. Returns (value_sats, script_type, block_height, block_time).
    pub fn get_utxo(
        &self,
        txid: &str,
        vout: u32,
    ) -> Result<Option<(u64, String, u32, i64)>, rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.get_utxo(txid, vout)
    }

    /// Cache a resolved UTXO.
    pub fn cache_utxo(
        &self,
        txid: &str,
        vout: u32,
        value: u64,
        script_type: &str,
        block_height: u32,
        block_time: i64,
    ) -> Result<(), rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.cache_utxo(txid, vout, value, script_type, block_height, block_time)
    }

    /// Store a signal for history.
    #[allow(dead_code)]
    pub fn store_signal(
        &self,
        txid: &str,
        score: f64,
        alert_level: &str,
        rule_scores_json: &str,
    ) -> Result<(), rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.store_signal(txid, score, alert_level, rule_scores_json)
    }
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        schema::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// Cache a UTXO's metadata for fast prevout resolution.
    pub fn cache_utxo(
        &self,
        txid: &str,
        vout: u32,
        value: u64,
        script_type: &str,
        block_height: u32,
        block_time: i64,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT OR REPLACE INTO utxo_cache (txid, vout, value, script_type, block_height, block_time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![txid, vout, value, script_type, block_height, block_time],
        )?;
        Ok(())
    }

    /// Look up cached UTXO metadata.
    pub fn get_utxo(
        &self,
        txid: &str,
        vout: u32,
    ) -> Result<Option<(u64, String, u32, i64)>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT value, script_type, block_height, block_time FROM utxo_cache WHERE txid = ?1 AND vout = ?2",
        )?;
        let mut rows = stmt.query(rusqlite::params![txid, vout])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        } else {
            Ok(None)
        }
    }

    /// Store a signal for history/backtesting.
    #[allow(dead_code)]
    pub fn store_signal(
        &self,
        txid: &str,
        score: f64,
        alert_level: &str,
        rule_scores_json: &str,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO signals (txid, score, alert_level, rule_scores, created_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            rusqlite::params![txid, score, alert_level, rule_scores_json],
        )?;
        Ok(())
    }
}
