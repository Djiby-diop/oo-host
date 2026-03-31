use crate::io::now_epoch_s;
use crate::types::{OutputFormat, PolicyEnforcement, RuntimeCtx, RuntimeMode};

pub struct VitalSignal {
    pub name: String,
    pub value: String,
    pub contribution: i8,
    pub status: &'static str,
}

pub struct OrganismVitals {
    pub score: u8,
    pub pulse: &'static str,
    pub signals: Vec<VitalSignal>,
}

pub fn compute_vitals(ctx: &RuntimeCtx, now: u64) -> OrganismVitals {
    let mut signals: Vec<VitalSignal> = Vec::new();
    let mut total: i32 = 0;

    // runtime_mode
    {
        let (value, contrib, status): (&str, i8, &'static str) = match &ctx.state.mode {
            RuntimeMode::Normal => ("normal", 20, "healthy"),
            RuntimeMode::Degraded => ("degraded", -20, "critical"),
            RuntimeMode::Safe => ("safe", -20, "critical"),
        };
        total += contrib as i32;
        signals.push(VitalSignal {
            name: "runtime_mode".to_string(),
            value: value.to_string(),
            contribution: contrib,
            status,
        });
    }

    // worker_health
    {
        let workers = &ctx.state.workers;
        let stale_threshold_default = 300u64;
        let alive: Vec<_> = workers
            .iter()
            .filter(|w| {
                let threshold = w.stale_after_s.unwrap_or(stale_threshold_default);
                now.saturating_sub(w.last_heartbeat_epoch_s) <= threshold
            })
            .collect();

        let (value, contrib, status): (String, i8, &'static str) = if workers.is_empty() {
            ("no workers".to_string(), 0, "warn")
        } else if alive.len() == workers.len() {
            (format!("{}/{} alive", alive.len(), workers.len()), 15, "healthy")
        } else if alive.is_empty() {
            (format!("0/{} alive", workers.len()), -15, "critical")
        } else {
            (format!("{}/{} alive", alive.len(), workers.len()), 0, "warn")
        };
        total += contrib as i32;
        signals.push(VitalSignal {
            name: "worker_health".to_string(),
            value,
            contribution: contrib,
            status,
        });
    }

    // goal_momentum
    {
        let doing = ctx.state.goals.iter().any(|g| g.status == "doing");
        let pending = ctx.state.goals.iter().any(|g| g.status == "pending");
        let (value, contrib, status): (&str, i8, &'static str) = if doing {
            ("doing", 15, "healthy")
        } else if pending {
            ("pending", 5, "warn")
        } else {
            ("no goals", 0, "warn")
        };
        total += contrib as i32;
        signals.push(VitalSignal {
            name: "goal_momentum".to_string(),
            value: value.to_string(),
            contribution: contrib,
            status,
        });
    }

    // policy_posture
    {
        let held_goals = ctx
            .state
            .goals
            .iter()
            .filter(|g| g.hold_reason.as_deref() == Some("policy_hold"))
            .count();
        let (value, contrib, status): (&str, i8, &'static str) = match &ctx.state.policy.enforcement {
            PolicyEnforcement::Off => ("off", 10, "healthy"),
            PolicyEnforcement::Observe => ("observe", 10, "healthy"),
            PolicyEnforcement::Enforce => {
                if held_goals > 0 {
                    ("enforce+held", -10, "critical")
                } else {
                    ("enforce", 10, "healthy")
                }
            }
        };
        total += contrib as i32;
        signals.push(VitalSignal {
            name: "policy_posture".to_string(),
            value: value.to_string(),
            contribution: contrib,
            status,
        });
    }

    // last_shutdown
    {
        let (value, contrib, status): (&str, i8, &'static str) = if ctx.state.last_clean_shutdown {
            ("clean", 10, "healthy")
        } else {
            ("unclean", -10, "critical")
        };
        total += contrib as i32;
        signals.push(VitalSignal {
            name: "last_shutdown".to_string(),
            value: value.to_string(),
            contribution: contrib,
            status,
        });
    }

    // continuity
    {
        let epoch = ctx.state.continuity_epoch;
        let (value, contrib, status): (String, i8, &'static str) = if epoch == 0 {
            (format!("epoch={}", epoch), 10, "healthy")
        } else if epoch <= 3 {
            (format!("epoch={}", epoch), 5, "warn")
        } else {
            (format!("epoch={}", epoch), -5, "critical")
        };
        total += contrib as i32;
        signals.push(VitalSignal {
            name: "continuity".to_string(),
            value,
            contribution: contrib,
            status,
        });
    }

    // federation
    {
        let peers = &ctx.state.federation;
        let stale_threshold = 86400u64;
        let active_count = peers
            .iter()
            .filter(|p| now.saturating_sub(p.last_seen_epoch_s) <= stale_threshold)
            .count();

        let (value, contrib, status): (String, i8, &'static str) = if peers.is_empty() {
            ("no peers".to_string(), 0, "warn")
        } else if active_count == 0 {
            (format!("0/{} active", peers.len()), -5, "critical")
        } else {
            (format!("{}/{} active", active_count, peers.len()), 10, "healthy")
        };
        total += contrib as i32;
        signals.push(VitalSignal {
            name: "federation".to_string(),
            value,
            contribution: contrib,
            status,
        });
    }

    let score = total.clamp(0, 100) as u8;
    let pulse = if score >= 70 {
        "strong"
    } else if score >= 40 {
        "weak"
    } else {
        "critical"
    };

    OrganismVitals { score, pulse, signals }
}

pub fn print_vitals(ctx: &RuntimeCtx, format: OutputFormat) {
    let now = now_epoch_s();
    let vitals = compute_vitals(ctx, now);

    if format.is_markdown() {
        println!("# organism vitals\n");
        println!("score: {}/100 [{}]\n", vitals.score, vitals.pulse);
        println!("| signal | value | contribution | status |");
        println!("|--------|-------|--------------|--------|");
        for sig in &vitals.signals {
            let contrib_str = if sig.contribution >= 0 {
                format!("+{}", sig.contribution)
            } else {
                format!("{}", sig.contribution)
            };
            println!("| {} | {} | {} | {} |", sig.name, sig.value, contrib_str, sig.status);
        }
    } else {
        println!("=== organism vitals ===");
        println!("score  : {}/100  [{}]", vitals.score, vitals.pulse);
        println!();
        println!("{:<20} {:<15} {:<14} {}", "signal", "value", "contribution", "status");
        for sig in &vitals.signals {
            let contrib_str = if sig.contribution >= 0 {
                format!("+{}", sig.contribution)
            } else {
                format!("{}", sig.contribution)
            };
            println!("{:<20} {:<15} {:<14} {}", sig.name, sig.value, contrib_str, sig.status);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        AppPaths, FederationPeer, Goal, Identity, PolicyEnforcement, PolicyState,
        RuntimeCtx, RuntimeMode, State, WorkerState,
    };
    use std::path::PathBuf;

    fn make_ctx(
        mode: RuntimeMode,
        epoch: u64,
        clean_shutdown: bool,
        goals: Vec<Goal>,
        workers: Vec<WorkerState>,
        peers: Vec<FederationPeer>,
        enforcement: PolicyEnforcement,
    ) -> RuntimeCtx {
        RuntimeCtx {
            paths: AppPaths::new(PathBuf::from("/tmp/vitals-test")),
            identity: Identity {
                organism_id: "org-1".to_string(),
                genesis_id: "gen-1".to_string(),
                runtime_habitat: "host_test".to_string(),
                created_at_epoch_s: 0,
            },
            state: State {
                boot_or_start_count: 1,
                continuity_epoch: epoch,
                last_clean_shutdown: clean_shutdown,
                last_recovery_reason: None,
                last_started_at_epoch_s: 0,
                mode,
                policy: PolicyState {
                    safe_first: true,
                    deny_by_default: true,
                    llm_advisory_only: true,
                    enforcement,
                },
                workers,
                goals,
                federation: peers,
            },
            runtime_instance_id: "r1".to_string(),
        }
    }

    fn make_goal(status: &str, hold_reason: Option<&str>) -> Goal {
        Goal {
            goal_id: uuid::Uuid::new_v4().to_string(),
            title: "test goal".to_string(),
            status: status.to_string(),
            hold_reason: hold_reason.map(|s| s.to_string()),
            notes: Vec::new(),
            tags: Vec::new(),
            priority: 0,
            created_at_epoch_s: 0,
            updated_at_epoch_s: 0,
            origin: "operator".to_string(),
            safety_class: "normal".to_string(),
            delegated_to: None,
        }
    }

    fn make_worker(last_beat: u64) -> WorkerState {
        WorkerState {
            worker_id: "w1".to_string(),
            role: "test".to_string(),
            status: "alive".to_string(),
            last_heartbeat_epoch_s: last_beat,
            heartbeat_count: 1,
            stale_after_s: Some(300),
        }
    }

    fn make_peer(last_seen: u64) -> FederationPeer {
        FederationPeer {
            peer_id: "p1".to_string(),
            habitat: "host_test".to_string(),
            label: None,
            last_seen_epoch_s: last_seen,
            last_export_path: None,
            status: "active".to_string(),
        }
    }

    #[test]
    fn compute_vitals_baseline_normal_no_workers_no_goals() {
        // normal(+20) + no_workers(+0) + no_goals(+0) + observe(+10) + clean(+10) + epoch=0(+10) + no_peers(+0) = 50
        let ctx = make_ctx(
            RuntimeMode::Normal, 0, true,
            Vec::new(), Vec::new(), Vec::new(),
            PolicyEnforcement::Observe,
        );
        let vitals = compute_vitals(&ctx, 1000);
        assert_eq!(vitals.score, 50);
        assert_eq!(vitals.pulse, "weak");
    }

    #[test]
    fn compute_vitals_degraded_unclean_scores_zero() {
        // degraded(-20) + no_workers(+0) + no_goals(+0) + enforce(+10) + unclean(-10) + epoch>3(-5) + no_peers(+0) = -25 → 0
        let ctx = make_ctx(
            RuntimeMode::Degraded, 5, false,
            Vec::new(), Vec::new(), Vec::new(),
            PolicyEnforcement::Enforce,
        );
        let vitals = compute_vitals(&ctx, 1000);
        assert_eq!(vitals.score, 0);
        assert_eq!(vitals.pulse, "critical");
    }

    #[test]
    fn compute_vitals_strong_pulse_all_healthy() {
        // normal(+20) + 1/1 alive(+15) + doing(+15) + observe(+10) + clean(+10) + epoch=0(+10) + 1/1 active(+10) = 90
        let now = 1000u64;
        let ctx = make_ctx(
            RuntimeMode::Normal, 0, true,
            vec![make_goal("doing", None)],
            vec![make_worker(now - 10)],
            vec![make_peer(now - 100)],
            PolicyEnforcement::Observe,
        );
        let vitals = compute_vitals(&ctx, now);
        assert_eq!(vitals.score, 90);
        assert_eq!(vitals.pulse, "strong");
    }

    #[test]
    fn compute_vitals_stale_workers_score_correctly() {
        // Stale workers: alive_count=0, -15
        let now = 1000u64;
        let mut worker = make_worker(now - 1000); // way past 300s stale threshold
        worker.stale_after_s = Some(300);
        let ctx = make_ctx(
            RuntimeMode::Normal, 0, true,
            Vec::new(), vec![worker], Vec::new(),
            PolicyEnforcement::Observe,
        );
        let vitals = compute_vitals(&ctx, now);
        let worker_sig = vitals.signals.iter().find(|s| s.name == "worker_health").unwrap();
        assert_eq!(worker_sig.contribution, -15);
        assert_eq!(worker_sig.status, "critical");
    }

    #[test]
    fn compute_vitals_enforce_with_held_goals_penalised() {
        let ctx = make_ctx(
            RuntimeMode::Normal, 0, true,
            vec![make_goal("blocked", Some("policy_hold"))],
            Vec::new(), Vec::new(),
            PolicyEnforcement::Enforce,
        );
        let vitals = compute_vitals(&ctx, 1000);
        let pol_sig = vitals.signals.iter().find(|s| s.name == "policy_posture").unwrap();
        assert_eq!(pol_sig.contribution, -10);
        assert_eq!(pol_sig.status, "critical");
    }

    #[test]
    fn vitals_signal_names_all_present() {
        let ctx = make_ctx(
            RuntimeMode::Normal, 0, true,
            Vec::new(), Vec::new(), Vec::new(),
            PolicyEnforcement::Observe,
        );
        let vitals = compute_vitals(&ctx, 1000);
        let names: Vec<&str> = vitals.signals.iter().map(|s| s.name.as_str()).collect();
        for expected in &["runtime_mode", "worker_health", "goal_momentum", "policy_posture", "last_shutdown", "continuity", "federation"] {
            assert!(names.contains(expected), "missing signal: {expected}");
        }
    }

    #[test]
    fn compute_vitals_pending_goal_contributes_5() {
        let ctx = make_ctx(
            RuntimeMode::Normal, 0, true,
            vec![make_goal("pending", None)],
            Vec::new(), Vec::new(),
            PolicyEnforcement::Observe,
        );
        let vitals = compute_vitals(&ctx, 1000);
        let momentum = vitals.signals.iter().find(|s| s.name == "goal_momentum").unwrap();
        assert_eq!(momentum.contribution, 5);
        assert_eq!(momentum.status, "warn");
    }
}
