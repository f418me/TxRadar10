pub mod schema;

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::tags::AddressTag;

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
