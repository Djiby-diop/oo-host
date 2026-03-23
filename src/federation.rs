use std::path::Path;
use uuid::Uuid;
use crate::types::{FederationPeer, JournalEvent, RuntimeCtx};
use crate::io::{now_epoch_s, append_event, read_json};
use crate::state::persist_ctx;

pub const PEER_STALE_AFTER_S: u64 = 86400; // 24h

pub fn register_peer(
    ctx: &mut RuntimeCtx,
    peer_id: &str,
    habitat: &str,
    label: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    if let Some(peer) = ctx.state.federation.iter_mut().find(|p| p.peer_id == peer_id) {
        peer.habitat = habitat.to_string();
        if let Some(l) = label {
            peer.label = Some(l.to_string());
        }
        peer.last_seen_epoch_s = now;
        peer.status = effective_peer_status(peer, now).to_string();
    } else {
        ctx.state.federation.push(FederationPeer {
            peer_id: peer_id.to_string(),
            habitat: habitat.to_string(),
            label: label.map(|l| l.to_string()),
            last_seen_epoch_s: now,
            last_export_path: None,
            status: "active".to_string(),
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
            kind: "federation_peer_register".to_string(),
            severity: "info".to_string(),
            summary: format!("federation peer registered: {peer_id} ({habitat})"),
            reason: None,
            action: Some("register_peer".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
            signature: None,
        },
    )?;

    Ok(())
}

pub fn list_peers(ctx: &RuntimeCtx) {
    if ctx.state.federation.is_empty() {
        println!("No federation peers.");
        return;
    }
    let now = now_epoch_s();
    for peer in &ctx.state.federation {
        let status = effective_peer_status(peer, now);
        println!(
            "{} | {} | {} | {} | last_seen={}",
            peer.peer_id,
            peer.habitat,
            peer.label.as_deref().unwrap_or("(no label)"),
            status,
            peer.last_seen_epoch_s,
        );
    }
}

pub fn import_peer_export(
    ctx: &mut RuntimeCtx,
    export_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read the sovereign export to find the peer_id (organism_id of the peer)
    let export: serde_json::Value = read_json(export_path)?;
    let peer_id = export["organism_id"]
        .as_str()
        .ok_or("export missing organism_id")?
        .to_string();

    let now = now_epoch_s();
    let export_path_str = export_path.to_string_lossy().to_string();

    if let Some(peer) = ctx.state.federation.iter_mut().find(|p| p.peer_id == peer_id) {
        peer.last_seen_epoch_s = now;
        peer.last_export_path = Some(export_path_str.clone());
        peer.status = effective_peer_status(peer, now).to_string();
    } else {
        let habitat = export["runtime_habitat"].as_str().unwrap_or("unknown").to_string();
        ctx.state.federation.push(FederationPeer {
            peer_id: peer_id.clone(),
            habitat,
            label: None,
            last_seen_epoch_s: now,
            last_export_path: Some(export_path_str.clone()),
            status: "active".to_string(),
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
            kind: "federation_import".to_string(),
            severity: "info".to_string(),
            summary: format!("federation export imported from peer: {peer_id}"),
            reason: None,
            action: Some("import_peer_export".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
            signature: None,
        },
    )?;

    println!("OK: imported export from peer {peer_id}");
    Ok(())
}

pub fn effective_peer_status(peer: &FederationPeer, now: u64) -> &'static str {
    if now.saturating_sub(peer.last_seen_epoch_s) <= PEER_STALE_AFTER_S {
        "active"
    } else {
        "stale"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        AppPaths, FederationPeer, Identity, PolicyEnforcement, PolicyState, RuntimeCtx, RuntimeMode, State,
    };

    fn make_peer(last_seen_epoch_s: u64) -> FederationPeer {
        FederationPeer {
            peer_id: "peer-1".to_string(),
            habitat: "host_linux".to_string(),
            label: None,
            last_seen_epoch_s,
            last_export_path: None,
            status: "unknown".to_string(),
        }
    }

    #[test]
    fn effective_peer_status_active_within_24h() {
        let now = 100_000u64;
        let peer = make_peer(now - 3600); // 1h ago
        assert_eq!(effective_peer_status(&peer, now), "active");
    }

    #[test]
    fn effective_peer_status_stale_after_24h() {
        let now = 200_000u64;
        let peer = make_peer(now - PEER_STALE_AFTER_S - 1);
        assert_eq!(effective_peer_status(&peer, now), "stale");
    }

    #[test]
    fn effective_peer_status_exactly_at_boundary_is_active() {
        let now = 200_000u64;
        let peer = make_peer(now - PEER_STALE_AFTER_S);
        assert_eq!(effective_peer_status(&peer, now), "active");
    }

    fn make_ctx() -> (RuntimeCtx, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("oo-fed-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = AppPaths::new(dir.clone());
        let ctx = RuntimeCtx {
            paths,
            identity: Identity {
                organism_id: "org-1".to_string(),
                genesis_id: "gen-1".to_string(),
                runtime_habitat: "host_test".to_string(),
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
                workers: Vec::new(),
                goals: Vec::new(),
                federation: Vec::new(),
            },
            runtime_instance_id: "run-1".to_string(),
        };
        (ctx, dir)
    }

    #[test]
    fn register_peer_adds_new_peer() {
        let (mut ctx, dir) = make_ctx();
        register_peer(&mut ctx, "peer-abc", "host_linux", Some("my peer")).unwrap();
        assert_eq!(ctx.state.federation.len(), 1);
        assert_eq!(ctx.state.federation[0].peer_id, "peer-abc");
        assert_eq!(ctx.state.federation[0].habitat, "host_linux");
        assert_eq!(ctx.state.federation[0].label.as_deref(), Some("my peer"));
        assert_eq!(ctx.state.federation[0].status, "active");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn register_peer_updates_existing_peer() {
        let (mut ctx, dir) = make_ctx();
        register_peer(&mut ctx, "peer-abc", "host_linux", None).unwrap();
        register_peer(&mut ctx, "peer-abc", "host_windows", Some("updated")).unwrap();
        assert_eq!(ctx.state.federation.len(), 1);
        assert_eq!(ctx.state.federation[0].habitat, "host_windows");
        assert_eq!(ctx.state.federation[0].label.as_deref(), Some("updated"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
