use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use uuid::Uuid;
use crate::types::{Goal, JournalEvent, RuntimeCtx};
use crate::io::{read_recent_events, read_all_events, append_event, now_epoch_s};

pub fn tail_journal(path: &Path, count: usize) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        println!("Journal is empty.");
        return Ok(());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;
    let start = lines.len().saturating_sub(count);
    for line in lines.drain(start..) {
        println!("{line}");
    }
    Ok(())
}

pub fn explain_journal(path: &Path, count: usize) -> Result<(), Box<dyn std::error::Error>> {
    let events = read_recent_events(path, count)?;
    if events.is_empty() {
        println!("Journal is empty.");
        return Ok(());
    }

    for event in events {
        println!(
            "[{}] {} | {} | {}",
            event.ts_epoch_s,
            event.kind,
            event.severity,
            explain_event(&event)
        );
    }

    Ok(())
}

pub fn search_journal(
    path: &Path,
    kind: Option<&str>,
    severity: Option<&str>,
    since: Option<u64>,
    until: Option<u64>,
    count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let all = read_all_events(path)?;
    if all.is_empty() {
        println!("Journal is empty.");
        return Ok(());
    }

    let filtered: Vec<&JournalEvent> = all
        .iter()
        .filter(|e| {
            if let Some(k) = kind { if e.kind != k { return false; } }
            if let Some(s) = severity { if e.severity != s { return false; } }
            if let Some(since_s) = since { if e.ts_epoch_s < since_s { return false; } }
            if let Some(until_s) = until { if e.ts_epoch_s > until_s { return false; } }
            true
        })
        .collect();

    let start = filtered.len().saturating_sub(count);
    let page = &filtered[start..];

    if page.is_empty() {
        println!("No matching events.");
        return Ok(());
    }

    for event in page {
        println!(
            "[{}] {} | {} | {}",
            event.ts_epoch_s,
            event.kind,
            event.severity,
            explain_event(event)
        );
    }

    Ok(())
}

pub fn render_journal_explain_markdown(events: &[JournalEvent]) -> String {
    let mut lines = vec!["# journal explain".to_string(), String::new()];

    if events.is_empty() {
        lines.push("- none".to_string());
    } else {
        for event in events {
            lines.push(format!(
                "- [{}] {} | {} | {}",
                event.ts_epoch_s,
                event.kind,
                event.severity,
                explain_event(event)
            ));
        }
    }

    lines.join("\n")
}

pub fn explain_event(event: &JournalEvent) -> String {
    match event.kind.as_str() {
        "goal_note" => format!(
            "goal note recorded; summary='{}'; action={}",
            event.summary,
            event.action.as_deref().unwrap_or("none")
        ),
        "goal_hold" => format!(
            "operator hold applied; summary='{}'; reason={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none")
        ),
        "goal_policy_hold" => format!(
            "governance hold applied; summary='{}'; reason={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none")
        ),
        "goal_policy_release" => format!(
            "governance hold released; summary='{}'; result={}",
            event.summary,
            event.result.as_deref().unwrap_or("none")
        ),
        "goal_block" => format!(
            "worker incident blocked a goal; summary='{}'; reason={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none")
        ),
        "goal_recover" => format!(
            "goal entered recovery; summary='{}'; reason={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none")
        ),
        "goal_resume" => format!(
            "goal resumed; summary='{}'; action={}",
            event.summary,
            event.action.as_deref().unwrap_or("none")
        ),
        "worker_health" => format!(
            "worker health transition; summary='{}'; action={}; result={}",
            event.summary,
            event.action.as_deref().unwrap_or("none"),
            event.result.as_deref().unwrap_or("none")
        ),
        "scheduler_tick" => format!(
            "scheduler decision; summary='{}'; reason={}; result={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none"),
            event.result.as_deref().unwrap_or("none")
        ),
        "goal_start" | "goal_complete" | "goal_abort" | "goal_create" | "goal_delete" => format!(
            "goal lifecycle; summary='{}'; action={}; result={}",
            event.summary,
            event.action.as_deref().unwrap_or("none"),
            event.result.as_deref().unwrap_or("none")
        ),
        "goal_tag_add" | "goal_tag_remove" => format!(
            "goal tag changed; summary='{}'; action={}; result={}",
            event.summary,
            event.action.as_deref().unwrap_or("none"),
            event.result.as_deref().unwrap_or("none")
        ),
        "worker_heartbeat" => format!(
            "worker heartbeat observed; summary='{}'; result={}",
            event.summary,
            event.result.as_deref().unwrap_or("none")
        ),
        "policy_decision" => format!(
            "policy changed; summary='{}'; action={}",
            event.summary,
            event.action.as_deref().unwrap_or("none")
        ),
        _ => format!(
            "summary='{}'; reason={}; action={}; result={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none"),
            event.action.as_deref().unwrap_or("none"),
            event.result.as_deref().unwrap_or("none")
        ),
    }
}

pub fn collect_goal_events(
    path: &Path,
    goal: &Goal,
) -> Result<Vec<JournalEvent>, Box<dyn std::error::Error>> {
    let events = read_all_events(path)?;
    Ok(events
        .into_iter()
        .filter(|event| event_mentions_goal(event, goal))
        .collect())
}

pub fn event_mentions_goal(event: &JournalEvent, goal: &Goal) -> bool {
    let summary = event.summary.as_str();
    let reason = event.reason.as_deref().unwrap_or("");
    let action = event.action.as_deref().unwrap_or("");

    summary.contains(&goal.goal_id)
        || summary.contains(&goal.title)
        || reason.contains(&goal.goal_id)
        || reason.contains(&goal.title)
        || action.contains(&goal.goal_id)
}

pub fn rotate_journal(
    ctx: &RuntimeCtx,
    max_lines: usize,
    keep: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = &ctx.paths.journal_path;

    if !path.exists() {
        println!("OK: journal within limit (0 lines), no rotation needed");
        return Ok(());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;
    let total = lines.len();

    if total < max_lines {
        println!("OK: journal within limit ({total} lines), no rotation needed");
        return Ok(());
    }

    let ts = now_epoch_s();
    let archive_name = format!("organism_journal_{ts}.jsonl");
    let archive_path = path.parent().unwrap_or(Path::new(".")).join(&archive_name);

    fs::copy(path, &archive_path)?;

    let kept_lines = if keep >= total { &lines[..] } else { &lines[total - keep..] };
    let kept = kept_lines.len();
    {
        let mut out = File::create(path)?;
        for line in kept_lines {
            out.write_all(line.as_bytes())?;
            out.write_all(b"\n")?;
        }
    }

    append_event(
        path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now_epoch_s(),
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "journal_rotate".to_string(),
            severity: "info".to_string(),
            summary: format!(
                "journal rotated: {total} lines archived to {archive_name}, {kept} lines kept"
            ),
            reason: None,
            action: None,
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
            signature: None,
        },
    )?;

    println!("OK: journal rotated — {total} lines archived, {kept} kept");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_event_formats_worker_health_transition() {
        let event = JournalEvent {
            event_id: "e1".to_string(),
            ts_epoch_s: 1,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "worker_health".to_string(),
            severity: "warn".to_string(),
            summary: "1 worker stale; mode degraded".to_string(),
            reason: Some("stale_worker_detected".to_string()),
            action: Some("mode_set_degraded".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: 0,
            signature: None,
        };
        let text = explain_event(&event);
        assert!(text.contains("worker health transition"));
        assert!(text.contains("mode_set_degraded"));
        assert!(text.contains("ok"));
    }

    #[test]
    fn explain_event_formats_policy_hold() {
        let event = JournalEvent {
            event_id: "e2".to_string(),
            ts_epoch_s: 1,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "goal_policy_hold".to_string(),
            severity: "warn".to_string(),
            summary: "goal held by policy: unsafe".to_string(),
            reason: Some("policy_hold".to_string()),
            action: Some("goal_set_blocked".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: 0,
            signature: None,
        };
        let text = explain_event(&event);
        assert!(text.contains("governance hold applied"));
        assert!(text.contains("policy_hold"));
    }

    #[test]
    fn render_journal_explain_markdown_contains_event_explanations() {
        let events = vec![JournalEvent {
            event_id: "e1".to_string(),
            ts_epoch_s: 11,
            organism_id: "org-1".to_string(),
            runtime_habitat: "host_test".to_string(),
            runtime_instance_id: "run-1".to_string(),
            kind: "worker_health".to_string(),
            severity: "warn".to_string(),
            summary: "1 worker stale; mode degraded".to_string(),
            reason: Some("stale_worker_detected".to_string()),
            action: Some("mode_set_degraded".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: 0,
            signature: None,
        }];
        let markdown = render_journal_explain_markdown(&events);
        assert!(markdown.contains("# journal explain"));
        assert!(markdown.contains("worker health transition"));
        assert!(markdown.contains("mode_set_degraded"));
    }

    #[test]
    fn event_mentions_goal_matches_title_and_id() {
        let goal = Goal {
            goal_id: "g1".to_string(),
            title: "inspect me".to_string(),
            status: "doing".to_string(),
            hold_reason: None,
            notes: Vec::new(),
            tags: Vec::new(),
            priority: 1,
            created_at_epoch_s: 1,
            updated_at_epoch_s: 1,
            origin: "test".to_string(),
            safety_class: "normal".to_string(),
            delegated_to: None,
        };
        let by_title = JournalEvent {
            event_id: "e1".to_string(),
            ts_epoch_s: 1,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "goal_note".to_string(),
            severity: "info".to_string(),
            summary: "goal note added: inspect me".to_string(),
            reason: None,
            action: Some("goal_note_add".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: 0,
            signature: None,
        };
        let by_id = JournalEvent {
            event_id: "e2".to_string(),
            ts_epoch_s: 1,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "scheduler_tick".to_string(),
            severity: "notice".to_string(),
            summary: "scheduler activated goal: g1".to_string(),
            reason: Some("selected_pending_goal".to_string()),
            action: Some("tick_start_goal".to_string()),
            result: Some("goal_started".to_string()),
            continuity_epoch: 0,
            signature: None,
        };
        let other = JournalEvent {
            event_id: "e3".to_string(),
            ts_epoch_s: 1,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "goal_note".to_string(),
            severity: "info".to_string(),
            summary: "goal note added: someone else".to_string(),
            reason: None,
            action: Some("goal_note_add".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: 0,
            signature: None,
        };
        assert!(event_mentions_goal(&by_title, &goal));
        assert!(event_mentions_goal(&by_id, &goal));
        assert!(!event_mentions_goal(&other, &goal));
    }

    fn make_event(kind: &str, severity: &str, ts: u64) -> JournalEvent {
        JournalEvent {
            event_id: format!("e-{ts}"),
            ts_epoch_s: ts,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: kind.to_string(),
            severity: severity.to_string(),
            summary: format!("{kind} at {ts}"),
            reason: None,
            action: None,
            result: None,
            continuity_epoch: 0,
            signature: None,
        }
    }

    #[test]
    fn search_journal_filters_by_kind() {
        let events = vec![
            make_event("goal_create", "notice", 100),
            make_event("worker_heartbeat", "info", 200),
            make_event("goal_create", "notice", 300),
        ];
        let filtered: Vec<&JournalEvent> = events.iter()
            .filter(|e| e.kind == "goal_create")
            .collect();
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].ts_epoch_s, 100);
        assert_eq!(filtered[1].ts_epoch_s, 300);
    }

    #[test]
    fn search_journal_filters_by_severity() {
        let events = vec![
            make_event("goal_create", "notice", 100),
            make_event("goal_hold", "warn", 200),
            make_event("goal_note", "info", 300),
        ];
        let filtered: Vec<&JournalEvent> = events.iter()
            .filter(|e| e.severity == "warn")
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].kind, "goal_hold");
    }

    #[test]
    fn search_journal_filters_by_time_range() {
        let events = vec![
            make_event("goal_create", "notice", 100),
            make_event("goal_create", "notice", 200),
            make_event("goal_create", "notice", 300),
        ];
        let filtered: Vec<&JournalEvent> = events.iter()
            .filter(|e| e.ts_epoch_s >= 150 && e.ts_epoch_s <= 250)
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].ts_epoch_s, 200);
    }

    #[test]
    fn search_journal_limits_count() {
        let events: Vec<JournalEvent> = (0..10).map(|i| make_event("shutdown", "info", i)).collect();
        let count = 3;
        let start = events.len().saturating_sub(count);
        let page: Vec<&JournalEvent> = events[start..].iter().collect();
        assert_eq!(page.len(), 3);
        assert_eq!(page[0].ts_epoch_s, 7);
    }

    #[test]
    fn explain_event_formats_goal_delete() {
        let event = make_event("goal_delete", "notice", 1);
        let text = explain_event(&event);
        assert!(text.contains("goal lifecycle"));
    }

    #[test]
    fn explain_event_formats_goal_tag_add() {
        let event = make_event("goal_tag_add", "info", 1);
        let text = explain_event(&event);
        assert!(text.contains("goal tag changed"));
    }

    #[test]
    fn explain_event_formats_goal_tag_remove() {
        let event = make_event("goal_tag_remove", "info", 1);
        let text = explain_event(&event);
        assert!(text.contains("goal tag changed"));
    }

    #[test]
    fn rotate_journal_archives_and_keeps_recent_lines() {
        use crate::types::{
            AppPaths, Identity, PolicyEnforcement, PolicyState, RuntimeCtx, RuntimeMode, State,
        };
        use std::io::Write as _;
        use std::fs;
        use uuid::Uuid;

        let dir = std::env::temp_dir().join(format!("oo-host-rotate-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let journal_path = dir.join("organism_journal.jsonl");

        // Write 15 dummy lines
        let mut f = fs::File::create(&journal_path).unwrap();
        for i in 0..15 {
            writeln!(f, r#"{{"event_id":"e{i}","ts_epoch_s":{i},"organism_id":"o1","runtime_habitat":"host","runtime_instance_id":"r1","kind":"startup","severity":"info","summary":"line {i}","reason":null,"action":null,"result":null,"continuity_epoch":0}}"#).unwrap();
        }
        drop(f);

        let paths = AppPaths::new(dir.clone());
        let ctx = RuntimeCtx {
            paths,
            identity: Identity {
                organism_id: "o1".to_string(),
                genesis_id: "g1".to_string(),
                runtime_habitat: "host".to_string(),
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
                workers: vec![],
                goals: vec![],
            federation: Vec::new(),
            },
            runtime_instance_id: "r1".to_string(),
        };

        rotate_journal(&ctx, 10, 5).unwrap();

        // Active journal should have at most keep+1 lines (5 kept + rotation event)
        let active_content = fs::read_to_string(&journal_path).unwrap();
        let active_lines: Vec<&str> = active_content.lines().filter(|l| !l.is_empty()).collect();
        assert!(active_lines.len() <= 6, "expected ≤6 lines, got {}", active_lines.len());

        // Archive should exist
        let archives: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                name.starts_with("organism_journal_") && name.ends_with(".jsonl")
            })
            .collect();
        assert_eq!(archives.len(), 1, "expected 1 archive file");

        let archive_content = fs::read_to_string(archives[0].path()).unwrap();
        let archive_lines: Vec<&str> = archive_content.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(archive_lines.len(), 15, "archive should have all 15 original lines");

        let _ = fs::remove_dir_all(&dir);
    }
}
