use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::types::JournalEvent;

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    Ok(serde_json::from_reader(file)?)
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = path.with_extension("tmp");
    let mut file = File::create(&tmp)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    drop(file);
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn write_text_file(path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let mut file = File::create(&tmp)?;
    file.write_all(contents.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()?;
    drop(file);
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn append_event(path: &Path, event: &JournalEvent) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    Ok(())
}

pub fn emit_report(contents: &str, out: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = out {
        write_text_file(path, contents)?;
        println!("OK: wrote report to {}", path.display());
    } else {
        println!("{contents}");
    }
    Ok(())
}

pub fn truncate_text(text: &str, max_chars: usize) -> String {
    let truncated: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        format!("{truncated}...")
    } else {
        truncated
    }
}

pub fn now_epoch_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn detect_habitat() -> &'static str {
    if cfg!(target_os = "windows") {
        "host_windows"
    } else if cfg!(target_os = "macos") {
        "host_macos"
    } else if cfg!(target_os = "linux") {
        "host_linux"
    } else {
        "host_unknown"
    }
}

pub fn read_key_value_file(path: &Path) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut values = BTreeMap::new();
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    Ok(values)
}

pub fn present_absent(present: bool) -> &'static str {
    if present {
        "present"
    } else {
        "missing"
    }
}

pub fn read_recent_events(path: &Path, count: usize) -> Result<Vec<JournalEvent>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: JournalEvent = serde_json::from_str(&line)?;
        events.push(event);
    }
    let start = events.len().saturating_sub(count);
    Ok(events.into_iter().skip(start).collect())
}

pub fn read_all_events(path: &Path) -> Result<Vec<JournalEvent>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: JournalEvent = serde_json::from_str(&line)?;
        events.push(event);
    }
    Ok(events)
}

/// Metadata returned alongside the fingerprint hash.
pub struct FingerprintResult {
    pub fingerprint: String,
    pub event_count: usize,
    pub first_event_ts: Option<u64>,
    pub last_event_ts: Option<u64>,
    pub span_s: u64,
}

/// Compute a rolling SHA-256 fingerprint over all journal events in order.
/// Each event contributes: `event_id|ts_epoch_s|kind|continuity_epoch\n`
pub fn compute_fingerprint(journal_path: &Path) -> Result<FingerprintResult, Box<dyn std::error::Error>> {
    let events = read_all_events(journal_path)?;
    let mut hasher = Sha256::new();
    for event in &events {
        let line = format!(
            "{}|{}|{}|{}\n",
            event.event_id, event.ts_epoch_s, event.kind, event.continuity_epoch
        );
        hasher.update(line.as_bytes());
    }
    let bytes = hasher.finalize();
    let fingerprint: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();

    let first = events.first().map(|e| e.ts_epoch_s);
    let last = events.last().map(|e| e.ts_epoch_s);
    let span_s = match (first, last) {
        (Some(f), Some(l)) => l.saturating_sub(f),
        _ => 0,
    };

    Ok(FingerprintResult {
        fingerprint,
        event_count: events.len(),
        first_event_ts: first,
        last_event_ts: last,
        span_s,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JournalEvent;
    use std::env;
    use uuid::Uuid;

    #[test]
    fn truncate_text_adds_ellipsis_when_needed() {
        let text = truncate_text("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(text, "abcdefghij...");
    }

    #[test]
    fn write_text_file_persists_report_contents() {
        let dir = env::temp_dir().join(format!("oo-host-test-out-{}", Uuid::new_v4()));
        let path = dir.join("report.md");

        write_text_file(&path, "# report\nhello").expect("write report");
        let saved = std::fs::read_to_string(&path).expect("read report");
        assert!(saved.contains("# report"));
        assert!(saved.contains("hello"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn write_events_to_file(path: &std::path::Path, events: &[JournalEvent]) {
        use std::io::Write as _;
        let mut f = std::fs::File::create(path).unwrap();
        for e in events {
            serde_json::to_writer(&mut f, e).unwrap();
            f.write_all(b"\n").unwrap();
        }
    }

    fn sample_event(n: u64) -> JournalEvent {
        JournalEvent {
            event_id: format!("event-{n}"),
            ts_epoch_s: n * 100,
            organism_id: "org-fp".to_string(),
            runtime_habitat: "host_test".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "startup".to_string(),
            severity: "info".to_string(),
            summary: format!("event {n}"),
            reason: None,
            action: None,
            result: None,
            continuity_epoch: 0,
            signature: None,
        }
    }

    #[test]
    fn compute_fingerprint_identical_sequences_produce_same_hash() {
        let dir = env::temp_dir().join(format!("oo-fp-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p1 = dir.join("j1.jsonl");
        let p2 = dir.join("j2.jsonl");
        let events: Vec<JournalEvent> = (1..=5).map(sample_event).collect();
        write_events_to_file(&p1, &events);
        write_events_to_file(&p2, &events);
        let r1 = compute_fingerprint(&p1).unwrap();
        let r2 = compute_fingerprint(&p2).unwrap();
        assert_eq!(r1.fingerprint, r2.fingerprint);
        assert_eq!(r1.event_count, 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compute_fingerprint_changes_when_event_added() {
        let dir = env::temp_dir().join(format!("oo-fp-test2-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p1 = dir.join("j1.jsonl");
        let p2 = dir.join("j2.jsonl");
        let events_short: Vec<JournalEvent> = (1..=3).map(sample_event).collect();
        let events_long: Vec<JournalEvent> = (1..=4).map(sample_event).collect();
        write_events_to_file(&p1, &events_short);
        write_events_to_file(&p2, &events_long);
        let r1 = compute_fingerprint(&p1).unwrap();
        let r2 = compute_fingerprint(&p2).unwrap();
        assert_ne!(r1.fingerprint, r2.fingerprint);
        assert_eq!(r1.event_count, 3);
        assert_eq!(r2.event_count, 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compute_fingerprint_empty_journal_returns_empty_hash() {
        let dir = env::temp_dir().join(format!("oo-fp-test3-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.jsonl");
        std::fs::write(&path, "").unwrap();
        let r = compute_fingerprint(&path).unwrap();
        assert_eq!(r.event_count, 0);
        assert_eq!(r.fingerprint.len(), 64); // SHA-256 hex is always 64 chars
        let _ = std::fs::remove_dir_all(&dir);
    }
}
