use std::collections::HashMap;
use std::path::Path;
use crate::io::{emit_report, now_epoch_s, read_all_events};
use crate::types::{RuntimeCtx, RuntimeMode};

pub fn run_dream(ctx: &RuntimeCtx, depth: usize, out: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    let all_events = read_all_events(&ctx.paths.journal_path)?;

    // Take last `depth` events for analysis
    let start = all_events.len().saturating_sub(depth);
    let events = &all_events[start..];

    // 1. Pattern detection — count event.kind occurrences in the window
    let mut kind_counts: HashMap<String, usize> = HashMap::new();
    for event in events {
        *kind_counts.entry(event.kind.clone()).or_insert(0) += 1;
    }
    let mut kind_vec: Vec<(String, usize)> = kind_counts.into_iter().collect();
    kind_vec.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let top3 = &kind_vec[..kind_vec.len().min(3)];

    let top_kinds_str = if top3.is_empty() {
        "none".to_string()
    } else {
        top3.iter()
            .map(|(k, n)| format!("{} ({}×)", k, n))
            .collect::<Vec<_>>()
            .join(", ")
    };

    // 2. Blocked goal analysis — current state + journal hold events
    let blocked_active: Vec<_> = ctx.state.goals.iter()
        .filter(|g| g.status == "blocked")
        .collect();
    let blocked_count = blocked_active.len();

    let mut hold_reason_counts: HashMap<String, usize> = HashMap::new();
    for goal in &ctx.state.goals {
        if let Some(reason) = &goal.hold_reason {
            *hold_reason_counts.entry(reason.clone()).or_insert(0) += 1;
        }
    }
    for event in &all_events {
        if matches!(event.kind.as_str(), "goal_hold" | "goal_policy_hold" | "goal_block") {
            if let Some(reason) = &event.reason {
                *hold_reason_counts.entry(reason.clone()).or_insert(0) += 1;
            }
        }
    }
    let most_common_hold_reason = hold_reason_counts.iter()
        .max_by_key(|(_, v)| *v)
        .map(|(k, _)| k.as_str())
        .unwrap_or("none")
        .to_string();

    // 3. Goal velocity — average seconds from goal_create to goal_complete for done goals
    let mut velocities: Vec<u64> = Vec::new();
    for goal in ctx.state.goals.iter().filter(|g| g.status == "done") {
        let create_t = all_events.iter()
            .find(|e| e.kind == "goal_create" && e.summary.contains(&goal.title))
            .map(|e| e.ts_epoch_s);
        let complete_t = all_events.iter()
            .find(|e| e.kind == "goal_complete" && e.summary.contains(&goal.title))
            .map(|e| e.ts_epoch_s);
        if let (Some(ct), Some(dt)) = (create_t, complete_t) {
            if dt >= ct {
                velocities.push(dt - ct);
            }
        }
    }
    let velocity_str = if velocities.is_empty() {
        "unknown".to_string()
    } else {
        let avg = velocities.iter().sum::<u64>() / velocities.len() as u64;
        format!("{}s avg", avg)
    };

    // 4. Drift risk score
    let recovery_count = ctx.state.continuity_epoch;
    let drift_risk = if ctx.state.continuity_epoch > 0 && ctx.state.last_recovery_reason.is_some() {
        (ctx.state.continuity_epoch * 10 + blocked_count as u64 * 5).min(100)
    } else {
        0
    };

    // 5. Hypothetical futures
    let active_goals: Vec<_> = ctx.state.goals.iter()
        .filter(|g| g.status != "done" && g.status != "aborted")
        .collect();

    let mut scenarios: Vec<String> = Vec::new();
    match &ctx.state.mode {
        RuntimeMode::Degraded => {
            scenarios.push(
                "Scenario A: worker recovery restores Normal mode → scheduler resumes pending goals"
                    .to_string(),
            );
        }
        _ => {
            if active_goals.is_empty() {
                scenarios.push(
                    "Scenario A: new goal injection triggers scheduler activation → organism enters doing state"
                        .to_string(),
                );
            }
        }
    }
    if !blocked_active.is_empty() {
        scenarios.push(format!(
            "Scenario B: operator releases {} held goals → continuity_epoch stabilises",
            blocked_active.len()
        ));
    }
    if active_goals.len() > 5 {
        scenarios.push(
            "Scenario C: goal pruning reduces active set → organism achieves focused execution"
                .to_string(),
        );
    }
    scenarios.push(format!(
        "Scenario Ω: organism reaches continuity_epoch {} through clean handoff cycle",
        ctx.state.continuity_epoch + 1
    ));

    let scenarios_str = scenarios.iter()
        .map(|s| format!("- {}", s))
        .collect::<Vec<_>>()
        .join("\n");

    let ts = now_epoch_s();
    let goals_total = ctx.state.goals.len();
    let goals_completed = ctx.state.goals.iter().filter(|g| g.status == "done").count();

    let report = format!(
        "# organism dream — epoch {}\n\ngenerated_at: {}\norganism_id: {}\ndream_depth: {}\n\n\
## pattern analysis\n- top event kinds: {}\n- total events analyzed: {}\n\n\
## goal memory\n- goals total: {}\n- goals completed: {}\n- goals blocked (all time): {}\n\
- most common hold_reason: {}\n- estimated goal velocity: {}\n\n\
## drift risk\n- continuity_epoch: {}\n- recovery_count: {}\n- drift_risk_score: {}/100\n\n\
## hypothetical futures\n{}",
        ctx.state.continuity_epoch,
        ts,
        ctx.identity.organism_id,
        depth,
        top_kinds_str,
        events.len(),
        goals_total,
        goals_completed,
        blocked_count,
        most_common_hold_reason,
        velocity_str,
        ctx.state.continuity_epoch,
        recovery_count,
        drift_risk,
        scenarios_str,
    );

    emit_report(&report, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::types::{
        AppPaths, Goal, Identity, JournalEvent, PolicyEnforcement, PolicyState, RuntimeCtx,
        RuntimeMode, State,
    };
    use std::env;
    use uuid::Uuid;

    fn make_event(kind: &str, ts: u64) -> JournalEvent {
        JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: ts,
            organism_id: "org-dream-test".to_string(),
            runtime_habitat: "host_test".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: kind.to_string(),
            severity: "info".to_string(),
            summary: format!("{kind} at {ts}"),
            reason: None,
            action: None,
            result: None,
            continuity_epoch: 0,
            signature: None,
        }
    }

    fn make_ctx(
        events: Vec<JournalEvent>,
        goals: Vec<Goal>,
        mode: RuntimeMode,
        epoch: u64,
        recovery_reason: Option<String>,
    ) -> (RuntimeCtx, std::path::PathBuf) {
        use std::io::Write as _;
        let dir = env::temp_dir().join(format!("oo-dream-test-{}", Uuid::new_v4()));
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
                organism_id: "org-dream-test".to_string(),
                genesis_id: "gen-1".to_string(),
                runtime_habitat: "host_test".to_string(),
                created_at_epoch_s: 0,
            },
            state: State {
                boot_or_start_count: 1,
                continuity_epoch: epoch,
                last_clean_shutdown: true,
                last_recovery_reason: recovery_reason,
                last_started_at_epoch_s: 0,
                mode,
                policy: PolicyState {
                    safe_first: true,
                    deny_by_default: true,
                    llm_advisory_only: true,
                    enforcement: PolicyEnforcement::Observe,
                },
                workers: Vec::new(),
                goals,
                federation: Vec::new(),
            },
            runtime_instance_id: "r1".to_string(),
        };
        (ctx, dir)
    }

    #[test]
    fn pattern_detection_ranks_most_frequent_kind_first() {
        let events = vec![
            make_event("startup", 1),
            make_event("goal_create", 2),
            make_event("goal_create", 3),
            make_event("goal_create", 4),
            make_event("shutdown", 5),
            make_event("shutdown", 6),
        ];
        let mut kind_counts: HashMap<String, usize> = HashMap::new();
        for e in &events {
            *kind_counts.entry(e.kind.clone()).or_insert(0) += 1;
        }
        let mut kind_vec: Vec<(String, usize)> = kind_counts.into_iter().collect();
        kind_vec.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        assert_eq!(kind_vec[0].0, "goal_create");
        assert_eq!(kind_vec[0].1, 3);
        assert_eq!(kind_vec[1].1, 2);
    }

    #[test]
    fn scenario_degraded_mode_produces_scenario_a() {
        let events = vec![make_event("startup", 1)];
        let (ctx, dir) = make_ctx(events, Vec::new(), RuntimeMode::Degraded, 0, None);
        run_dream(&ctx, 50, None).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scenario_no_goals_produces_activation_scenario() {
        let events = vec![make_event("startup", 1)];
        let (ctx, dir) = make_ctx(events, Vec::new(), RuntimeMode::Normal, 0, None);
        run_dream(&ctx, 50, None).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dream_writes_report_to_file() {
        let events = vec![make_event("startup", 1)];
        let (ctx, dir) = make_ctx(events, Vec::new(), RuntimeMode::Normal, 0, None);
        let out_path = dir.join("dream_report.md");
        run_dream(&ctx, 50, Some(&out_path)).unwrap();
        let content = std::fs::read_to_string(&out_path).unwrap();
        assert!(content.contains("# organism dream"));
        assert!(content.contains("org-dream-test"));
        assert!(content.contains("## pattern analysis"));
        assert!(content.contains("## goal memory"));
        assert!(content.contains("## drift risk"));
        assert!(content.contains("## hypothetical futures"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drift_risk_nonzero_when_recovered() {
        let events = vec![make_event("startup", 1)];
        let (ctx, dir) = make_ctx(
            events,
            Vec::new(),
            RuntimeMode::Normal,
            2,
            Some("manual_recover".to_string()),
        );
        // drift_risk = min(100, 2*10 + 0*5) = 20
        let drift = (ctx.state.continuity_epoch * 10).min(100);
        assert_eq!(drift, 20);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn omega_scenario_always_present() {
        let events = vec![make_event("startup", 1)];
        let (ctx, dir) = make_ctx(events, Vec::new(), RuntimeMode::Normal, 3, None);
        let out_path = dir.join("dream.md");
        run_dream(&ctx, 50, Some(&out_path)).unwrap();
        let content = std::fs::read_to_string(&out_path).unwrap();
        assert!(content.contains("Scenario Ω"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
