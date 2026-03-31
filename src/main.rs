mod dream;
mod export;
mod federation;
mod goals;
mod io;
mod journal;
mod memory;
mod narrate;
mod policy;
mod reports;
mod scheduler;
mod serve;
mod signing;
mod state;
mod training;
mod types;
mod vitals;
mod workers;

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use uuid::Uuid;

use export::export_sovereign;
use dream::run_dream;
use federation::{import_peer_export, list_peers, register_peer};
use goals::{
    abort_goal, add_goal, add_goal_note, delegate_goal, delete_goal, hold_goal, inspect_goal,
    list_goals, mark_goal_done, print_next_goal, recall_goal, resume_goal, start_goal, tag_goal,
    untag_goal,
};
use io::{append_event, compute_fingerprint, now_epoch_s};
use journal::{explain_journal, rotate_journal, search_journal, tail_journal};
use memory::{consolidate_journal, list_memories};
use narrate::narrate;
use policy::{print_mode, print_policy, set_mode, set_policy_enforcement};
use reports::{print_status, write_daily_reports};
use scheduler::scheduler_tick;
use serve::run_server;
use signing::{sign_event, verify_event};
use state::{bootstrap, recover_state, save_recovery_snapshot, save_state};
use types::{JournalEvent, OutputFormat, PolicyEnforcement, RuntimeMode};
use training::{export_training, ingest_sovereign_training, locate_sovereign_train, print_training_summary};
use vitals::print_vitals;
use workers::{beat_worker, list_workers, run_worker_watchdog};

#[derive(Parser, Debug)]
#[command(name = "oo-host")]
#[command(about = "Minimal OO host runtime v0")]
struct Cli {
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Status(StatusCommand),
    Goal(GoalCommand),
    Goals(GoalsCommand),
    Journal(JournalCommand),
    Report(ReportCommand),
    Mode(ModeCommand),
    Policy(PolicyCommand),
    Worker(WorkerCommand),
    Tick,
    Recover,
    Export(ExportCommand),
    Federation(FederationCommand),
    Serve(ServeCommand),
    Dream(DreamCommand),
    Vitals(VitalsCommand),
    Fingerprint(FingerprintCommand),
    Memory(MemoryCommand),
    Narrate(NarrateCommand),
    Training(TrainingCommand),
}

#[derive(Args, Debug)]
struct StatusCommand {
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct GoalCommand {
    #[command(subcommand)]
    command: GoalSubcommand,
}

#[derive(Subcommand, Debug)]
enum GoalSubcommand {
    Add {
        title: String,
        #[arg(long, default_value_t = 0)]
        priority: i32,
        #[arg(long, default_value = "operator")]
        origin: String,
        #[arg(long, default_value = "normal")]
        safety: String,
    },
    Done {
        goal_id: String,
    },
    Start {
        goal_id: String,
    },
    Hold {
        goal_id: String,
        #[arg(long, default_value = "operator_hold")]
        reason: String,
    },
    Note {
        goal_id: String,
        text: String,
        #[arg(long, default_value = "operator")]
        author: String,
    },
    Abort {
        goal_id: String,
        #[arg(long, default_value = "operator_abort")]
        reason: String,
    },
    Resume {
        goal_id: String,
    },
    Delete {
        goal_id: String,
    },
    Tag {
        goal_id: String,
        tag: String,
    },
    Untag {
        goal_id: String,
        tag: String,
    },
    Delegate {
        goal_id: String,
        #[arg(long)]
        peer: String,
    },
    Recall {
        goal_id: String,
    },
}

#[derive(Args, Debug)]
struct GoalsCommand {
    #[command(subcommand)]
    command: GoalsSubcommand,
}

#[derive(Subcommand, Debug)]
enum GoalsSubcommand {
    List,
    Next,
    Inspect {
        goal_id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Args, Debug)]
struct JournalCommand {
    #[command(subcommand)]
    command: JournalSubcommand,
}

#[derive(Args, Debug)]
struct ReportCommand {
    #[command(subcommand)]
    command: ReportSubcommand,
}

#[derive(Subcommand, Debug)]
enum JournalSubcommand {
    Tail {
        #[arg(short = 'n', long, default_value_t = 20)]
        count: usize,
    },
    Explain {
        #[arg(short = 'n', long, default_value_t = 20)]
        count: usize,
    },
    Search {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        severity: Option<String>,
        #[arg(long)]
        since: Option<u64>,
        #[arg(long)]
        until: Option<u64>,
        #[arg(short = 'n', long, default_value_t = 50)]
        count: usize,
    },
    Rotate {
        #[arg(long, default_value_t = 10000)]
        max_lines: usize,
        #[arg(long, default_value_t = 2000)]
        keep: usize,
    },
    Sign {
        #[arg(short = 'n', long, default_value_t = 20)]
        count: usize,
        #[arg(long)]
        key: PathBuf,
    },
    Verify {
        #[arg(short = 'n', long, default_value_t = 20)]
        count: usize,
        #[arg(long)]
        key: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum ReportSubcommand {
    Daily {
        #[arg(long)]
        out_dir: Option<PathBuf>,
        #[arg(short = 'n', long, default_value_t = 20)]
        journal_count: usize,
        #[arg(long)]
        include_sovereign: bool,
        #[arg(long)]
        include_sync: bool,
        #[arg(long)]
        sovereign_workspace: Option<PathBuf>,
    },
}

#[derive(Args, Debug)]
struct ModeCommand {
    #[command(subcommand)]
    command: ModeSubcommand,
}

#[derive(Subcommand, Debug)]
enum ModeSubcommand {
    Show,
    Set {
        mode: RuntimeMode,
    },
}

#[derive(Args, Debug)]
struct PolicyCommand {
    #[command(subcommand)]
    command: PolicySubcommand,
}

#[derive(Args, Debug)]
struct WorkerCommand {
    #[command(subcommand)]
    command: WorkerSubcommand,
}

#[derive(Subcommand, Debug)]
enum WorkerSubcommand {
    List,
    Beat {
        worker_id: String,
        #[arg(long, default_value = "generic")]
        role: String,
        #[arg(long, default_value = "heartbeat")]
        summary: String,
        #[arg(long)]
        stale_after: Option<u64>,
    },
    Watchdog {
        #[arg(long, default_value_t = 3)]
        cycles: u32,
        #[arg(long, default_value_t = 0)]
        interval_ms: u64,
    },
}

#[derive(Subcommand, Debug)]
enum PolicySubcommand {
    Show,
    Set {
        enforcement: PolicyEnforcement,
    },
}

#[derive(Args, Debug)]
struct ExportCommand {
    #[command(subcommand)]
    command: ExportSubcommand,
}

#[derive(Subcommand, Debug)]
enum ExportSubcommand {
    Sovereign {
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Args, Debug)]
struct FederationCommand {
    #[command(subcommand)]
    command: FederationSubcommand,
}

#[derive(Subcommand, Debug)]
enum FederationSubcommand {
    List,
    Register {
        peer_id: String,
        #[arg(long)]
        habitat: String,
        #[arg(long)]
        label: Option<String>,
    },
    Import {
        export_path: PathBuf,
    },
}

#[derive(Args, Debug)]
struct ServeCommand {
    #[arg(long, default_value_t = 8080)]
    port: u16,
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
}

#[derive(Args, Debug)]
struct DreamCommand {
    #[arg(long, default_value_t = 50)]
    depth: usize,
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct VitalsCommand {
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Debug)]
struct FingerprintCommand {
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Debug)]
struct MemoryCommand {
    #[command(subcommand)]
    command: MemorySubcommand,
}

#[derive(Subcommand, Debug)]
enum MemorySubcommand {
    Consolidate {
        #[arg(long, default_value_t = 3600)]
        window_s: u64,
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },
    List,
}

#[derive(Args, Debug)]
struct NarrateCommand {
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct TrainingCommand {
    #[command(subcommand)]
    command: TrainingSubcommand,
}

#[derive(Subcommand, Debug)]
enum TrainingSubcommand {
    /// Show stats about the firmware's OO_TRAIN.JSONL artifact.
    Summary {
        /// Path to OO_TRAIN.JSONL (firmware EFI volume or mounted path).
        #[arg(long)]
        sovereign: Option<PathBuf>,
    },
    /// Export high-quality samples to a clean JSONL file for SFT.
    Export {
        #[arg(long)]
        sovereign: Option<PathBuf>,
        /// Minimum quality score (0-10) to include.
        #[arg(long, default_value_t = 6)]
        min_quality: u8,
        /// Output path for the exported JSONL.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Ingest new firmware samples into the host training dataset.
    Ingest {
        #[arg(long)]
        sovereign: Option<PathBuf>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mut ctx = bootstrap(cli.data_dir)?;

    match cli.command {
        Command::Status(status) => print_status(&ctx, status.format, status.out.as_deref())?,
        Command::Goal(goal) => match goal.command {
            GoalSubcommand::Add {
                title,
                priority,
                origin,
                safety,
            } => {
                add_goal(&mut ctx, title, priority, origin, safety)?;
                println!("OK: goal added");
            }
            GoalSubcommand::Done { goal_id } => {
                mark_goal_done(&mut ctx, &goal_id)?;
                println!("OK: goal done");
            }
            GoalSubcommand::Start { goal_id } => {
                start_goal(&mut ctx, &goal_id)?;
                println!("OK: goal started");
            }
            GoalSubcommand::Hold { goal_id, reason } => {
                hold_goal(&mut ctx, &goal_id, &reason)?;
                println!("OK: goal held");
            }
            GoalSubcommand::Note {
                goal_id,
                text,
                author,
            } => {
                add_goal_note(&mut ctx, &goal_id, &text, &author)?;
                println!("OK: goal note added");
            }
            GoalSubcommand::Abort { goal_id, reason } => {
                abort_goal(&mut ctx, &goal_id, &reason)?;
                println!("OK: goal aborted");
            }
            GoalSubcommand::Resume { goal_id } => {
                resume_goal(&mut ctx, &goal_id)?;
                println!("OK: goal resumed");
            }
            GoalSubcommand::Delete { goal_id } => {
                delete_goal(&mut ctx, &goal_id)?;
                println!("OK: goal deleted");
            }
            GoalSubcommand::Tag { goal_id, tag } => {
                tag_goal(&mut ctx, &goal_id, &tag)?;
                println!("OK: goal tag added");
            }
            GoalSubcommand::Untag { goal_id, tag } => {
                untag_goal(&mut ctx, &goal_id, &tag)?;
                println!("OK: goal tag removed");
            }
            GoalSubcommand::Delegate { goal_id, peer } => {
                delegate_goal(&mut ctx, &goal_id, &peer)?;
                println!("OK: goal delegated to {peer}");
            }
            GoalSubcommand::Recall { goal_id } => {
                recall_goal(&mut ctx, &goal_id)?;
                println!("OK: goal recalled");
            }
        },
        Command::Goals(goals) => match goals.command {
            GoalsSubcommand::List => list_goals(&ctx),
            GoalsSubcommand::Next => print_next_goal(&ctx),
            GoalsSubcommand::Inspect {
                goal_id,
                format,
                out,
            } => inspect_goal(&ctx, &goal_id, format, out.as_deref())?,
        },
        Command::Journal(journal) => match journal.command {
            JournalSubcommand::Tail { count } => tail_journal(&ctx.paths.journal_path, count)?,
            JournalSubcommand::Explain { count } => explain_journal(&ctx.paths.journal_path, count)?,
            JournalSubcommand::Search { kind, severity, since, until, count } => {
                search_journal(
                    &ctx.paths.journal_path,
                    kind.as_deref(),
                    severity.as_deref(),
                    since,
                    until,
                    count,
                )?;
            }
            JournalSubcommand::Rotate { max_lines, keep } => {
                rotate_journal(&ctx, max_lines, keep)?;
            }
            JournalSubcommand::Sign { count, key } => {
                let events = io::read_recent_events(&ctx.paths.journal_path, count)?;
                let unsigned: Vec<_> = events.into_iter().filter(|e| e.signature.is_none()).collect();
                let n = unsigned.len();
                // Rewrite entire journal: re-read all, replace unsigned ones with signed versions
                let all_events = io::read_all_events(&ctx.paths.journal_path)?;
                let unsigned_ids: std::collections::HashSet<String> =
                    unsigned.iter().map(|e| e.event_id.clone()).collect();
                let mut signed_events: Vec<types::JournalEvent> = Vec::new();
                for mut event in all_events {
                    if unsigned_ids.contains(&event.event_id) {
                        sign_event(&mut event, &key)?;
                    }
                    signed_events.push(event);
                }
                {
                    use std::io::Write as _;
                    let tmp = ctx.paths.journal_path.with_extension("tmp");
                    let mut f = std::fs::File::create(&tmp)?;
                    for e in &signed_events {
                        serde_json::to_writer(&mut f, e)?;
                        f.write_all(b"\n")?;
                    }
                    std::fs::rename(tmp, &ctx.paths.journal_path)?;
                }
                println!("OK: {n} events signed");
            }
            JournalSubcommand::Verify { count, key } => {
                let events = io::read_recent_events(&ctx.paths.journal_path, count)?;
                for event in &events {
                    let result = if event.signature.is_none() {
                        "unsigned".to_string()
                    } else {
                        match verify_event(event, &key) {
                            Ok(true) => "OK".to_string(),
                            Ok(false) => "INVALID".to_string(),
                            Err(e) => format!("ERROR: {e}"),
                        }
                    };
                    println!("[{}] {} | {}", event.ts_epoch_s, event.kind, result);
                }
            }
        },
        Command::Report(report) => match report.command {
            ReportSubcommand::Daily {
                out_dir,
                journal_count,
                include_sovereign,
                include_sync,
                sovereign_workspace,
            } => {
                let target_dir = out_dir.unwrap_or_else(|| ctx.paths.data_dir.join("reports").join("daily"));
                write_daily_reports(
                    &ctx,
                    &target_dir,
                    journal_count,
                    include_sovereign,
                    include_sync,
                    sovereign_workspace.as_deref(),
                )?;
                println!("OK: daily reports written to {}", target_dir.display());
            }
        },
        Command::Mode(mode) => match mode.command {
            ModeSubcommand::Show => print_mode(&ctx),
            ModeSubcommand::Set { mode } => {
                set_mode(&mut ctx, mode)?;
                println!("OK: mode updated");
            }
        },
        Command::Policy(policy) => match policy.command {
            PolicySubcommand::Show => print_policy(&ctx),
            PolicySubcommand::Set { enforcement } => {
                set_policy_enforcement(&mut ctx, enforcement)?;
                println!("OK: policy updated");
            }
        },
        Command::Worker(worker) => match worker.command {
            WorkerSubcommand::List => list_workers(&ctx),
            WorkerSubcommand::Beat {
                worker_id,
                role,
                summary,
                stale_after,
            } => {
                beat_worker(&mut ctx, &worker_id, &role, &summary, stale_after)?;
                println!("OK: worker heartbeat recorded");
            }
            WorkerSubcommand::Watchdog {
                cycles,
                interval_ms,
            } => run_worker_watchdog(&mut ctx, cycles, interval_ms)?,
        },
        Command::Tick => {
            let result = scheduler_tick(&mut ctx)?;
            println!("tick_result       : {}", result);
        }
        Command::Recover => {
            recover_state(&mut ctx)?;
            println!("OK: recovery snapshot restored");
        }
        Command::Export(export) => match export.command {
            ExportSubcommand::Sovereign { out } => {
                let out_path = out.unwrap_or_else(|| ctx.paths.data_dir.join("sovereign_export.json"));
                export_sovereign(&ctx, &out_path)?;
                println!("OK: sovereign export written to {}", out_path.display());
            }
        },
        Command::Federation(fed) => match fed.command {
            FederationSubcommand::List => list_peers(&ctx),
            FederationSubcommand::Register { peer_id, habitat, label } => {
                register_peer(&mut ctx, &peer_id, &habitat, label.as_deref())?;
                println!("OK: peer {peer_id} registered");
            }
            FederationSubcommand::Import { export_path } => {
                import_peer_export(&mut ctx, &export_path)?;
            }
        },
        Command::Serve(serve) => {
            // Save state before entering server loop (clean shutdown already set to true)
            ctx.state.last_clean_shutdown = true;
            save_state(&ctx.paths.state_path, &ctx.state)?;
            save_recovery_snapshot(&ctx.paths.recovery_path, &ctx.state)?;
            run_server(ctx, &serve.bind, serve.port)?;
            return Ok(());
        }
        Command::Dream(dream) => {
            run_dream(&ctx, dream.depth, dream.out.as_deref())?;
        }
        Command::Vitals(vitals) => {
            print_vitals(&ctx, vitals.format);
        }
        Command::Fingerprint(fp) => {
            let result = compute_fingerprint(&ctx.paths.journal_path)?;
            if fp.format.is_markdown() {
                println!("# continuity fingerprint\n");
                println!("organism_id    : {}", ctx.identity.organism_id);
                println!("fingerprint    : {}", result.fingerprint);
                println!("event_count    : {}", result.event_count);
                println!("first_event    : {}", result.first_event_ts.unwrap_or(0));
                println!("last_event     : {}", result.last_event_ts.unwrap_or(0));
                println!("span_s         : {}", result.span_s);
            } else {
                println!("=== continuity fingerprint ===");
                println!("organism_id    : {}", ctx.identity.organism_id);
                println!("fingerprint    : {}", result.fingerprint);
                println!("event_count    : {}", result.event_count);
                println!("first_event    : {}", result.first_event_ts.unwrap_or(0));
                println!("last_event     : {}", result.last_event_ts.unwrap_or(0));
                println!("span_s         : {}", result.span_s);
            }
        }
        Command::Memory(mem) => match mem.command {
            MemorySubcommand::Consolidate { window_s, out_dir } => {
                let target = out_dir.unwrap_or_else(|| ctx.paths.data_dir.clone());
                let n = consolidate_journal(&ctx, window_s, &target)?;
                println!("OK: consolidated {n} memories");
            }
            MemorySubcommand::List => {
                let memories_path = ctx.paths.data_dir.join("organism_memories.jsonl");
                list_memories(&memories_path)?;
            }
        },
        Command::Narrate(nar) => {
            narrate(&ctx, nar.format, nar.out.as_deref())?;
        }
        Command::Training(tr) => match tr.command {
            TrainingSubcommand::Summary { sovereign } => {
                let base = sovereign
                    .unwrap_or_else(|| ctx.paths.data_dir.join("sovereign"));
                let src = locate_sovereign_train(&base)
                    .or_else(|| {
                        // Fallback: directly use the data_dir for local testing
                        let p = base.with_extension("jsonl");
                        if p.exists() { Some(p) } else { None }
                    })
                    .ok_or("OO_TRAIN.JSONL not found — use --sovereign <efi_vol_path>")?;
                print_training_summary(&src)?;
            }
            TrainingSubcommand::Export { sovereign, min_quality, out } => {
                let base = sovereign
                    .unwrap_or_else(|| ctx.paths.data_dir.join("sovereign"));
                let src = locate_sovereign_train(&base)
                    .ok_or("OO_TRAIN.JSONL not found")?;
                let out_path = out.unwrap_or_else(|| {
                    ctx.paths.data_dir.join("oo_train_export.jsonl")
                });
                let n = export_training(&src, &out_path, min_quality)?;
                println!("OK: exported {n} samples (quality >= {min_quality}) to {}", out_path.display());
            }
            TrainingSubcommand::Ingest { sovereign } => {
                let base = sovereign
                    .unwrap_or_else(|| ctx.paths.data_dir.join("sovereign"));
                let src = locate_sovereign_train(&base)
                    .ok_or("OO_TRAIN.JSONL not found")?;
                let n = ingest_sovereign_training(&src, &ctx.paths.data_dir)?;
                if n > 0 {
                    println!("OK: ingested {n} new training samples from sovereign");
                } else {
                    println!("OK: no new samples (already up to date)");
                }
            }
        },
    }

    ctx.state.last_clean_shutdown = true;
    save_state(&ctx.paths.state_path, &ctx.state)?;
    save_recovery_snapshot(&ctx.paths.recovery_path, &ctx.state)?;
    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now_epoch_s(),
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "shutdown".to_string(),
            severity: "info".to_string(),
            summary: "host runtime command complete".to_string(),
            reason: None,
            action: None,
            result: Some("clean_shutdown".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
            signature: None,
        },
    )?;

    Ok(())
}

