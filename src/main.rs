mod export;
mod goals;
mod io;
mod journal;
mod policy;
mod reports;
mod scheduler;
mod state;
mod types;
mod workers;

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use uuid::Uuid;

use export::export_sovereign;
use goals::{
    abort_goal, add_goal, add_goal_note, hold_goal, inspect_goal, list_goals, mark_goal_done,
    print_next_goal, resume_goal, start_goal,
};
use io::{append_event, now_epoch_s};
use journal::{explain_journal, tail_journal};
use policy::{print_mode, print_policy, set_mode, set_policy_enforcement};
use reports::{print_status, write_daily_reports};
use scheduler::scheduler_tick;
use state::{bootstrap, recover_state, save_recovery_snapshot, save_state};
use types::{JournalEvent, OutputFormat, PolicyEnforcement, RuntimeMode};
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
            } => {
                beat_worker(&mut ctx, &worker_id, &role, &summary)?;
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
        },
    )?;

    Ok(())
}

