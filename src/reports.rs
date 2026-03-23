use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use crate::types::*;
use crate::io::{now_epoch_s, emit_report, write_text_file, read_recent_events, read_key_value_file, present_absent};
use crate::workers::count_stale_workers;
use crate::goals::{select_next_goal, render_next_goal_markdown};
use crate::journal::render_journal_explain_markdown;

pub fn print_status(
    ctx: &RuntimeCtx,
    format: OutputFormat,
    out: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if format.is_markdown() {
        let rendered = render_status_markdown(ctx);
        emit_report(&rendered, out)?;
        return Ok(());
    }

    let rendered = render_status_text(ctx);
    emit_report(&rendered, out)?;
    Ok(())
}

pub fn render_status_text(ctx: &RuntimeCtx) -> String {
    let mut lines = vec![
        format!("organism_id       : {}", ctx.identity.organism_id),
        format!("genesis_id        : {}", ctx.identity.genesis_id),
        format!("runtime_habitat   : {}", ctx.identity.runtime_habitat),
        format!("runtime_instance  : {}", ctx.runtime_instance_id),
        format!("start_count       : {}", ctx.state.boot_or_start_count),
        format!("continuity_epoch  : {}", ctx.state.continuity_epoch),
        format!("mode              : {}", ctx.state.mode.as_str()),
        format!("policy            : {}", ctx.state.policy.enforcement.as_str()),
        format!("last_clean        : {}", ctx.state.last_clean_shutdown),
        format!(
            "last_recovery     : {}",
            ctx.state.last_recovery_reason.as_deref().unwrap_or("none")
        ),
        format!("goals             : {}", ctx.state.goals.len()),
    ];
    let stale_workers = count_stale_workers(&ctx.state, now_epoch_s());
    let alive_workers = ctx.state.workers.len().saturating_sub(stale_workers);
    lines.push(format!("workers           : {}", ctx.state.workers.len()));
    lines.push(format!("workers_alive     : {}", alive_workers));
    lines.push(format!("workers_stale     : {}", stale_workers));
    if let Some(goal) = select_next_goal(&ctx.state) {
        lines.push(format!("next_goal_id      : {}", goal.goal_id));
        lines.push(format!("next_goal_title   : {}", goal.title));
        lines.push(format!("next_goal_prio    : {}", goal.priority));
        lines.push(format!("next_goal_status  : {}", goal.status));
    } else {
        lines.push("next_goal_id      : none".to_string());
    }
    lines.join("\n")
}

pub fn render_status_markdown(ctx: &RuntimeCtx) -> String {
    let stale_workers = count_stale_workers(&ctx.state, now_epoch_s());
    let alive_workers = ctx.state.workers.len().saturating_sub(stale_workers);

    let mut lines = vec![
        "# oo-host status".to_string(),
        String::new(),
        format!("- organism_id: {}", ctx.identity.organism_id),
        format!("- genesis_id: {}", ctx.identity.genesis_id),
        format!("- runtime_habitat: {}", ctx.identity.runtime_habitat),
        format!("- runtime_instance: {}", ctx.runtime_instance_id),
        format!("- start_count: {}", ctx.state.boot_or_start_count),
        format!("- continuity_epoch: {}", ctx.state.continuity_epoch),
        format!("- mode: {}", ctx.state.mode.as_str()),
        format!("- policy: {}", ctx.state.policy.enforcement.as_str()),
        format!("- last_clean: {}", ctx.state.last_clean_shutdown),
        format!(
            "- last_recovery: {}",
            ctx.state.last_recovery_reason.as_deref().unwrap_or("none")
        ),
        format!("- goals: {}", ctx.state.goals.len()),
        format!("- workers: {}", ctx.state.workers.len()),
        format!("- workers_alive: {}", alive_workers),
        format!("- workers_stale: {}", stale_workers),
    ];

    lines.push(String::new());
    lines.push("## next goal".to_string());
    lines.push(String::new());
    if let Some(goal) = select_next_goal(&ctx.state) {
        lines.push(format!("- id: {}", goal.goal_id));
        lines.push(format!("- title: {}", goal.title));
        lines.push(format!("- status: {}", goal.status));
        lines.push(format!("- priority: {}", goal.priority));
        lines.push(format!("- hold_reason: {}", goal.hold_reason.as_deref().unwrap_or("none")));
    } else {
        lines.push("- none".to_string());
    }

    lines.join("\n")
}

pub fn write_daily_reports(
    ctx: &RuntimeCtx,
    out_dir: &Path,
    journal_count: usize,
    include_sovereign: bool,
    include_sync: bool,
    sovereign_workspace: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let status_path = out_dir.join("status.md");
    let next_goal_path = out_dir.join("next-goal.md");
    let journal_path = out_dir.join("journal-explain.md");

    write_text_file(&status_path, &render_status_markdown(ctx))?;
    write_text_file(&next_goal_path, &render_next_goal_markdown(ctx))?;

    let events = read_recent_events(&ctx.paths.journal_path, journal_count)?;
    write_text_file(&journal_path, &render_journal_explain_markdown(&events))?;

    if include_sovereign || include_sync {
        let workspace = resolve_sovereign_workspace(sovereign_workspace)?;
        if include_sovereign {
            let sovereign_path = out_dir.join("sovereign.md");
            write_text_file(&sovereign_path, &render_sovereign_summary_markdown(&workspace)?)?;
        }
        if include_sync {
            let sync_path = out_dir.join("sync.md");
            write_text_file(&sync_path, &render_sync_summary_markdown(ctx, &workspace)?)?;
        }
    }

    Ok(())
}

pub fn render_sovereign_summary_markdown(workspace: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let readme_path = workspace.join("README.md");
    let receipt_path = workspace.join("OOHANDOFF.TXT");
    let handoff_script = workspace.join("test-qemu-handoff.ps1");
    let smoke_script = workspace.join("llmk-autorun-handoff-smoke.txt");
    let receipt = read_key_value_file(&receipt_path)?;

    let mut lines = vec![
        "# sovereign summary".to_string(),
        String::new(),
        format!("- workspace: {}", workspace.display()),
        format!("- present: {}", workspace.exists()),
        format!("- readme: {}", present_absent(readme_path.exists())),
        format!("- handoff_receipt: {}", present_absent(receipt_path.exists())),
        format!("- handoff_script: {}", present_absent(handoff_script.exists())),
        format!("- handoff_smoke: {}", present_absent(smoke_script.exists())),
        String::new(),
        "## receipt".to_string(),
        String::new(),
    ];

    if receipt.is_empty() {
        lines.push("- none".to_string());
    } else {
        for (key, value) in receipt {
            lines.push(format!("- {}: {}", key, value));
        }
    }

    Ok(lines.join("\n"))
}

pub fn render_sync_summary_markdown(
    ctx: &RuntimeCtx,
    workspace: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let receipt = read_key_value_file(&workspace.join("OOHANDOFF.TXT"))?;
    let host = host_sync_fields(ctx);
    let verdict = compute_sync_verdict(&host, &receipt);
    let mismatches = collect_sync_mismatches(&host, &receipt);
    let actions = recommend_sync_actions(verdict, &mismatches);

    let mut lines = vec![
        "# sync summary".to_string(),
        String::new(),
        format!("- sovereign_workspace: {}", workspace.display()),
        format!("- verdict: {}", verdict),
        String::new(),
        "## comparisons".to_string(),
        String::new(),
    ];

    for (key, host_value) in &host {
        let receipt_value = receipt.get(key).map(String::as_str).unwrap_or("missing");
        let status = if receipt_value == host_value { "aligned" } else { "mismatch" };
        lines.push(format!(
            "- {}: host=`{}` sovereign=`{}` status=`{}`",
            key, host_value, receipt_value, status
        ));
    }

    lines.push(String::new());
    lines.push("## mismatches".to_string());
    lines.push(String::new());
    if mismatches.is_empty() {
        lines.push("- none".to_string());
    } else {
        for mismatch in &mismatches {
            lines.push(format!(
                "- {}: host=`{}` sovereign=`{}`",
                mismatch.field, mismatch.host_value, mismatch.receipt_value
            ));
        }
    }

    lines.push(String::new());
    lines.push("## recommended actions".to_string());
    lines.push(String::new());
    for action in actions {
        lines.push(format!("- {}", action));
    }

    Ok(lines.join("\n"))
}

pub fn collect_sync_mismatches(
    host: &BTreeMap<String, String>,
    receipt: &BTreeMap<String, String>,
) -> Vec<SyncMismatch> {
    host.iter()
        .filter_map(|(field, host_value)| {
            let receipt_value = receipt
                .get(field)
                .cloned()
                .unwrap_or_else(|| "missing".to_string());
            if receipt_value == *host_value {
                None
            } else {
                Some(SyncMismatch {
                    field: field.clone(),
                    host_value: host_value.clone(),
                    receipt_value,
                })
            }
        })
        .collect()
}

pub fn recommend_sync_actions(verdict: &str, mismatches: &[SyncMismatch]) -> Vec<String> {
    let mut out = Vec::new();
    match verdict {
        "aligned" => out.push("No action required; host state and sovereign receipt are aligned.".to_string()),
        "receipt_missing" => {
            out.push("Generate or collect `OOHANDOFF.TXT` from the sovereign workspace before trusting sync state.".to_string());
            out.push("Run the sovereign handoff flow so the receipt is refreshed beside the workspace.".to_string());
        }
        "organism_mismatch" => {
            out.push("Stop cross-runtime handoff until both sides point to the same organism identifier again.".to_string());
            out.push("Verify the correct sovereign workspace is selected before applying any further host actions.".to_string());
        }
        "host_ahead" => {
            out.push("Export or apply the latest host handoff so the sovereign receipt catches up.".to_string());
            out.push("Re-run the daily report after the next handoff application to confirm convergence.".to_string());
        }
        "drift" => {
            out.push("Inspect the mismatched fields and determine whether host or sovereign state changed unexpectedly.".to_string());
            out.push("Repair the drift before the next handoff cycle so continuity and governance stay consistent.".to_string());
        }
        _ => out.push("Review the sync comparison manually before proceeding.".to_string()),
    }

    if !mismatches.is_empty() && verdict != "aligned" {
        let fields = mismatches
            .iter()
            .map(|m| m.field.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push(format!("Focus first on these fields: {}.", fields));
    }

    out
}

pub fn host_sync_fields(ctx: &RuntimeCtx) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    out.insert("organism_id".to_string(), ctx.identity.organism_id.clone());
    out.insert("mode".to_string(), ctx.state.mode.as_str().to_string());
    out.insert(
        "policy_enforcement".to_string(),
        ctx.state.policy.enforcement.as_str().to_string(),
    );
    out.insert(
        "continuity_epoch".to_string(),
        ctx.state.continuity_epoch.to_string(),
    );
    out.insert(
        "last_recovery_reason".to_string(),
        ctx.state
            .last_recovery_reason
            .clone()
            .unwrap_or_else(|| "none".to_string()),
    );
    out
}

pub fn compute_sync_verdict(host: &BTreeMap<String, String>, receipt: &BTreeMap<String, String>) -> &'static str {
    if receipt.is_empty() {
        return "receipt_missing";
    }

    let organism_matches = receipt.get("organism_id") == host.get("organism_id");
    if !organism_matches {
        return "organism_mismatch";
    }

    let host_continuity = host
        .get("continuity_epoch")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let receipt_continuity = receipt
        .get("continuity_epoch")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let all_match = host.iter().all(|(key, value)| receipt.get(key) == Some(value));
    if all_match {
        "aligned"
    } else if host_continuity > receipt_continuity {
        "host_ahead"
    } else {
        "drift"
    }
}

pub fn resolve_sovereign_workspace(override_path: Option<&Path>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = override_path {
        return Ok(path.to_path_buf());
    }

    let cwd = std::env::current_dir()?;
    if let Some(parent) = cwd.parent() {
        return Ok(parent.join("llm-baremetal"));
    }

    Ok(PathBuf::from("..\\llm-baremetal"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use uuid::Uuid;
    use crate::types::{AppPaths, Goal, Identity, RuntimeCtx, RuntimeMode, State};
    use crate::state::default_policy_state;
    use crate::io::{append_event, write_text_file};

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

    fn sample_ctx(goals: Vec<Goal>) -> RuntimeCtx {
        let tmp = env::temp_dir().join(format!("oo-host-test-{}", Uuid::new_v4()));
        RuntimeCtx {
            paths: AppPaths::new(tmp),
            identity: Identity {
                organism_id: "org-1".to_string(),
                genesis_id: "gen-1".to_string(),
                runtime_habitat: "host_test".to_string(),
                created_at_epoch_s: 1,
            },
            state: sample_state(goals),
            runtime_instance_id: "run-1".to_string(),
        }
    }

    #[test]
    fn render_status_markdown_contains_next_goal_section() {
        let ctx = sample_ctx(vec![goal("g1", "inspect me", "pending", 3, 1)]);
        let markdown = render_status_markdown(&ctx);
        assert!(markdown.contains("# oo-host status"));
        assert!(markdown.contains("## next goal"));
        assert!(markdown.contains("- id: g1"));
        assert!(markdown.contains("- title: inspect me"));
    }

    #[test]
    fn render_status_text_contains_next_goal_fields() {
        let ctx = sample_ctx(vec![goal("g1", "inspect me", "pending", 3, 1)]);
        let text = render_status_text(&ctx);
        assert!(text.contains("organism_id       : org-1"));
        assert!(text.contains("next_goal_id      : g1"));
        assert!(text.contains("next_goal_title   : inspect me"));
    }

    #[test]
    fn write_daily_reports_creates_expected_bundle() {
        let dir = env::temp_dir().join(format!("oo-host-daily-{}", Uuid::new_v4()));
        let report_dir = dir.join("bundle");
        let journal_path = dir.join("organism_journal.jsonl");
        let mut ctx = sample_ctx(vec![goal("g1", "inspect me", "pending", 3, 1)]);
        ctx.paths = AppPaths::new(dir.clone());
        append_event(
            &journal_path,
            &crate::types::JournalEvent {
                event_id: "e1".to_string(),
                ts_epoch_s: 11,
                organism_id: "org-1".to_string(),
                runtime_habitat: "host_test".to_string(),
                runtime_instance_id: "run-1".to_string(),
                kind: "goal_note".to_string(),
                severity: "info".to_string(),
                summary: "goal note added: inspect me".to_string(),
                reason: None,
                action: Some("goal_note_add".to_string()),
                result: Some("ok".to_string()),
                continuity_epoch: 0,
            signature: None,
            },
        ).expect("append event");
        write_daily_reports(&ctx, &report_dir, 20, false, false, None).expect("write daily reports");
        let status = std::fs::read_to_string(report_dir.join("status.md")).expect("read status");
        let next_goal = std::fs::read_to_string(report_dir.join("next-goal.md")).expect("read next-goal");
        let journal = std::fs::read_to_string(report_dir.join("journal-explain.md")).expect("read journal");
        assert!(status.contains("# oo-host status"));
        assert!(next_goal.contains("# goal g1") || next_goal.contains("# next goal"));
        assert!(journal.contains("# journal explain"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_sovereign_summary_markdown_contains_receipt_fields() {
        let dir = env::temp_dir().join(format!("oo-host-sovereign-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create sovereign dir");
        write_text_file(
            &dir.join("OOHANDOFF.TXT"),
            "organism_id=abc\nmode=normal\npolicy_enforcement=observe",
        ).expect("write receipt");
        let markdown = render_sovereign_summary_markdown(&dir).expect("render sovereign summary");
        assert!(markdown.contains("# sovereign summary"));
        assert!(markdown.contains("- handoff_receipt: present"));
        assert!(markdown.contains("- organism_id: abc"));
        assert!(markdown.contains("- mode: normal"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_daily_reports_can_include_sovereign_bundle() {
        let dir = env::temp_dir().join(format!("oo-host-daily-sovereign-{}", Uuid::new_v4()));
        let report_dir = dir.join("bundle");
        let sovereign_dir = dir.join("llm-baremetal");
        let mut ctx = sample_ctx(vec![goal("g1", "inspect me", "pending", 3, 1)]);
        ctx.paths = AppPaths::new(dir.join("data"));
        std::fs::create_dir_all(&sovereign_dir).expect("create sovereign dir");
        write_text_file(&sovereign_dir.join("OOHANDOFF.TXT"), "organism_id=abc\nmode=normal")
            .expect("write sovereign receipt");
        write_daily_reports(&ctx, &report_dir, 20, true, false, Some(&sovereign_dir))
            .expect("write daily reports with sovereign");
        let sovereign = std::fs::read_to_string(report_dir.join("sovereign.md")).expect("read sovereign");
        assert!(sovereign.contains("# sovereign summary"));
        assert!(sovereign.contains("- organism_id: abc"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_sync_summary_markdown_reports_alignment() {
        let dir = env::temp_dir().join(format!("oo-host-sync-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create sync dir");
        write_text_file(
            &dir.join("OOHANDOFF.TXT"),
            "organism_id=org-1\nmode=normal\npolicy_enforcement=observe\ncontinuity_epoch=0\nlast_recovery_reason=none",
        ).expect("write receipt");
        let ctx = sample_ctx(vec![]);
        let markdown = render_sync_summary_markdown(&ctx, &dir).expect("render sync summary");
        assert!(markdown.contains("# sync summary"));
        assert!(markdown.contains("- verdict: aligned"));
        assert!(markdown.contains("organism_id"));
        assert!(markdown.contains("## recommended actions"));
        assert!(markdown.contains("No action required"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recommend_sync_actions_reports_host_ahead_guidance() {
        let mismatches = vec![SyncMismatch {
            field: "continuity_epoch".to_string(),
            host_value: "2".to_string(),
            receipt_value: "1".to_string(),
        }];
        let actions = recommend_sync_actions("host_ahead", &mismatches);
        assert!(actions.iter().any(|item| item.contains("catches up")));
        assert!(actions.iter().any(|item| item.contains("continuity_epoch")));
    }

    #[test]
    fn collect_sync_mismatches_returns_only_different_fields() {
        let mut host = BTreeMap::new();
        host.insert("organism_id".to_string(), "org-1".to_string());
        host.insert("mode".to_string(), "normal".to_string());
        let mut receipt = BTreeMap::new();
        receipt.insert("organism_id".to_string(), "org-1".to_string());
        receipt.insert("mode".to_string(), "safe".to_string());
        let mismatches = collect_sync_mismatches(&host, &receipt);
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].field, "mode");
        assert_eq!(mismatches[0].receipt_value, "safe");
    }

    #[test]
    fn write_daily_reports_can_include_sync_bundle() {
        let dir = env::temp_dir().join(format!("oo-host-daily-sync-{}", Uuid::new_v4()));
        let report_dir = dir.join("bundle");
        let sovereign_dir = dir.join("llm-baremetal");
        let ctx = sample_ctx(vec![goal("g1", "inspect me", "pending", 3, 1)]);
        std::fs::create_dir_all(&sovereign_dir).expect("create sovereign dir");
        write_text_file(
            &sovereign_dir.join("OOHANDOFF.TXT"),
            "organism_id=org-1\nmode=normal\npolicy_enforcement=observe\ncontinuity_epoch=0\nlast_recovery_reason=none",
        ).expect("write sovereign receipt");
        write_daily_reports(&ctx, &report_dir, 20, true, true, Some(&sovereign_dir))
            .expect("write daily reports with sync");
        let sync = std::fs::read_to_string(report_dir.join("sync.md")).expect("read sync");
        assert!(sync.contains("# sync summary"));
        assert!(sync.contains("- verdict: aligned"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
