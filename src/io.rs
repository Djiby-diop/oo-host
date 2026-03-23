use serde::{Deserialize, Serialize};
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


#[cfg(test)]
mod tests {
    use super::*;
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
}
