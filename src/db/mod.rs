pub mod schema;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::tags::AddressTag;

/// A persisted signal record from the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalRecord {
    pub id: i64,
    pub txid: String,
    pub score: f64,
    pub alert_level: String,
    pub rule_scores_json: String,
    pub to_exchange: bool,
    pub total_input_value: u64,
    pub fee_rate: f64,
    pub coin_days_destroyed: Option<f64>,
    pub block_height_seen: u32,
    pub created_at: String,
}

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

    /// Look up an address tag.
    pub fn lookup_address(&self, address: &str) -> Option<AddressTag> {
        let db = self.inner.lock().unwrap();
        db.lookup_address(address)
    }

    /// Insert an address tag.
    pub fn insert_tag(&self, tag: &AddressTag) -> Result<(), rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.insert_tag(tag)
    }

    /// Bulk-load tags from a CSV file.
    pub fn load_tags_from_csv(&self, path: &Path) -> Result<usize, Box<dyn std::error::Error>> {
        let db = self.inner.lock().unwrap();
        db.load_tags_from_csv(path)
    }

    /// Load all address tags from DB.
    pub fn all_tags(&self) -> Result<Vec<AddressTag>, rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.all_tags()
    }

    /// Store a signal for history (extended version).
    pub fn store_signal(
        &self,
        txid: &str,
        score: f64,
        alert_level: &str,
        rule_scores_json: &str,
        to_exchange: bool,
        total_input_value: u64,
        fee_rate: f64,
        coin_days_destroyed: Option<f64>,
        block_height_seen: u32,
    ) -> Result<(), rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.store_signal(txid, score, alert_level, rule_scores_json, to_exchange, total_input_value, fee_rate, coin_days_destroyed, block_height_seen)
    }

    /// Batch-store multiple signals in a single transaction.
    pub fn store_signals_batch(
        &self,
        signals: &[SignalBatchEntry],
    ) -> Result<(), rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.store_signals_batch(signals)
    }

    /// Get recent signals ordered by time.
    pub fn get_recent_signals(&self, limit: usize) -> Result<Vec<SignalRecord>, rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.get_recent_signals(limit)
    }

    /// Get signals with score above threshold.
    pub fn get_signals_above_score(&self, min_score: f64, limit: usize) -> Result<Vec<SignalRecord>, rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.get_signals_above_score(min_score, limit)
    }

    /// Get total signal count.
    pub fn get_signal_count(&self) -> Result<usize, rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.get_signal_count()
    }

    /// Get signals within a time range.
    pub fn get_signals_by_timerange(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<SignalRecord>, rusqlite::Error> {
        let db = self.inner.lock().unwrap();
        db.get_signals_by_timerange(from, to)
    }
}

/// Entry for batch insertion.
pub struct SignalBatchEntry {
    pub txid: String,
    pub score: f64,
    pub alert_level: String,
    pub rule_scores_json: String,
    pub to_exchange: bool,
    pub total_input_value: u64,
    pub fee_rate: f64,
    pub coin_days_destroyed: Option<f64>,
    pub block_height_seen: u32,
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

    /// Look up an address tag.
    pub fn lookup_address(&self, address: &str) -> Option<AddressTag> {
        let mut stmt = self.conn.prepare(
            "SELECT address, entity, entity_type, confidence, source FROM address_tags WHERE address = ?1",
        ).ok()?;
        let mut rows = stmt.query(rusqlite::params![address]).ok()?;
        if let Some(row) = rows.next().ok()? {
            Some(AddressTag {
                address: row.get(0).ok()?,
                entity: row.get(1).ok()?,
                entity_type: row.get(2).ok()?,
                confidence: row.get(3).ok()?,
                source: row.get(4).ok()?,
            })
        } else {
            None
        }
    }

    /// Insert an address tag.
    pub fn insert_tag(&self, tag: &AddressTag) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT OR REPLACE INTO address_tags (address, entity, entity_type, confidence, source, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            rusqlite::params![tag.address, tag.entity, tag.entity_type, tag.confidence, tag.source],
        )?;
        Ok(())
    }

    /// Load all address tags from DB.
    pub fn all_tags(&self) -> Result<Vec<AddressTag>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT address, entity, entity_type, confidence, source FROM address_tags",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AddressTag {
                address: row.get(0)?,
                entity: row.get(1)?,
                entity_type: row.get(2)?,
                confidence: row.get(3)?,
                source: row.get(4)?,
            })
        })?;
        let mut tags = Vec::new();
        for tag in rows {
            tags.push(tag?);
        }
        Ok(tags)
    }

    /// Bulk-load tags from a CSV file.
    pub fn load_tags_from_csv(&self, path: &Path) -> Result<usize, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let mut count = 0;
        for line in content.lines().skip(1) {
            // skip header
            let parts: Vec<&str> = line.splitn(5, ',').collect();
            if parts.len() < 4 {
                continue;
            }
            let tag = AddressTag {
                address: parts[0].trim().to_string(),
                entity: parts[1].trim().to_string(),
                entity_type: parts[2].trim().to_string(),
                confidence: parts[3].trim().parse().unwrap_or(0.5),
                source: parts.get(4).map(|s| s.trim().to_string()),
            };
            self.insert_tag(&tag)?;
            count += 1;
        }
        Ok(count)
    }

    /// Store a signal for history/backtesting.
    pub fn store_signal(
        &self,
        txid: &str,
        score: f64,
        alert_level: &str,
        rule_scores_json: &str,
        to_exchange: bool,
        total_input_value: u64,
        fee_rate: f64,
        coin_days_destroyed: Option<f64>,
        block_height_seen: u32,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO signals (txid, score, alert_level, rule_scores, to_exchange, total_input_value, fee_rate, coin_days_destroyed, block_height_seen, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))",
            rusqlite::params![txid, score, alert_level, rule_scores_json, to_exchange as i32, total_input_value, fee_rate, coin_days_destroyed, block_height_seen],
        )?;
        Ok(())
    }

    /// Batch-store multiple signals in a single transaction.
    pub fn store_signals_batch(
        &self,
        signals: &[SignalBatchEntry],
    ) -> Result<(), rusqlite::Error> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO signals (txid, score, alert_level, rule_scores, to_exchange, total_input_value, fee_rate, coin_days_destroyed, block_height_seen, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))"
            )?;
            for s in signals {
                stmt.execute(rusqlite::params![
                    s.txid, s.score, s.alert_level, s.rule_scores_json,
                    s.to_exchange as i32, s.total_input_value, s.fee_rate,
                    s.coin_days_destroyed, s.block_height_seen
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn row_to_signal(row: &rusqlite::Row) -> rusqlite::Result<SignalRecord> {
        let to_ex: i32 = row.get(5)?;
        Ok(SignalRecord {
            id: row.get(0)?,
            txid: row.get(1)?,
            score: row.get(2)?,
            alert_level: row.get(3)?,
            rule_scores_json: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            to_exchange: to_ex != 0,
            total_input_value: row.get::<_, i64>(6)? as u64,
            fee_rate: row.get(7)?,
            coin_days_destroyed: row.get(8)?,
            block_height_seen: row.get::<_, i64>(9)? as u32,
            created_at: row.get(10)?,
        })
    }

    /// Get recent signals ordered by time.
    pub fn get_recent_signals(&self, limit: usize) -> Result<Vec<SignalRecord>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, txid, score, alert_level, rule_scores, to_exchange, total_input_value, fee_rate, coin_days_destroyed, block_height_seen, created_at
             FROM signals ORDER BY created_at DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(rusqlite::params![limit as i64], Self::row_to_signal)?;
        rows.collect()
    }

    /// Get signals with score above threshold.
    pub fn get_signals_above_score(&self, min_score: f64, limit: usize) -> Result<Vec<SignalRecord>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, txid, score, alert_level, rule_scores, to_exchange, total_input_value, fee_rate, coin_days_destroyed, block_height_seen, created_at
             FROM signals WHERE score >= ?1 ORDER BY score DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(rusqlite::params![min_score, limit as i64], Self::row_to_signal)?;
        rows.collect()
    }

    /// Get total signal count.
    pub fn get_signal_count(&self) -> Result<usize, rusqlite::Error> {
        self.conn.query_row("SELECT COUNT(*) FROM signals", [], |row| {
            row.get::<_, i64>(0).map(|c| c as usize)
        })
    }

    /// Get signals within a time range.
    pub fn get_signals_by_timerange(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<SignalRecord>, rusqlite::Error> {
        let from_str = from.format("%Y-%m-%d %H:%M:%S").to_string();
        let to_str = to.format("%Y-%m-%d %H:%M:%S").to_string();
        let mut stmt = self.conn.prepare(
            "SELECT id, txid, score, alert_level, rule_scores, to_exchange, total_input_value, fee_rate, coin_days_destroyed, block_height_seen, created_at
             FROM signals WHERE created_at >= ?1 AND created_at <= ?2 ORDER BY created_at DESC"
        )?;
        let rows = stmt.query_map(rusqlite::params![from_str, to_str], Self::row_to_signal)?;
        rows.collect()
    }
}
