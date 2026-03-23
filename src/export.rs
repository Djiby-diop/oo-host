use std::path::Path;
use crate::types::*;
use crate::io::{now_epoch_s, write_json, read_recent_events};

pub fn export_sovereign(ctx: &RuntimeCtx, out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut goals: Vec<&Goal> = ctx
        .state
        .goals
        .iter()
        .filter(|g| g.status != "done" && g.status != "aborted")
        .collect();
    goals.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.created_at_epoch_s.cmp(&b.created_at_epoch_s)));

    let recent_events = read_recent_events(&ctx.paths.journal_path, 8)?
        .into_iter()
        .map(|e| SovereignEventExport {
            ts_epoch_s: e.ts_epoch_s,
            kind: e.kind,
            severity: e.severity,
            summary: e.summary,
            reason: e.reason,
            action: e.action,
            result: e.result,
            continuity_epoch: e.continuity_epoch,
        })
        .collect();

    let export = SovereignExport {
        schema_version: 1,
        export_kind: "oo_sovereign_handoff",
        generated_at_epoch_s: now_epoch_s(),
        organism_id: &ctx.identity.organism_id,
        genesis_id: &ctx.identity.genesis_id,
        runtime_habitat: &ctx.identity.runtime_habitat,
        runtime_instance_id: &ctx.runtime_instance_id,
        continuity_epoch: ctx.state.continuity_epoch,
        boot_or_start_count: ctx.state.boot_or_start_count,
        mode: ctx.state.mode.as_str(),
        last_recovery_reason: ctx.state.last_recovery_reason.as_deref(),
        policy: SovereignPolicyExport {
            safe_first: ctx.state.policy.safe_first,
            deny_by_default: ctx.state.policy.deny_by_default,
            llm_advisory_only: ctx.state.policy.llm_advisory_only,
            enforcement: ctx.state.policy.enforcement.as_str(),
        },
        active_goal_count: ctx
            .state
            .goals
            .iter()
            .filter(|g| g.status != "done" && g.status != "aborted")
            .count(),
        top_goals: goals
            .into_iter()
            .take(8)
            .map(|g| SovereignGoalExport {
                goal_id: &g.goal_id,
                title: &g.title,
                status: &g.status,
                priority: g.priority,
                safety_class: &g.safety_class,
            })
            .collect(),
        recent_events,
    };

    write_json(out_path, &export)
}
