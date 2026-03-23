use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use crate::types::*;
use crate::io::{now_epoch_s, read_json, write_json, append_event, detect_habitat};

pub fn bootstrap(data_dir: PathBuf) -> Result<RuntimeCtx, Box<dyn std::error::Error>> {
    let paths = AppPaths::new(data_dir);
    fs::create_dir_all(&paths.data_dir)?;

    let identity = load_or_create_identity(&paths.identity_path)?;
    let runtime_instance_id = Uuid::new_v4().to_string();
    let mut state = load_or_create_state(&paths.state_path)?;
    state.boot_or_start_count += 1;
    state.last_clean_shutdown = false;
    state.last_started_at_epoch_s = now_epoch_s();
    save_state(&paths.state_path, &state)?;
    save_recovery_snapshot(&paths.recovery_path, &state)?;

    append_event(
        &paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now_epoch_s(),
            organism_id: identity.organism_id.clone(),
            runtime_habitat: identity.runtime_habitat.clone(),
            runtime_instance_id: runtime_instance_id.clone(),
            kind: "startup".to_string(),
            severity: "info".to_string(),
            summary: "host runtime command start".to_string(),
            reason: None,
            action: Some("bootstrap".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: state.continuity_epoch,
        },
    )?;

    Ok(RuntimeCtx {
        paths,
        identity,
        state,
        runtime_instance_id,
    })
}

pub fn load_or_create_identity(path: &Path) -> Result<Identity, Box<dyn std::error::Error>> {
    if path.exists() {
        return Ok(read_json(path)?);
    }

    let identity = Identity {
        organism_id: Uuid::new_v4().to_string(),
        genesis_id: Uuid::new_v4().to_string(),
        runtime_habitat: detect_habitat().to_string(),
        created_at_epoch_s: now_epoch_s(),
    };
    write_json(path, &identity)?;
    Ok(identity)
}

pub fn load_or_create_state(path: &Path) -> Result<State, Box<dyn std::error::Error>> {
    if path.exists() {
        return Ok(read_json(path)?);
    }

    let state = State {
        boot_or_start_count: 0,
        continuity_epoch: 0,
        last_clean_shutdown: true,
        last_recovery_reason: None,
        last_started_at_epoch_s: now_epoch_s(),
        mode: RuntimeMode::Normal,
        policy: PolicyState {
            safe_first: true,
            deny_by_default: true,
            llm_advisory_only: true,
            enforcement: PolicyEnforcement::Observe,
        },
        workers: Vec::new(),
        goals: Vec::new(),
    };
    write_json(path, &state)?;
    Ok(state)
}

pub fn default_runtime_mode() -> RuntimeMode {
    RuntimeMode::Normal
}

pub fn default_policy_state() -> PolicyState {
    PolicyState {
        safe_first: true,
        deny_by_default: true,
        llm_advisory_only: true,
        enforcement: PolicyEnforcement::Observe,
    }
}

pub fn persist_ctx(ctx: &RuntimeCtx) -> Result<(), Box<dyn std::error::Error>> {
    save_state(&ctx.paths.state_path, &ctx.state)?;
    save_recovery_snapshot(&ctx.paths.recovery_path, &ctx.state)?;
    Ok(())
}

pub fn recover_state(ctx: &mut RuntimeCtx) -> Result<(), Box<dyn std::error::Error>> {
    let mut recovered: State = read_json(&ctx.paths.recovery_path)?;
    recovered.continuity_epoch += 1;
    recovered.last_recovery_reason = Some("manual_recover".to_string());
    recovered.last_clean_shutdown = false;
    recovered.last_started_at_epoch_s = now_epoch_s();
    ctx.state = recovered;
    persist_ctx(ctx)?;

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now_epoch_s(),
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "recovery".to_string(),
            severity: "warn".to_string(),
            summary: "manual recovery snapshot restored".to_string(),
            reason: Some("manual_recover".to_string()),
            action: Some("recovery_restore".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;
    Ok(())
}

pub fn save_state(path: &Path, state: &State) -> Result<(), Box<dyn std::error::Error>> {
    write_json(path, state)
}

pub fn save_recovery_snapshot(path: &Path, state: &State) -> Result<(), Box<dyn std::error::Error>> {
    write_json(path, state)
}
