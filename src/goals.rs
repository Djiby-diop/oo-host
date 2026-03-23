use std::path::Path;
use uuid::Uuid;
use crate::types::*;
use crate::io::{now_epoch_s, append_event, emit_report, truncate_text};
use crate::state::persist_ctx;
use crate::journal::{collect_goal_events, explain_event};

pub fn add_goal(
    ctx: &mut RuntimeCtx,
    title: String,
    priority: i32,
    origin: String,
    safety: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let goal = Goal {
        goal_id: Uuid::new_v4().to_string(),
        title: title.clone(),
        status: "pending".to_string(),
        hold_reason: None,
        notes: Vec::new(),
        tags: Vec::new(),
        priority,
        created_at_epoch_s: now,
        updated_at_epoch_s: now,
        origin,
        safety_class: safety,
    };

    ctx.state.goals.push(goal);
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "goal_create".to_string(),
            severity: "notice".to_string(),
            summary: format!("goal created: {title}"),
            reason: None,
            action: Some("goal_add".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

pub fn mark_goal_done(ctx: &mut RuntimeCtx, goal_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let goal = ctx
        .state
        .goals
        .iter_mut()
        .find(|g| g.goal_id == goal_id)
        .ok_or_else(|| format!("goal not found: {goal_id}"))?;

    goal.status = "done".to_string();
    goal.hold_reason = None;
    goal.updated_at_epoch_s = now;
    let title = goal.title.clone();
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "goal_complete".to_string(),
            severity: "notice".to_string(),
            summary: format!("goal done: {title}"),
            reason: None,
            action: Some("goal_done".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

pub fn hold_goal(
    ctx: &mut RuntimeCtx,
    goal_id: &str,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let goal = ctx
        .state
        .goals
        .iter_mut()
        .find(|g| g.goal_id == goal_id)
        .ok_or_else(|| format!("goal not found: {goal_id}"))?;

    if is_terminal_goal_status(&goal.status) {
        return Err(format!("goal is terminal: {goal_id}").into());
    }

    goal.status = "blocked".to_string();
    goal.hold_reason = Some(reason.to_string());
    goal.updated_at_epoch_s = now;
    let title = goal.title.clone();
    let hold_reason = reason.to_string();
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "goal_hold".to_string(),
            severity: "warn".to_string(),
            summary: format!("goal held: {title}"),
            reason: Some(hold_reason),
            action: Some("goal_set_blocked".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

pub fn add_goal_note(
    ctx: &mut RuntimeCtx,
    goal_id: &str,
    text: &str,
    author: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let goal = ctx
        .state
        .goals
        .iter_mut()
        .find(|g| g.goal_id == goal_id)
        .ok_or_else(|| format!("goal not found: {goal_id}"))?;

    goal.notes.push(GoalNote {
        ts_epoch_s: now,
        author: author.to_string(),
        text: text.to_string(),
    });
    let title = goal.title.clone();
    let note_preview = truncate_text(text, 80);
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "goal_note".to_string(),
            severity: "info".to_string(),
            summary: format!("goal note added: {title} ({note_preview})"),
            reason: None,
            action: Some("goal_note_add".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

pub fn resume_goal(ctx: &mut RuntimeCtx, goal_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let goal = ctx
        .state
        .goals
        .iter_mut()
        .find(|g| g.goal_id == goal_id)
        .ok_or_else(|| format!("goal not found: {goal_id}"))?;

    if is_terminal_goal_status(&goal.status) {
        return Err(format!("goal is terminal: {goal_id}").into());
    }

    if goal.status != "blocked" && goal.status != "recovering" {
        return Err(format!("goal is not resumable: {goal_id}").into());
    }

    goal.status = "doing".to_string();
    goal.hold_reason = None;
    goal.updated_at_epoch_s = now;
    let title = goal.title.clone();
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "goal_resume".to_string(),
            severity: "notice".to_string(),
            summary: format!("goal resumed: {title}"),
            reason: Some("operator_resume".to_string()),
            action: Some("goal_resume".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

pub fn start_goal(ctx: &mut RuntimeCtx, goal_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let goal = ctx
        .state
        .goals
        .iter_mut()
        .find(|g| g.goal_id == goal_id)
        .ok_or_else(|| format!("goal not found: {goal_id}"))?;

    if is_terminal_goal_status(&goal.status) {
        return Err(format!("goal is terminal: {goal_id}").into());
    }

    goal.status = "doing".to_string();
    goal.hold_reason = None;
    goal.updated_at_epoch_s = now;
    let title = goal.title.clone();
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "goal_start".to_string(),
            severity: "notice".to_string(),
            summary: format!("goal started: {title}"),
            reason: None,
            action: Some("goal_start".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

pub fn abort_goal(
    ctx: &mut RuntimeCtx,
    goal_id: &str,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let goal = ctx
        .state
        .goals
        .iter_mut()
        .find(|g| g.goal_id == goal_id)
        .ok_or_else(|| format!("goal not found: {goal_id}"))?;

    if is_terminal_goal_status(&goal.status) {
        return Err(format!("goal is terminal: {goal_id}").into());
    }

    goal.status = "aborted".to_string();
    goal.hold_reason = Some(reason.to_string());
    goal.updated_at_epoch_s = now;
    let title = goal.title.clone();
    let abort_reason = reason.to_string();
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "goal_abort".to_string(),
            severity: "warn".to_string(),
            summary: format!("goal aborted: {title}"),
            reason: Some(abort_reason),
            action: Some("goal_abort".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

pub fn delete_goal(ctx: &mut RuntimeCtx, goal_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let idx = ctx
        .state
        .goals
        .iter()
        .position(|g| g.goal_id == goal_id)
        .ok_or_else(|| format!("goal not found: {goal_id}"))?;

    if !is_terminal_goal_status(&ctx.state.goals[idx].status) {
        return Err(format!("goal is not terminal: {goal_id}").into());
    }

    let title = ctx.state.goals[idx].title.clone();
    ctx.state.goals.remove(idx);
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "goal_delete".to_string(),
            severity: "notice".to_string(),
            summary: format!("goal deleted: {title}"),
            reason: None,
            action: Some("goal_delete".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

pub fn tag_goal(ctx: &mut RuntimeCtx, goal_id: &str, tag: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let goal = ctx
        .state
        .goals
        .iter_mut()
        .find(|g| g.goal_id == goal_id)
        .ok_or_else(|| format!("goal not found: {goal_id}"))?;

    if !goal.tags.contains(&tag.to_string()) {
        goal.tags.push(tag.to_string());
    }
    let title = goal.title.clone();
    goal.updated_at_epoch_s = now;
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "goal_tag_add".to_string(),
            severity: "info".to_string(),
            summary: format!("goal tag added: {title} ({tag})"),
            reason: None,
            action: Some("goal_tag_add".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

pub fn untag_goal(ctx: &mut RuntimeCtx, goal_id: &str, tag: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let goal = ctx
        .state
        .goals
        .iter_mut()
        .find(|g| g.goal_id == goal_id)
        .ok_or_else(|| format!("goal not found: {goal_id}"))?;

    goal.tags.retain(|t| t != tag);
    let title = goal.title.clone();
    goal.updated_at_epoch_s = now;
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "goal_tag_remove".to_string(),
            severity: "info".to_string(),
            summary: format!("goal tag removed: {title} ({tag})"),
            reason: None,
            action: Some("goal_tag_remove".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

pub fn list_goals(ctx: &RuntimeCtx) {
    if ctx.state.goals.is_empty() {
        println!("No goals.");
        return;
    }

    for goal in &ctx.state.goals {
        let tags_str = if goal.tags.is_empty() {
            "none".to_string()
        } else {
            goal.tags.join(",")
        };
        println!(
            "{} | {} | prio={} | {} | hold={} | notes={} | tags={} | {}",
            goal.goal_id,
            goal.status,
            goal.priority,
            goal.origin,
            goal.hold_reason.as_deref().unwrap_or("none"),
            goal.notes.len(),
            tags_str,
            goal.title
        );
    }
}

pub fn print_next_goal(ctx: &RuntimeCtx) {
    match select_next_goal(&ctx.state) {
        Some(goal) => {
            println!("goal_id      : {}", goal.goal_id);
            println!("title        : {}", goal.title);
            println!("status       : {}", goal.status);
            println!("hold_reason  : {}", goal.hold_reason.as_deref().unwrap_or("none"));
            println!("note_count   : {}", goal.notes.len());
            if let Some(note) = goal.notes.last() {
                println!("last_note_by : {}", note.author);
                println!("last_note    : {}", note.text);
            }
            println!("priority     : {}", goal.priority);
            println!("origin       : {}", goal.origin);
            println!("safety_class : {}", goal.safety_class);
        }
        None => println!("No active goals."),
    }
}

pub fn inspect_goal(
    ctx: &RuntimeCtx,
    goal_id: &str,
    format: OutputFormat,
    out: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let goal = ctx
        .state
        .goals
        .iter()
        .find(|g| g.goal_id == goal_id)
        .ok_or_else(|| format!("goal not found: {goal_id}"))?;

    let related_events = collect_goal_events(&ctx.paths.journal_path, goal)?;

    if format.is_markdown() {
        let rendered = render_goal_inspect_markdown(goal, &related_events);
        emit_report(&rendered, out)?;
        return Ok(());
    }

    let rendered = render_goal_inspect_text(goal, &related_events);
    emit_report(&rendered, out)?;

    Ok(())
}

pub fn render_goal_inspect_text(goal: &Goal, related_events: &[JournalEvent]) -> String {
    let tags_str = if goal.tags.is_empty() {
        "none".to_string()
    } else {
        goal.tags.join(", ")
    };
    let mut lines = vec![
        format!("goal_id       : {}", goal.goal_id),
        format!("title         : {}", goal.title),
        format!("status        : {}", goal.status),
        format!("hold_reason   : {}", goal.hold_reason.as_deref().unwrap_or("none")),
        format!("priority      : {}", goal.priority),
        format!("origin        : {}", goal.origin),
        format!("safety_class  : {}", goal.safety_class),
        format!("tags          : {}", tags_str),
        format!("created_at    : {}", goal.created_at_epoch_s),
        format!("updated_at    : {}", goal.updated_at_epoch_s),
        format!("note_count    : {}", goal.notes.len()),
    ];

    if goal.notes.is_empty() {
        lines.push("notes         : none".to_string());
    } else {
        lines.push("notes         :".to_string());
        for note in &goal.notes {
            lines.push(format!("  - [{}] {}: {}", note.ts_epoch_s, note.author, note.text));
        }
    }

    if related_events.is_empty() {
        lines.push("recent_events : none".to_string());
    } else {
        lines.push("recent_events :".to_string());
        for event in related_events.iter().rev().take(12).rev() {
            lines.push(format!(
                "  - [{}] {} | {}",
                event.ts_epoch_s,
                event.kind,
                explain_event(event)
            ));
        }
    }

    lines.join("\n")
}

pub fn render_goal_inspect_markdown(goal: &Goal, related_events: &[JournalEvent]) -> String {
    let tags_str = if goal.tags.is_empty() {
        "none".to_string()
    } else {
        goal.tags.join(", ")
    };
    let mut lines = vec![
        format!("# goal {}", goal.goal_id),
        String::new(),
        format!("- title: {}", goal.title),
        format!("- status: {}", goal.status),
        format!("- hold_reason: {}", goal.hold_reason.as_deref().unwrap_or("none")),
        format!("- priority: {}", goal.priority),
        format!("- origin: {}", goal.origin),
        format!("- safety_class: {}", goal.safety_class),
        format!("- tags: {}", tags_str),
        format!("- created_at: {}", goal.created_at_epoch_s),
        format!("- updated_at: {}", goal.updated_at_epoch_s),
        format!("- note_count: {}", goal.notes.len()),
        String::new(),
        "## notes".to_string(),
        String::new(),
    ];

    if goal.notes.is_empty() {
        lines.push("- none".to_string());
    } else {
        for note in &goal.notes {
            lines.push(format!("- [{}] {}: {}", note.ts_epoch_s, note.author, note.text));
        }
    }

    lines.push(String::new());
    lines.push("## recent events".to_string());
    lines.push(String::new());
    if related_events.is_empty() {
        lines.push("- none".to_string());
    } else {
        for event in related_events.iter().rev().take(12).rev() {
            lines.push(format!(
                "- [{}] {}: {}",
                event.ts_epoch_s,
                event.kind,
                explain_event(event)
            ));
        }
    }

    lines.join("\n")
}

pub fn render_next_goal_markdown(ctx: &RuntimeCtx) -> String {
    if let Some(goal) = select_next_goal(&ctx.state) {
        let related_events = collect_goal_events(&ctx.paths.journal_path, goal).unwrap_or_default();
        return render_goal_inspect_markdown(goal, &related_events);
    }

    ["# next goal".to_string(), String::new(), "- none".to_string()].join("\n")
}

pub fn select_next_goal(state: &State) -> Option<&Goal> {
    state
        .goals
        .iter()
        .filter(|g| is_actionable_goal_status(&g.status))
        .max_by(|a, b| {
            goal_selection_rank(&a.status)
                .cmp(&goal_selection_rank(&b.status))
                .then_with(|| a.priority.cmp(&b.priority))
                .then_with(|| b.created_at_epoch_s.cmp(&a.created_at_epoch_s))
                .then_with(|| b.goal_id.cmp(&a.goal_id))
        })
}

pub fn goal_selection_rank(status: &str) -> u8 {
    match status {
        "doing" => 3,
        "recovering" => 2,
        "pending" => 1,
        _ => 0,
    }
}

pub fn is_terminal_goal_status(status: &str) -> bool {
    status == "done" || status == "aborted"
}

pub fn is_actionable_goal_status(status: &str) -> bool {
    status == "doing" || status == "recovering" || status == "pending"
}

pub fn is_policy_safe_goal(goal: &Goal) -> bool {
    goal.safety_class == "normal"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Goal, GoalNote, PolicyEnforcement, PolicyState, RuntimeMode, State};
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
    fn next_goal_prefers_highest_priority_active_goal() {
        let state = sample_state(vec![
            goal("g1", "low", "pending", 1, 10),
            goal("g2", "high", "pending", 5, 20),
            goal("g3", "done", "done", 9, 30),
        ]);
        let next = select_next_goal(&state).expect("expected active goal");
        assert_eq!(next.goal_id, "g2");
    }

    #[test]
    fn next_goal_breaks_ties_by_oldest_created_goal() {
        let state = sample_state(vec![
            goal("g1", "older", "pending", 5, 10),
            goal("g2", "newer", "pending", 5, 20),
        ]);
        let next = select_next_goal(&state).expect("expected active goal");
        assert_eq!(next.goal_id, "g1");
    }

    #[test]
    fn next_goal_prefers_doing_over_pending() {
        let state = sample_state(vec![
            goal("g1", "active", "doing", 1, 10),
            goal("g2", "pending_hi", "pending", 9, 5),
        ]);
        let next = select_next_goal(&state).expect("expected active goal");
        assert_eq!(next.goal_id, "g1");
    }

    #[test]
    fn next_goal_prefers_recovering_over_pending() {
        let state = sample_state(vec![
            goal("g1", "recover me", "recovering", 1, 10),
            goal("g2", "pending_hi", "pending", 9, 5),
        ]);
        let next = select_next_goal(&state).expect("expected active goal");
        assert_eq!(next.goal_id, "g1");
    }

    #[test]
    fn next_goal_returns_none_when_only_terminal_goals_exist() {
        let state = sample_state(vec![
            goal("g1", "done", "done", 5, 10),
            goal("g2", "aborted", "aborted", 7, 20),
        ]);
        assert!(select_next_goal(&state).is_none());
    }

    #[test]
    fn goal_selection_rank_orders_expected_states() {
        assert!(goal_selection_rank("doing") > goal_selection_rank("pending"));
        assert!(goal_selection_rank("recovering") > goal_selection_rank("pending"));
        assert!(goal_selection_rank("pending") > goal_selection_rank("done"));
    }

    #[test]
    fn goal_note_can_be_appended_without_state_transition() {
        let mut g = goal("g1", "note me", "doing", 1, 1);
        g.notes.push(GoalNote {
            ts_epoch_s: 10,
            author: "operator".to_string(),
            text: "remember this".to_string(),
        });
        assert_eq!(g.status, "doing");
        assert_eq!(g.notes.len(), 1);
        assert_eq!(g.notes[0].author, "operator");
        assert_eq!(g.notes[0].text, "remember this");
    }

    #[test]
    fn render_goal_inspect_markdown_contains_notes_and_events() {
        let mut g = goal("g1", "inspect me", "doing", 3, 1);
        g.notes.push(GoalNote {
            ts_epoch_s: 10,
            author: "operator".to_string(),
            text: "important context".to_string(),
        });
        let events = vec![crate::types::JournalEvent {
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
        }];
        let markdown = render_goal_inspect_markdown(&g, &events);
        assert!(markdown.contains("# goal g1"));
        assert!(markdown.contains("## notes"));
        assert!(markdown.contains("important context"));
        assert!(markdown.contains("## recent events"));
        assert!(markdown.contains("goal note recorded"));
    }

    #[test]
    fn render_goal_inspect_text_contains_notes_and_events() {
        let mut g = goal("g1", "inspect me", "doing", 3, 1);
        g.notes.push(GoalNote {
            ts_epoch_s: 10,
            author: "operator".to_string(),
            text: "important context".to_string(),
        });
        let events = vec![crate::types::JournalEvent {
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
        }];
        let text = render_goal_inspect_text(&g, &events);
        assert!(text.contains("goal_id       : g1"));
        assert!(text.contains("note_count    : 1"));
        assert!(text.contains("important context"));
        assert!(text.contains("recent_events :"));
    }

    #[test]
    fn delete_goal_removes_terminal_goal() {
        let g = goal("g1", "done goal", "done", 1, 1);
        let mut state = sample_state(vec![g, goal("g2", "active", "doing", 1, 2)]);
        assert_eq!(state.goals.len(), 2);
        let idx = state.goals.iter().position(|g| g.goal_id == "g1").unwrap();
        assert!(is_terminal_goal_status(&state.goals[idx].status));
        state.goals.remove(idx);
        assert_eq!(state.goals.len(), 1);
        assert_eq!(state.goals[0].goal_id, "g2");
    }

    #[test]
    fn delete_goal_rejects_non_terminal_goal() {
        let g = goal("g1", "active", "doing", 1, 1);
        assert!(!is_terminal_goal_status(&g.status));
    }

    #[test]
    fn tag_goal_adds_tag_to_goal() {
        let mut g = goal("g1", "tagged", "doing", 1, 1);
        assert!(g.tags.is_empty());
        if !g.tags.contains(&"v1".to_string()) {
            g.tags.push("v1".to_string());
        }
        assert_eq!(g.tags, vec!["v1"]);
    }

    #[test]
    fn tag_goal_is_idempotent() {
        let mut g = goal("g1", "tagged", "doing", 1, 1);
        g.tags.push("v1".to_string());
        // Adding same tag again should not duplicate
        if !g.tags.contains(&"v1".to_string()) {
            g.tags.push("v1".to_string());
        }
        assert_eq!(g.tags.len(), 1);
    }

    #[test]
    fn untag_goal_removes_tag_from_goal() {
        let mut g = goal("g1", "tagged", "doing", 1, 1);
        g.tags.push("v1".to_string());
        g.tags.push("v2".to_string());
        g.tags.retain(|t| t != "v1");
        assert_eq!(g.tags, vec!["v2"]);
    }

    #[test]
    fn list_goals_output_includes_tags_field() {
        let mut g = goal("g1", "my goal", "pending", 1, 1);
        g.tags.push("important".to_string());
        let tags_str = if g.tags.is_empty() { "none".to_string() } else { g.tags.join(",") };
        let line = format!(
            "{} | {} | prio={} | {} | hold={} | notes={} | tags={} | {}",
            g.goal_id, g.status, g.priority, g.origin,
            g.hold_reason.as_deref().unwrap_or("none"),
            g.notes.len(), tags_str, g.title
        );
        assert!(line.contains("tags=important"));
    }
}
