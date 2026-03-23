use uuid::Uuid;
use crate::types::*;
use crate::io::{now_epoch_s, append_event};
use crate::state::persist_ctx;
use crate::goals::{is_actionable_goal_status, is_policy_safe_goal};

pub fn set_mode(ctx: &mut RuntimeCtx, mode: RuntimeMode) -> Result<(), Box<dyn std::error::Error>> {
    let mode_name = mode.as_str().to_string();
    ctx.state.mode = mode;
    persist_ctx(ctx)?;
    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now_epoch_s(),
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "mode_change".to_string(),
            severity: "warn".to_string(),
            summary: format!("mode set to {mode_name}"),
            reason: None,
            action: Some("mode_set".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;
    Ok(())
}

pub fn set_policy_enforcement(
    ctx: &mut RuntimeCtx,
    enforcement: PolicyEnforcement,
) -> Result<(), Box<dyn std::error::Error>> {
    let enforcement_name = enforcement.as_str().to_string();
    ctx.state.policy.enforcement = enforcement;
    persist_ctx(ctx)?;
    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now_epoch_s(),
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "policy_decision".to_string(),
            severity: "warn".to_string(),
            summary: format!("policy enforcement set to {enforcement_name}"),
            reason: None,
            action: Some("policy_set".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;
    Ok(())
}

pub fn print_mode(ctx: &RuntimeCtx) {
    println!("mode={}", ctx.state.mode.as_str());
}

pub fn print_policy(ctx: &RuntimeCtx) {
    println!("safe_first       : {}", ctx.state.policy.safe_first);
    println!("deny_by_default  : {}", ctx.state.policy.deny_by_default);
    println!("llm_advisory_only: {}", ctx.state.policy.llm_advisory_only);
    println!("enforcement      : {}", ctx.state.policy.enforcement.as_str());
}

pub fn apply_policy_homeostasis(
    ctx: &mut RuntimeCtx,
    now_epoch_s: u64,
) -> Result<&'static str, Box<dyn std::error::Error>> {
    if matches!(ctx.state.policy.enforcement, PolicyEnforcement::Enforce)
        && ctx.state.policy.safe_first
        && ctx.state.policy.deny_by_default
    {
        let blocked_goals = block_policy_unsafe_goals(&mut ctx.state, now_epoch_s);
        if !blocked_goals.is_empty() {
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
                        kind: "goal_policy_hold".to_string(),
                        severity: "warn".to_string(),
                        summary: format!("goal held by policy: {goal_title}"),
                        reason: Some("policy_hold".to_string()),
                        action: Some("goal_set_blocked".to_string()),
                        result: Some("ok".to_string()),
                        continuity_epoch: ctx.state.continuity_epoch,
                    },
                )?;
            }
            return Ok("policy_hold_active");
        }

        persist_ctx(ctx)?;
        return Ok("policy_clear");
    }

    let released_goals = release_policy_held_goals(&mut ctx.state, now_epoch_s);
    if !released_goals.is_empty() {
        persist_ctx(ctx)?;
        for goal_title in released_goals {
            append_event(
                &ctx.paths.journal_path,
                &JournalEvent {
                    event_id: Uuid::new_v4().to_string(),
                    ts_epoch_s: now_epoch_s,
                    organism_id: ctx.identity.organism_id.clone(),
                    runtime_habitat: ctx.identity.runtime_habitat.clone(),
                    runtime_instance_id: ctx.runtime_instance_id.clone(),
                    kind: "goal_policy_release".to_string(),
                    severity: "notice".to_string(),
                    summary: format!("goal released from policy hold: {goal_title}"),
                    reason: Some("policy_relaxed".to_string()),
                    action: Some("goal_set_recovering".to_string()),
                    result: Some("ok".to_string()),
                    continuity_epoch: ctx.state.continuity_epoch,
                },
            )?;
        }
        return Ok("policy_released");
    }

    persist_ctx(ctx)?;
    Ok("policy_clear")
}

pub fn block_policy_unsafe_goals(state: &mut State, now_epoch_s: u64) -> Vec<String> {
    let mut blocked = Vec::new();
    for goal in &mut state.goals {
        if is_actionable_goal_status(&goal.status) && !is_policy_safe_goal(goal) {
            goal.status = "blocked".to_string();
            goal.hold_reason = Some("policy_hold".to_string());
            goal.updated_at_epoch_s = now_epoch_s;
            blocked.push(goal.title.clone());
        }
    }
    blocked
}

pub fn release_policy_held_goals(state: &mut State, now_epoch_s: u64) -> Vec<String> {
    let mut recovering = Vec::new();
    for goal in &mut state.goals {
        if goal.status == "blocked" && matches!(goal.hold_reason.as_deref(), Some("policy_hold")) {
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
    use crate::types::{Goal, RuntimeMode, State};
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
        }
    }

    #[test]
    fn block_policy_unsafe_goals_blocks_non_normal_actionable_goals() {
        let mut state = sample_state(vec![
            Goal {
                safety_class: "elevated".to_string(),
                ..goal("g1", "unsafe", "pending", 1, 1)
            },
            goal("g2", "safe", "pending", 1, 2),
            Goal {
                safety_class: "admin".to_string(),
                ..goal("g3", "doing unsafe", "doing", 1, 3)
            },
        ]);
        let blocked = block_policy_unsafe_goals(&mut state, 77);
        assert_eq!(blocked.len(), 2);
        assert_eq!(state.goals[0].status, "blocked");
        assert_eq!(state.goals[0].hold_reason.as_deref(), Some("policy_hold"));
        assert_eq!(state.goals[1].status, "pending");
        assert_eq!(state.goals[2].status, "blocked");
        assert_eq!(state.goals[2].hold_reason.as_deref(), Some("policy_hold"));
    }

    #[test]
    fn release_policy_held_goals_moves_only_policy_holds_to_recovering() {
        let mut state = sample_state(vec![
            Goal {
                hold_reason: Some("policy_hold".to_string()),
                ..goal("g1", "policy", "blocked", 1, 1)
            },
            Goal {
                hold_reason: Some("operator_hold".to_string()),
                ..goal("g2", "operator", "blocked", 1, 2)
            },
        ]);
        let released = release_policy_held_goals(&mut state, 88);
        assert_eq!(released.len(), 1);
        assert_eq!(state.goals[0].status, "recovering");
        assert_eq!(state.goals[0].hold_reason, None);
        assert_eq!(state.goals[1].status, "blocked");
        assert_eq!(state.goals[1].hold_reason.as_deref(), Some("operator_hold"));
    }
}
