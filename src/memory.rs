use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::io::{append_event, now_epoch_s, read_all_events};
use crate::types::{JournalEvent, RuntimeCtx};

#[derive(Debug, Serialize, Deserialize)]
pub struct Memory {
    pub memory_id: String,
    pub ts_start: u64,
    pub ts_end: u64,
    pub event_count: usize,
    pub dominant_kind: String,
    pub summary: String,
    pub continuity_epoch: u64,
}

/// Group all journal events into time windows of `window_s` seconds.
/// For each window, create a Memory entry with the dominant event kind.
/// Appends memories to `<out_dir>/organism_memories.jsonl` and emits a journal event.
/// Returns the number of new memories written.
pub fn consolidate_journal(
    ctx: &RuntimeCtx,
    window_s: u64,
    out_dir: &Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let events = read_all_events(&ctx.paths.journal_path)?;
    if events.is_empty() {
        return Ok(0);
    }

    let total_events = events.len();

    // Partition events into time windows
    let mut windows: Vec<Vec<&JournalEvent>> = Vec::new();
    let mut current_window: Vec<&JournalEvent> = Vec::new();
    let mut window_start = events[0].ts_epoch_s;

    for event in &events {
        if event.ts_epoch_s > window_start + window_s && !current_window.is_empty() {
            windows.push(current_window);
            current_window = Vec::new();
            window_start = event.ts_epoch_s;
        }
        current_window.push(event);
    }
    if !current_window.is_empty() {
        windows.push(current_window);
    }

    std::fs::create_dir_all(out_dir)?;
    let memories_path = out_dir.join("organism_memories.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(&memories_path)?;

    let mut memories_written = 0;
    for window in &windows {
        if window.is_empty() {
            continue;
        }

        // Find dominant event kind
        let mut kind_counts: HashMap<String, usize> = HashMap::new();
        for e in window.iter() {
            *kind_counts.entry(e.kind.clone()).or_insert(0) += 1;
        }
        let dominant_kind = kind_counts
            .iter()
            .max_by_key(|(_, v)| *v)
            .map(|(k, _)| k.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let ts_start = window[0].ts_epoch_s;
        let ts_end = window[window.len() - 1].ts_epoch_s;
        let span_s = ts_end.saturating_sub(ts_start);
        let epoch = window[0].continuity_epoch;

        let summary = format!(
            "{} phase: {} events spanning {}s (epoch {})",
            dominant_kind,
            window.len(),
            span_s,
            epoch
        );

        let memory = Memory {
            memory_id: Uuid::new_v4().to_string(),
            ts_start,
            ts_end,
            event_count: window.len(),
            dominant_kind,
            summary,
            continuity_epoch: epoch,
        };

        serde_json::to_writer(&mut file, &memory)?;
        file.write_all(b"\n")?;
        memories_written += 1;
    }

    // Emit journal event for the consolidation
    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now_epoch_s(),
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "memory_consolidation".to_string(),
            severity: "info".to_string(),
            summary: format!(
                "consolidated {} memories from {} events",
                memories_written, total_events
            ),
            reason: None,
            action: Some("consolidate".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
            signature: None,
        },
    )?;

    Ok(memories_written)
}

/// Print all memory entries from `organism_memories.jsonl`.
pub fn list_memories(memories_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !memories_path.exists() {
        println!("No memories yet.");
        return Ok(());
    }

    let file = File::open(memories_path)?;
    let reader = BufReader::new(file);
    let mut count = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let memory: Memory = serde_json::from_str(&line)?;
        println!(
            "[{}..{}] epoch={} events={} dominant={} | {}",
            memory.ts_start,
            memory.ts_end,
            memory.continuity_epoch,
            memory.event_count,
            memory.dominant_kind,
            memory.summary,
        );
        count += 1;
    }

    if count == 0 {
        println!("No memories yet.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AppPaths, Identity, JournalEvent, PolicyEnforcement, PolicyState, RuntimeCtx, RuntimeMode, State};
    use std::env;
    use uuid::Uuid;

    fn make_event(kind: &str, ts: u64, epoch: u64) -> JournalEvent {
        JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: ts,
            organism_id: "org-1".to_string(),
            runtime_habitat: "host_test".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: kind.to_string(),
            severity: "info".to_string(),
            summary: format!("{kind} at {ts}"),
            reason: None,
            action: None,
            result: None,
            continuity_epoch: epoch,
            signature: None,
        }
    }

    fn make_ctx(events: Vec<JournalEvent>) -> (RuntimeCtx, std::path::PathBuf) {
        use std::io::Write as _;
        let dir = env::temp_dir().join(format!("oo-memory-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let journal_path = dir.join("organism_journal.jsonl");
        let mut f = std::fs::File::create(&journal_path).unwrap();
        for e in &events {
            serde_json::to_writer(&mut f, e).unwrap();
            f.write_all(b"\n").unwrap();
        }
        let ctx = RuntimeCtx {
            paths: AppPaths::new(dir.clone()),
            identity: Identity {
                organism_id: "org-1".to_string(),
                genesis_id: "gen-1".to_string(),
                runtime_habitat: "host_test".to_string(),
                created_at_epoch_s: 0,
            },
            state: State {
                boot_or_start_count: 1,
                continuity_epoch: 0,
                last_clean_shutdown: true,
                last_recovery_reason: None,
                last_started_at_epoch_s: 0,
                mode: RuntimeMode::Normal,
                policy: PolicyState {
                    safe_first: true,
                    deny_by_default: true,
                    llm_advisory_only: true,
                    enforcement: PolicyEnforcement::Observe,
                },
                workers: Vec::new(),
                goals: Vec::new(),
                federation: Vec::new(),
            },
            runtime_instance_id: "r1".to_string(),
        };
        (ctx, dir)
    }

    #[test]
    fn window_grouping_splits_on_gap() {
        // events 100–300 in window 1, then 5000–5100 in window 2
        let events = vec![
            make_event("startup", 100, 0),
            make_event("goal_create", 200, 0),
            make_event("goal_create", 300, 0),
            make_event("shutdown", 5000, 0),
            make_event("startup", 5100, 0),
        ];
        let (ctx, dir) = make_ctx(events);
        let n = consolidate_journal(&ctx, 3600, &dir).unwrap();
        assert_eq!(n, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dominant_kind_reflects_most_frequent_event() {
        let events = vec![
            make_event("goal_create", 100, 0),
            make_event("goal_create", 200, 0),
            make_event("startup", 300, 0),
        ];
        let (ctx, dir) = make_ctx(events);
        let memories_path = dir.join("organism_memories.jsonl");
        consolidate_journal(&ctx, 3600, &dir).unwrap();
        let content = std::fs::read_to_string(&memories_path).unwrap();
        let mem: Memory = serde_json::from_str(content.lines().next().unwrap()).unwrap();
        assert_eq!(mem.dominant_kind, "goal_create");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summary_format_contains_dominant_kind_and_span() {
        let events = vec![
            make_event("shutdown", 0, 0),
            make_event("shutdown", 60, 0),
        ];
        let (ctx, dir) = make_ctx(events);
        let memories_path = dir.join("organism_memories.jsonl");
        consolidate_journal(&ctx, 3600, &dir).unwrap();
        let content = std::fs::read_to_string(&memories_path).unwrap();
        let mem: Memory = serde_json::from_str(content.lines().next().unwrap()).unwrap();
        assert!(mem.summary.contains("shutdown phase"));
        assert!(mem.summary.contains("60s"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_journal_produces_zero_memories() {
        let (ctx, dir) = make_ctx(Vec::new());
        let n = consolidate_journal(&ctx, 3600, &dir).unwrap();
        assert_eq!(n, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_window_all_events_in_one_memory() {
        let events = vec![
            make_event("startup", 1, 0),
            make_event("goal_create", 10, 0),
            make_event("shutdown", 20, 0),
        ];
        let (ctx, dir) = make_ctx(events);
        let n = consolidate_journal(&ctx, 3600, &dir).unwrap();
        assert_eq!(n, 1);
        let memories_path = dir.join("organism_memories.jsonl");
        let content = std::fs::read_to_string(&memories_path).unwrap();
        let mem: Memory = serde_json::from_str(content.lines().next().unwrap()).unwrap();
        assert_eq!(mem.event_count, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
