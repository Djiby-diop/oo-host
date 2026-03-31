use std::path::Path;
use crate::io::{emit_report, now_epoch_s, read_all_events};
use crate::types::{OutputFormat, PolicyEnforcement, RuntimeCtx, RuntimeMode};

/// Generate a first-person prose narrative of the organism's existence, derived
/// deterministically from its state and journal — no LLM involved.
pub fn narrate(
    ctx: &RuntimeCtx,
    format: OutputFormat,
    out: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let events = read_all_events(&ctx.paths.journal_path)?;
    let event_count = events.len();

    let id = &ctx.identity.organism_id;
    let born = ctx.identity.created_at_epoch_s;
    let habitat = &ctx.identity.runtime_habitat;
    let boot_count = ctx.state.boot_or_start_count;
    let epoch = ctx.state.continuity_epoch;

    let active_goals: Vec<_> = ctx
        .state
        .goals
        .iter()
        .filter(|g| g.status != "done" && g.status != "aborted")
        .collect();

    // Pick the highest-priority "doing" goal as current focus
    let next_goal = {
        let mut doing: Vec<_> = active_goals
            .iter()
            .filter(|g| g.status == "doing")
            .collect();
        doing.sort_by(|a, b| b.priority.cmp(&a.priority));
        doing.first().map(|g| **g)
    };

    let workers = &ctx.state.workers;
    let now = now_epoch_s();
    let alive_count = workers
        .iter()
        .filter(|w| {
            let threshold = w.stale_after_s.unwrap_or(300);
            now.saturating_sub(w.last_heartbeat_epoch_s) <= threshold
        })
        .count();
    let stale_count = workers.len().saturating_sub(alive_count);

    let peers = &ctx.state.federation;
    let enforcement = ctx.state.policy.enforcement.as_str();

    let mut sections: Vec<String> = Vec::new();

    // Section 1 — Identity and continuity
    let mut s1 = format!(
        "I am {}, born at epoch {}.\nI have been started {} times across my existence.\nMy habitat is {}.",
        id, born, boot_count, habitat
    );
    if epoch == 0 {
        s1.push_str(
            "\n\nI have never recovered from an interruption — my continuity is pristine.",
        );
    } else {
        let reason = ctx.state.last_recovery_reason.as_deref().unwrap_or("unknown");
        s1.push_str(&format!(
            "\n\nI have recovered {} times. My last recovery was due to: {}.",
            epoch, reason
        ));
    }
    sections.push(s1);

    // Section 2 — Current mode
    let mut s2 = format!("My current mode is {}.", ctx.state.mode.as_str());
    match &ctx.state.mode {
        RuntimeMode::Degraded => {
            s2.push_str(
                "\nI am operating under stress — some of my workers have gone silent.",
            );
        }
        RuntimeMode::Safe => {
            s2.push_str("\nI am in a safe, restricted state.");
        }
        RuntimeMode::Normal => {}
    }
    sections.push(s2);

    // Section 3 — Goals
    let mut s3 = format!("I am pursuing {} active goals.", active_goals.len());
    if let Some(g) = next_goal {
        s3.push_str(&format!(
            "\nMy current focus is: \"{}\" (priority {}, safety_class {}).",
            g.title, g.priority, g.safety_class
        ));
    } else if active_goals.is_empty() {
        s3.push_str("\nI have no active goals — I am waiting.");
    }
    sections.push(s3);

    // Section 4 — Workers (only if any registered)
    if !workers.is_empty() {
        sections.push(format!(
            "I am supported by {} workers. {} are alive, {} have gone silent.",
            workers.len(),
            alive_count,
            stale_count
        ));
    }

    // Section 5 — Federation (only if any peers)
    if !peers.is_empty() {
        let peer_ids: Vec<&str> = peers.iter().map(|p| p.peer_id.as_str()).collect();
        sections.push(format!(
            "I am federated with {} peer organisms: {}.",
            peers.len(),
            peer_ids.join(", ")
        ));
    }

    // Section 6 — Journal and policy
    let mut s6 = format!(
        "I have written {} journal entries since my genesis.\nMy policy posture is {}.",
        event_count, enforcement
    );
    if matches!(&ctx.state.policy.enforcement, PolicyEnforcement::Enforce) {
        s6.push_str(
            "\nI operate under strict governance — non-normal goals are held pending review.",
        );
    }
    s6.push_str("\n\nMy continuity fingerprint is evolving with every breath.");
    sections.push(s6);

    let text = if format.is_markdown() {
        let header = format!("# organism narrative — {}\n", id);
        let body = sections.join("\n\n---\n\n");
        format!("{}\n{}", header, body)
    } else {
        sections.join("\n\n")
    };

    emit_report(&text, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AppPaths, Goal, Identity, OutputFormat, PolicyEnforcement, PolicyState, RuntimeCtx, RuntimeMode, State};
    use std::path::PathBuf;

    fn make_ctx(
        mode: RuntimeMode,
        epoch: u64,
        goals: Vec<Goal>,
        boot_count: u64,
        recovery_reason: Option<String>,
    ) -> RuntimeCtx {
        RuntimeCtx {
            // Non-existent path → read_all_events returns empty Vec
            paths: AppPaths::new(PathBuf::from(format!(
                "C:\\nonexistent\\narrate-test-{}",
                uuid::Uuid::new_v4()
            ))),
            identity: Identity {
                organism_id: "org-narrate-test".to_string(),
                genesis_id: "gen-1".to_string(),
                runtime_habitat: "host_test".to_string(),
                created_at_epoch_s: 12345,
            },
            state: State {
                boot_or_start_count: boot_count,
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
        }
    }

    fn make_goal(status: &str, priority: i32) -> Goal {
        Goal {
            goal_id: uuid::Uuid::new_v4().to_string(),
            title: "test-goal".to_string(),
            status: status.to_string(),
            hold_reason: None,
            notes: Vec::new(),
            tags: Vec::new(),
            priority,
            created_at_epoch_s: 0,
            updated_at_epoch_s: 0,
            origin: "operator".to_string(),
            safety_class: "normal".to_string(),
            delegated_to: None,
        }
    }

    #[test]
    fn narrative_contains_organism_id_and_habitat() {
        let ctx = make_ctx(RuntimeMode::Normal, 0, Vec::new(), 3, None);
        let dir = std::env::temp_dir().join(format!("oo-narrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("narrate.txt");
        narrate(&ctx, OutputFormat::Text, Some(&out)).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("org-narrate-test"));
        assert!(content.contains("host_test"));
        assert!(content.contains("12345"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn narrative_pristine_continuity_text_when_epoch_zero() {
        let ctx = make_ctx(RuntimeMode::Normal, 0, Vec::new(), 1, None);
        let dir = std::env::temp_dir().join(format!("oo-narrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("n.txt");
        narrate(&ctx, OutputFormat::Text, Some(&out)).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("continuity is pristine"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn narrative_recovery_text_when_epoch_nonzero() {
        let ctx = make_ctx(
            RuntimeMode::Normal, 2, Vec::new(), 5,
            Some("manual_recover".to_string()),
        );
        let dir = std::env::temp_dir().join(format!("oo-narrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("n.txt");
        narrate(&ctx, OutputFormat::Text, Some(&out)).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("recovered 2 times"));
        assert!(content.contains("manual_recover"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn narrative_degraded_mode_mentions_stress() {
        let ctx = make_ctx(RuntimeMode::Degraded, 0, Vec::new(), 1, None);
        let dir = std::env::temp_dir().join(format!("oo-narrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("n.txt");
        narrate(&ctx, OutputFormat::Text, Some(&out)).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("operating under stress"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn narrative_waiting_when_no_active_goals() {
        let ctx = make_ctx(RuntimeMode::Normal, 0, Vec::new(), 1, None);
        let dir = std::env::temp_dir().join(format!("oo-narrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("n.txt");
        narrate(&ctx, OutputFormat::Text, Some(&out)).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("I am waiting"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn narrative_markdown_has_header_and_separators() {
        let ctx = make_ctx(RuntimeMode::Normal, 0, Vec::new(), 1, None);
        let dir = std::env::temp_dir().join(format!("oo-narrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("n.md");
        narrate(&ctx, OutputFormat::Markdown, Some(&out)).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("# organism narrative"));
        assert!(content.contains("---"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn narrative_current_focus_when_doing_goal_present() {
        let goals = vec![make_goal("doing", 5)];
        let ctx = make_ctx(RuntimeMode::Normal, 0, goals, 1, None);
        let dir = std::env::temp_dir().join(format!("oo-narrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("n.txt");
        narrate(&ctx, OutputFormat::Text, Some(&out)).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("My current focus is"));
        assert!(content.contains("test-goal"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn narrative_fingerprint_line_always_present() {
        let ctx = make_ctx(RuntimeMode::Normal, 0, Vec::new(), 1, None);
        let dir = std::env::temp_dir().join(format!("oo-narrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("n.txt");
        narrate(&ctx, OutputFormat::Text, Some(&out)).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("continuity fingerprint"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
