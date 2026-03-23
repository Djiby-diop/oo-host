use std::thread;
use std::time::Duration;
use uuid::Uuid;
use crate::types::*;
use crate::io::{now_epoch_s, append_event};
use crate::state::persist_ctx;

pub const WORKER_STALE_AFTER_S: u64 = 300;

pub fn beat_worker(
    ctx: &mut RuntimeCtx,
    worker_id: &str,
    role: &str,
    summary: &str,
    stale_after: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    if let Some(worker) = ctx.state.workers.iter_mut().find(|w| w.worker_id == worker_id) {
        worker.role = role.to_string();
        worker.status = "alive".to_string();
        worker.last_heartbeat_epoch_s = now;
        worker.heartbeat_count += 1;
        if stale_after.is_some() {
            worker.stale_after_s = stale_after;
        }
    } else {
        ctx.state.workers.push(WorkerState {
            worker_id: worker_id.to_string(),
            role: role.to_string(),
            status: "alive".to_string(),
            last_heartbeat_epoch_s: now,
            heartbeat_count: 1,
            stale_after_s: stale_after,
        });
    }

    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "worker_heartbeat".to_string(),
            severity: "info".to_string(),
            summary: format!("worker heartbeat: {worker_id} ({summary})"),
            reason: None,
            action: Some("worker_beat".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
            signature: None,
        },
    )?;

    Ok(())
}

pub fn list_workers(ctx: &RuntimeCtx) {
    if ctx.state.workers.is_empty() {
        println!("No workers.");
        return;
    }

    let now = now_epoch_s();
    for worker in &ctx.state.workers {
        let status = effective_worker_status(worker, now);
        println!(
            "{} | {} | {} | beats={} | last={} ",
            worker.worker_id,
            worker.role,
            status,
            worker.heartbeat_count,
            worker.last_heartbeat_epoch_s
        );
    }
}

pub fn run_worker_watchdog(
    ctx: &mut RuntimeCtx,
    cycles: u32,
    interval_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let total_cycles = if cycles == 0 { 1 } else { cycles };
    for i in 0..total_cycles {
        let now = now_epoch_s();
        let result = apply_worker_homeostasis(ctx, now)?;
        let stale = count_stale_workers(&ctx.state, now);
        let alive = ctx.state.workers.len().saturating_sub(stale);
        println!(
            "watchdog.cycle={} result={} mode={} workers_alive={} workers_stale={}",
            i + 1,
            result,
            ctx.state.mode.as_str(),
            alive,
            stale
        );
        if interval_ms > 0 && i + 1 < total_cycles {
            thread::sleep(Duration::from_millis(interval_ms));
        }
    }
    Ok(())
}

pub fn effective_worker_status(worker: &WorkerState, now_epoch_s: u64) -> &'static str {
    let threshold = worker.stale_after_s.unwrap_or(WORKER_STALE_AFTER_S);
    if now_epoch_s.saturating_sub(worker.last_heartbeat_epoch_s) > threshold {
        "stale"
    } else {
        "alive"
    }
}

pub fn count_stale_workers(state: &State, now_epoch_s: u64) -> usize {
    state
        .workers
        .iter()
        .filter(|w| effective_worker_status(w, now_epoch_s) == "stale")
        .count()
}

pub fn refresh_worker_health(state: &mut State, now_epoch_s: u64) -> usize {
    let mut stale = 0;
    for worker in &mut state.workers {
        let status = effective_worker_status(worker, now_epoch_s).to_string();
        if status == "stale" {
            stale += 1;
        }
        worker.status = status;
    }
    stale
}

pub fn apply_worker_homeostasis(
    ctx: &mut RuntimeCtx,
    now_epoch_s: u64,
) -> Result<&'static str, Box<dyn std::error::Error>> {
    let stale_workers = refresh_worker_health(&mut ctx.state, now_epoch_s);

    if stale_workers > 0 && !matches!(ctx.state.mode, RuntimeMode::Safe | RuntimeMode::Degraded) {
        let blocked_goals = block_active_goals(&mut ctx.state, now_epoch_s);
        ctx.state.mode = RuntimeMode::Degraded;
        persist_ctx(ctx)?;
        for goal_title in blocked_goals {
            append_event(
                &ctx.paths.journal_path,
                &JournalEvent {
                    event_id: Uuid::new_v4().to_string(),
                    ts_epoch_s: now_epoch_s,
                    organism_id: ctx.identity.organism_id.clone(),
                    runtime_habitat: ctx.identity.runtime_habitat.clone(),
                    runtime_instance_id: ctx.runtime_instance_id.clone(),
                    kind: "goal_block".to_string(),
                    severity: "warn".to_string(),
                    summary: format!("goal blocked: {goal_title}"),
                    reason: Some("worker_health_degraded".to_string()),
                    action: Some("goal_set_blocked".to_string()),
                    result: Some("ok".to_string()),
                    continuity_epoch: ctx.state.continuity_epoch,
            signature: None,
                },
            )?;
        }
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now_epoch_s,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "worker_health".to_string(),
                severity: "warn".to_string(),
                summary: format!("{} worker(s) stale; mode degraded", stale_workers),
                reason: Some("stale_worker_detected".to_string()),
                action: Some("mode_set_degraded".to_string()),
                result: Some("ok".to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            signature: None,
            },
        )?;
        return Ok("mode_degraded");
    }

    if stale_workers == 0 && !ctx.state.workers.is_empty() && matches!(ctx.state.mode, RuntimeMode::Degraded) {
        let recovering_goals = recover_blocked_goals(&mut ctx.state, now_epoch_s);
        ctx.state.mode = RuntimeMode::Normal;
        persist_ctx(ctx)?;
        for goal_title in recovering_goals {
            append_event(
                &ctx.paths.journal_path,
                &JournalEvent {
                    event_id: Uuid::new_v4().to_string(),
                    ts_epoch_s: now_epoch_s,
                    organism_id: ctx.identity.organism_id.clone(),
                    runtime_habitat: ctx.identity.runtime_habitat.clone(),
                    runtime_instance_id: ctx.runtime_instance_id.clone(),
                    kind: "goal_recover".to_string(),
                    severity: "notice".to_string(),
                    summary: format!("goal recovering: {goal_title}"),
                    reason: Some("worker_health_restored".to_string()),
                    action: Some("goal_set_recovering".to_string()),
                    result: Some("ok".to_string()),
                    continuity_epoch: ctx.state.continuity_epoch,
            signature: None,
                },
            )?;
        }
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now_epoch_s,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "worker_health".to_string(),
                severity: "notice".to_string(),
                summary: "workers healthy; mode restored to normal".to_string(),
                reason: Some("worker_health_restored".to_string()),
                action: Some("mode_set_normal".to_string()),
                result: Some("ok".to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            signature: None,
            },
        )?;
        return Ok("mode_restored");
    }

    persist_ctx(ctx)?;
    Ok(if stale_workers > 0 { "stale_unchanged" } else { "healthy_unchanged" })
}

pub fn block_active_goals(state: &mut State, now_epoch_s: u64) -> Vec<String> {
    let mut blocked = Vec::new();
    for goal in &mut state.goals {
        if goal.status == "doing" || goal.status == "recovering" {
            goal.status = "blocked".to_string();
            goal.hold_reason = Some("worker_health".to_string());
            goal.updated_at_epoch_s = now_epoch_s;
            blocked.push(goal.title.clone());
        }
    }
    blocked
}

pub fn recover_blocked_goals(state: &mut State, now_epoch_s: u64) -> Vec<String> {
    let mut recovering = Vec::new();
    for goal in &mut state.goals {
        if goal.status == "blocked"
            && matches!(goal.hold_reason.as_deref(), Some("worker_health"))
        {
            goal.status = "recovering".to_string();
            goal.hold_reason = None;
            goal.updated_at_epoch_s = now_epoch_s;
            recovering.push(goal.title.clone());
        }
    }
    recovering
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Goal, PolicyEnforcement, PolicyState, RuntimeMode, State};
    use crate::state::default_policy_state;

    fn sample_state(goals: Vec<Goal>) -> State {
        State {
            boot_or_start_count: 1,
            continuity_epoch: 0,
            last_clean_shutdown: true,
            last_recovery_reason: None,
            last_started_at_epoch_s: 1,
            mode: RuntimeMode::Normal,
            policy: default_policy_state(),
            workers: Vec::new(),
            goals,
            federation: Vec::new(),
        }
    }

    fn goal(id: &str, title: &str, status: &str, priority: i32, created_at_epoch_s: u64) -> Goal {
        Goal {
            goal_id: id.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            hold_reason: None,
            notes: Vec::new(),
            tags: Vec::new(),
            priority,
            created_at_epoch_s,
            updated_at_epoch_s: created_at_epoch_s,
            origin: "test".to_string(),
            safety_class: "normal".to_string(),
            delegated_to: None,
        }
    }

    #[test]
    fn block_active_goals_moves_doing_and_recovering_to_blocked() {
        let mut state = sample_state(vec![
            goal("g1", "doing", "doing", 1, 1),
            goal("g2", "recovering", "recovering", 1, 2),
            goal("g3", "pending", "pending", 1, 3),
        ]);
        let blocked = block_active_goals(&mut state, 99);
        assert_eq!(blocked.len(), 2);
        assert_eq!(state.goals[0].status, "blocked");
        assert_eq!(state.goals[0].hold_reason.as_deref(), Some("worker_health"));
        assert_eq!(state.goals[1].status, "blocked");
        assert_eq!(state.goals[1].hold_reason.as_deref(), Some("worker_health"));
        assert_eq!(state.goals[2].status, "pending");
    }

    #[test]
    fn recover_blocked_goals_moves_blocked_to_recovering() {
        let mut state = sample_state(vec![
            Goal {
                hold_reason: Some("worker_health".to_string()),
                ..goal("g1", "blocked", "blocked", 1, 1)
            },
            goal("g2", "pending", "pending", 1, 2),
        ]);
        let recovering = recover_blocked_goals(&mut state, 100);
        assert_eq!(recovering.len(), 1);
        assert_eq!(state.goals[0].status, "recovering");
        assert_eq!(state.goals[0].hold_reason, None);
        assert_eq!(state.goals[1].status, "pending");
    }

    #[test]
    fn recover_blocked_goals_ignores_operator_hold() {
        let mut state = sample_state(vec![
            Goal {
                hold_reason: Some("operator_hold".to_string()),
                ..goal("g1", "blocked", "blocked", 1, 1)
            },
            goal("g2", "pending", "pending", 1, 2),
        ]);
        let recovering = recover_blocked_goals(&mut state, 100);
        assert!(recovering.is_empty());
        assert_eq!(state.goals[0].status, "blocked");
        assert_eq!(state.goals[0].hold_reason.as_deref(), Some("operator_hold"));
    }

    #[test]
    fn worker_health_marks_worker_stale_after_threshold() {
        let mut state = sample_state(Vec::new());
        state.workers.push(WorkerState {
            worker_id: "w1".to_string(),
            role: "clock".to_string(),
            status: "alive".to_string(),
            last_heartbeat_epoch_s: 10,
            heartbeat_count: 1,
            stale_after_s: None,
        });
        let stale = refresh_worker_health(&mut state, 10 + WORKER_STALE_AFTER_S + 1);
        assert_eq!(stale, 1);
        assert_eq!(state.workers[0].status, "stale");
    }

    #[test]
    fn worker_health_keeps_recent_worker_alive() {
        let mut state = sample_state(Vec::new());
        state.workers.push(WorkerState {
            worker_id: "w1".to_string(),
            role: "clock".to_string(),
            status: "unknown".to_string(),
            last_heartbeat_epoch_s: 100,
            heartbeat_count: 2,
            stale_after_s: None,
        });
        let stale = refresh_worker_health(&mut state, 100 + WORKER_STALE_AFTER_S);
        assert_eq!(stale, 0);
        assert_eq!(state.workers[0].status, "alive");
    }

    #[test]
    fn stale_worker_count_matches_status() {
        let mut state = sample_state(Vec::new());
        state.workers.push(WorkerState {
            worker_id: "w1".to_string(),
            role: "clock".to_string(),
            status: "alive".to_string(),
            last_heartbeat_epoch_s: 1,
            heartbeat_count: 1,
            stale_after_s: None,
        });
        state.workers.push(WorkerState {
            worker_id: "w2".to_string(),
            role: "fs".to_string(),
            status: "alive".to_string(),
            last_heartbeat_epoch_s: 1 + WORKER_STALE_AFTER_S + 5,
            heartbeat_count: 1,
            stale_after_s: None,
        });
        assert_eq!(count_stale_workers(&state, 1 + WORKER_STALE_AFTER_S + 10), 1);
    }

    #[test]
    fn effective_worker_status_uses_custom_stale_after_s() {
        let worker = WorkerState {
            worker_id: "w1".to_string(),
            role: "fast".to_string(),
            status: "alive".to_string(),
            last_heartbeat_epoch_s: 100,
            heartbeat_count: 1,
            stale_after_s: Some(60),
        };
        // 61 seconds later — should be stale with custom threshold of 60s
        assert_eq!(effective_worker_status(&worker, 161), "stale");
        // 60 seconds later — not yet stale
        assert_eq!(effective_worker_status(&worker, 160), "alive");
    }
}
