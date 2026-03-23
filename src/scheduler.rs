use uuid::Uuid;
use crate::types::*;
use crate::io::{now_epoch_s, append_event};
use crate::workers::apply_worker_homeostasis;
use crate::policy::apply_policy_homeostasis;
use crate::goals::{select_next_goal, start_goal};

pub fn scheduler_tick(ctx: &mut RuntimeCtx) -> Result<&'static str, Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let policy_result = apply_policy_homeostasis(ctx, now)?;
    let _ = apply_worker_homeostasis(ctx, now)?;

    if policy_result == "policy_hold_active" {
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "scheduler_tick".to_string(),
                severity: "warn".to_string(),
                summary: "scheduler paused by policy hold".to_string(),
                reason: Some("policy_hold_active".to_string()),
                action: Some("tick_pause".to_string()),
                result: Some("policy_pause".to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            },
        )?;
        return Ok("policy_pause");
    }

    if matches!(ctx.state.mode, RuntimeMode::Degraded) {
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "scheduler_tick".to_string(),
                severity: "warn".to_string(),
                summary: "scheduler paused while runtime is degraded".to_string(),
                reason: Some("worker_health_degraded".to_string()),
                action: Some("tick_pause".to_string()),
                result: Some("degraded_pause".to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            },
        )?;
        return Ok("degraded_pause");
    }

    if let Some(goal) = ctx.state.goals.iter().find(|g| g.status == "doing") {
        let active_title = goal.title.clone();
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "scheduler_tick".to_string(),
                severity: "info".to_string(),
                summary: format!("scheduler kept active goal: {active_title}"),
                reason: Some("active_goal_present".to_string()),
                action: Some("tick_noop".to_string()),
                result: Some("active_goal_unchanged".to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            },
        )?;
        return Ok("active_goal_unchanged");
    }

    let next_goal = select_next_goal(&ctx.state)
        .filter(|g| g.status == "pending" || g.status == "recovering")
        .map(|g| (g.goal_id.clone(), g.status.clone(), g.title.clone()));

    if let Some((goal_id, prior_status, goal_title)) = next_goal {
        start_goal(ctx, &goal_id)?;
        let (summary, reason, action, result) = if prior_status == "recovering" {
            (
                format!("scheduler resumed goal: {goal_title}"),
                "worker_health_restored",
                "tick_resume_goal",
                "goal_resumed",
            )
        } else {
            (
                format!("scheduler activated goal: {goal_id}"),
                "selected_pending_goal",
                "tick_start_goal",
                "goal_started",
            )
        };
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "scheduler_tick".to_string(),
                severity: "notice".to_string(),
                summary,
                reason: Some(reason.to_string()),
                action: Some(action.to_string()),
                result: Some(result.to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            },
        )?;
        return Ok(result);
    }

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "scheduler_tick".to_string(),
            severity: "info".to_string(),
            summary: "scheduler found no pending goals".to_string(),
            reason: Some("no_pending_goals".to_string()),
            action: Some("tick_noop".to_string()),
            result: Some("idle".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok("idle")
}
