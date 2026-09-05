use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

const BASE_BACKOFF_SECS: u64 = 5;
const MAX_BACKOFF_SECS: u64 = 900; // 15 minute cap

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
pub fn is_locked(db: &Connection, key: &str) -> bool {
    let locked_until: Option<i64> = db
        .query_row(
            "SELECT locked_until FROM login_attempts WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .ok();

    match locked_until {
        Some(until) => (until as u64) > now_secs(),
        None => false,
    }
}

pub fn record_failure(db: &Connection, key: &str) {
    let failures: u32 = db
        .query_row(
            "SELECT failure_count FROM login_attempts WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let new_failures = failures + 1;
    let backoff_secs = BASE_BACKOFF_SECS
        .saturating_mul(2u64.saturating_pow(new_failures.saturating_sub(1)))
        .min(MAX_BACKOFF_SECS);
    let locked_until = now_secs() + backoff_secs;

    let _ = db.execute(
        "INSERT INTO login_attempts (key, failure_count, locked_until) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET failure_count = ?2, locked_until = ?3",
        rusqlite::params![key, new_failures, locked_until as i64],
    );
}

pub fn record_success(db: &Connection, key: &str) {
    let _ = db.execute("DELETE FROM login_attempts WHERE key = ?1", [key]);
}
