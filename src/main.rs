use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

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

#[derive(Debug, Clone)]
struct AppPaths {
    data_dir: PathBuf,
    identity_path: PathBuf,
    state_path: PathBuf,
    journal_path: PathBuf,
    recovery_path: PathBuf,
}

impl AppPaths {
    fn new(root: PathBuf) -> Self {
        Self {
            identity_path: root.join("organism_identity.json"),
            state_path: root.join("organism_state.json"),
            journal_path: root.join("organism_journal.jsonl"),
            recovery_path: root.join("organism_recovery.json"),
            data_dir: root,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Identity {
    organism_id: String,
    genesis_id: String,
    runtime_habitat: String,
    created_at_epoch_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum RuntimeMode {
    Normal,
    Degraded,
    Safe,
}

impl RuntimeMode {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Degraded => "degraded",
            Self::Safe => "safe",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum PolicyEnforcement {
    Off,
    Observe,
    Enforce,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum OutputFormat {
    Text,
    Markdown,
}

impl PolicyEnforcement {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Observe => "observe",
            Self::Enforce => "enforce",
        }
    }
}

impl OutputFormat {
    fn is_markdown(self) -> bool {
        matches!(self, Self::Markdown)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PolicyState {
    safe_first: bool,
    deny_by_default: bool,
    llm_advisory_only: bool,
    enforcement: PolicyEnforcement,
}

#[derive(Debug, Serialize, Deserialize)]
struct State {
    boot_or_start_count: u64,
    continuity_epoch: u64,
    last_clean_shutdown: bool,
    last_recovery_reason: Option<String>,
    last_started_at_epoch_s: u64,
    #[serde(default = "default_runtime_mode")]
    mode: RuntimeMode,
    #[serde(default = "default_policy_state")]
    policy: PolicyState,
    #[serde(default)]
    workers: Vec<WorkerState>,
    goals: Vec<Goal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkerState {
    worker_id: String,
    role: String,
    status: String,
    last_heartbeat_epoch_s: u64,
    heartbeat_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Goal {
    goal_id: String,
    title: String,
    status: String,
    #[serde(default)]
    hold_reason: Option<String>,
    #[serde(default)]
    notes: Vec<GoalNote>,
    priority: i32,
    created_at_epoch_s: u64,
    updated_at_epoch_s: u64,
    origin: String,
    safety_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoalNote {
    ts_epoch_s: u64,
    author: String,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct JournalEvent {
    event_id: String,
    ts_epoch_s: u64,
    organism_id: String,
    runtime_habitat: String,
    runtime_instance_id: String,
    kind: String,
    severity: String,
    summary: String,
    reason: Option<String>,
    action: Option<String>,
    result: Option<String>,
    continuity_epoch: u64,
}

#[derive(Debug, Serialize)]
struct SovereignExport<'a> {
    schema_version: u32,
    export_kind: &'static str,
    generated_at_epoch_s: u64,
    organism_id: &'a str,
    genesis_id: &'a str,
    runtime_habitat: &'a str,
    runtime_instance_id: &'a str,
    continuity_epoch: u64,
    boot_or_start_count: u64,
    mode: &'a str,
    last_recovery_reason: Option<&'a str>,
    policy: SovereignPolicyExport<'a>,
    active_goal_count: usize,
    top_goals: Vec<SovereignGoalExport<'a>>,
    recent_events: Vec<SovereignEventExport>,
}

#[derive(Debug, Serialize)]
struct SovereignPolicyExport<'a> {
    safe_first: bool,
    deny_by_default: bool,
    llm_advisory_only: bool,
    enforcement: &'a str,
}

#[derive(Debug, Serialize)]
struct SovereignGoalExport<'a> {
    goal_id: &'a str,
    title: &'a str,
    status: &'a str,
    priority: i32,
    safety_class: &'a str,
}

#[derive(Debug, Serialize)]
struct SovereignEventExport {
    ts_epoch_s: u64,
    kind: String,
    severity: String,
    summary: String,
    reason: Option<String>,
    action: Option<String>,
    result: Option<String>,
    continuity_epoch: u64,
}

struct RuntimeCtx {
    paths: AppPaths,
    identity: Identity,
    state: State,
    runtime_instance_id: String,
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
            } => {
                let target_dir = out_dir.unwrap_or_else(|| ctx.paths.data_dir.join("reports").join("daily"));
                write_daily_reports(&ctx, &target_dir, journal_count)?;
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

fn bootstrap(data_dir: PathBuf) -> Result<RuntimeCtx, Box<dyn std::error::Error>> {
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

fn load_or_create_identity(path: &Path) -> Result<Identity, Box<dyn std::error::Error>> {
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

fn load_or_create_state(path: &Path) -> Result<State, Box<dyn std::error::Error>> {
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

fn default_runtime_mode() -> RuntimeMode {
    RuntimeMode::Normal
}

fn default_policy_state() -> PolicyState {
    PolicyState {
        safe_first: true,
        deny_by_default: true,
        llm_advisory_only: true,
        enforcement: PolicyEnforcement::Observe,
    }
}

fn add_goal(
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

fn mark_goal_done(ctx: &mut RuntimeCtx, goal_id: &str) -> Result<(), Box<dyn std::error::Error>> {
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

fn hold_goal(
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

fn add_goal_note(
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

fn resume_goal(ctx: &mut RuntimeCtx, goal_id: &str) -> Result<(), Box<dyn std::error::Error>> {
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

fn start_goal(ctx: &mut RuntimeCtx, goal_id: &str) -> Result<(), Box<dyn std::error::Error>> {
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

fn abort_goal(
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

fn set_mode(ctx: &mut RuntimeCtx, mode: RuntimeMode) -> Result<(), Box<dyn std::error::Error>> {
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

fn set_policy_enforcement(
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

fn beat_worker(
    ctx: &mut RuntimeCtx,
    worker_id: &str,
    role: &str,
    summary: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    if let Some(worker) = ctx.state.workers.iter_mut().find(|w| w.worker_id == worker_id) {
        worker.role = role.to_string();
        worker.status = "alive".to_string();
        worker.last_heartbeat_epoch_s = now;
        worker.heartbeat_count += 1;
    } else {
        ctx.state.workers.push(WorkerState {
            worker_id: worker_id.to_string(),
            role: role.to_string(),
            status: "alive".to_string(),
            last_heartbeat_epoch_s: now,
            heartbeat_count: 1,
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
            kind: "worker_heartbeat".to_string(),
            severity: "info".to_string(),
            summary: format!("worker heartbeat: {worker_id} ({summary})"),
            reason: None,
            action: Some("worker_beat".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok(())
}

fn recover_state(ctx: &mut RuntimeCtx) -> Result<(), Box<dyn std::error::Error>> {
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

fn scheduler_tick(ctx: &mut RuntimeCtx) -> Result<&'static str, Box<dyn std::error::Error>> {
    let now = now_epoch_s();
    let policy_result = apply_policy_homeostasis(ctx, now)?;
    let _ = apply_worker_homeostasis(ctx, now)?;

    if policy_result == "policy_hold_active" {
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "scheduler_tick".to_string(),
                severity: "warn".to_string(),
                summary: "scheduler paused by policy hold".to_string(),
                reason: Some("policy_hold_active".to_string()),
                action: Some("tick_pause".to_string()),
                result: Some("policy_pause".to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            },
        )?;
        return Ok("policy_pause");
    }

    if matches!(ctx.state.mode, RuntimeMode::Degraded) {
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "scheduler_tick".to_string(),
                severity: "warn".to_string(),
                summary: "scheduler paused while runtime is degraded".to_string(),
                reason: Some("worker_health_degraded".to_string()),
                action: Some("tick_pause".to_string()),
                result: Some("degraded_pause".to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            },
        )?;
        return Ok("degraded_pause");
    }

    if let Some(goal) = ctx.state.goals.iter().find(|g| g.status == "doing") {
        let active_title = goal.title.clone();
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "scheduler_tick".to_string(),
                severity: "info".to_string(),
                summary: format!("scheduler kept active goal: {active_title}"),
                reason: Some("active_goal_present".to_string()),
                action: Some("tick_noop".to_string()),
                result: Some("active_goal_unchanged".to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            },
        )?;
        return Ok("active_goal_unchanged");
    }

    let next_goal = select_next_goal(&ctx.state)
        .filter(|g| g.status == "pending" || g.status == "recovering")
        .map(|g| (g.goal_id.clone(), g.status.clone(), g.title.clone()));

    if let Some((goal_id, prior_status, goal_title)) = next_goal {
        start_goal(ctx, &goal_id)?;
        let (summary, reason, action, result) = if prior_status == "recovering" {
            (
                format!("scheduler resumed goal: {goal_title}"),
                "worker_health_restored",
                "tick_resume_goal",
                "goal_resumed",
            )
        } else {
            (
                format!("scheduler activated goal: {goal_id}"),
                "selected_pending_goal",
                "tick_start_goal",
                "goal_started",
            )
        };
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "scheduler_tick".to_string(),
                severity: "notice".to_string(),
                summary,
                reason: Some(reason.to_string()),
                action: Some(action.to_string()),
                result: Some(result.to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            },
        )?;
        return Ok(result);
    }

    append_event(
        &ctx.paths.journal_path,
        &JournalEvent {
            event_id: Uuid::new_v4().to_string(),
            ts_epoch_s: now,
            organism_id: ctx.identity.organism_id.clone(),
            runtime_habitat: ctx.identity.runtime_habitat.clone(),
            runtime_instance_id: ctx.runtime_instance_id.clone(),
            kind: "scheduler_tick".to_string(),
            severity: "info".to_string(),
            summary: "scheduler found no pending goals".to_string(),
            reason: Some("no_pending_goals".to_string()),
            action: Some("tick_noop".to_string()),
            result: Some("idle".to_string()),
            continuity_epoch: ctx.state.continuity_epoch,
        },
    )?;

    Ok("idle")
}

fn export_sovereign(ctx: &RuntimeCtx, out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
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

fn read_recent_events(path: &Path, count: usize) -> Result<Vec<JournalEvent>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: JournalEvent = serde_json::from_str(&line)?;
        events.push(event);
    }
    let start = events.len().saturating_sub(count);
    Ok(events.into_iter().skip(start).collect())
}

fn read_all_events(path: &Path) -> Result<Vec<JournalEvent>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: JournalEvent = serde_json::from_str(&line)?;
        events.push(event);
    }
    Ok(events)
}

fn collect_goal_events(
    path: &Path,
    goal: &Goal,
) -> Result<Vec<JournalEvent>, Box<dyn std::error::Error>> {
    let events = read_all_events(path)?;
    Ok(events
        .into_iter()
        .filter(|event| event_mentions_goal(event, goal))
        .collect())
}

fn event_mentions_goal(event: &JournalEvent, goal: &Goal) -> bool {
    let summary = event.summary.as_str();
    let reason = event.reason.as_deref().unwrap_or("");
    let action = event.action.as_deref().unwrap_or("");

    summary.contains(&goal.goal_id)
        || summary.contains(&goal.title)
        || reason.contains(&goal.goal_id)
        || reason.contains(&goal.title)
        || action.contains(&goal.goal_id)
}

fn persist_ctx(ctx: &RuntimeCtx) -> Result<(), Box<dyn std::error::Error>> {
    save_state(&ctx.paths.state_path, &ctx.state)?;
    save_recovery_snapshot(&ctx.paths.recovery_path, &ctx.state)?;
    Ok(())
}

fn run_worker_watchdog(
    ctx: &mut RuntimeCtx,
    cycles: u32,
    interval_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let total_cycles = if cycles == 0 { 1 } else { cycles };
    for i in 0..total_cycles {
        let now = now_epoch_s();
        let result = apply_worker_homeostasis(ctx, now)?;
        let stale = count_stale_workers(&ctx.state, now);
        let alive = ctx.state.workers.len().saturating_sub(stale);
        println!(
            "watchdog.cycle={} result={} mode={} workers_alive={} workers_stale={}",
            i + 1,
            result,
            ctx.state.mode.as_str(),
            alive,
            stale
        );
        if interval_ms > 0 && i + 1 < total_cycles {
            thread::sleep(Duration::from_millis(interval_ms));
        }
    }
    Ok(())
}

fn print_status(
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

fn render_status_text(ctx: &RuntimeCtx) -> String {
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

fn render_status_markdown(ctx: &RuntimeCtx) -> String {
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

fn print_mode(ctx: &RuntimeCtx) {
    println!("mode={}", ctx.state.mode.as_str());
}

fn print_policy(ctx: &RuntimeCtx) {
    println!("safe_first       : {}", ctx.state.policy.safe_first);
    println!("deny_by_default  : {}", ctx.state.policy.deny_by_default);
    println!("llm_advisory_only: {}", ctx.state.policy.llm_advisory_only);
    println!("enforcement      : {}", ctx.state.policy.enforcement.as_str());
}

fn list_workers(ctx: &RuntimeCtx) {
    if ctx.state.workers.is_empty() {
        println!("No workers.");
        return;
    }

    let now = now_epoch_s();
    for worker in &ctx.state.workers {
        let status = effective_worker_status(worker, now);
        println!(
            "{} | {} | {} | beats={} | last={} ",
            worker.worker_id,
            worker.role,
            status,
            worker.heartbeat_count,
            worker.last_heartbeat_epoch_s
        );
    }
}

fn list_goals(ctx: &RuntimeCtx) {
    if ctx.state.goals.is_empty() {
        println!("No goals.");
        return;
    }

    for goal in &ctx.state.goals {
        println!(
            "{} | {} | prio={} | {} | hold={} | notes={} | {}",
            goal.goal_id,
            goal.status,
            goal.priority,
            goal.origin,
            goal.hold_reason.as_deref().unwrap_or("none"),
            goal.notes.len(),
            goal.title
        );
    }
}

fn print_next_goal(ctx: &RuntimeCtx) {
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

fn inspect_goal(
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

fn render_goal_inspect_text(goal: &Goal, related_events: &[JournalEvent]) -> String {
    let mut lines = vec![
        format!("goal_id       : {}", goal.goal_id),
        format!("title         : {}", goal.title),
        format!("status        : {}", goal.status),
        format!("hold_reason   : {}", goal.hold_reason.as_deref().unwrap_or("none")),
        format!("priority      : {}", goal.priority),
        format!("origin        : {}", goal.origin),
        format!("safety_class  : {}", goal.safety_class),
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

fn render_goal_inspect_markdown(goal: &Goal, related_events: &[JournalEvent]) -> String {
    let mut lines = vec![
        format!("# goal {}", goal.goal_id),
        String::new(),
        format!("- title: {}", goal.title),
        format!("- status: {}", goal.status),
        format!("- hold_reason: {}", goal.hold_reason.as_deref().unwrap_or("none")),
        format!("- priority: {}", goal.priority),
        format!("- origin: {}", goal.origin),
        format!("- safety_class: {}", goal.safety_class),
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

fn render_next_goal_markdown(ctx: &RuntimeCtx) -> String {
    if let Some(goal) = select_next_goal(&ctx.state) {
        let related_events = collect_goal_events(&ctx.paths.journal_path, goal).unwrap_or_default();
        return render_goal_inspect_markdown(goal, &related_events);
    }

    ["# next goal".to_string(), String::new(), "- none".to_string()].join("\n")
}

fn select_next_goal(state: &State) -> Option<&Goal> {
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

fn goal_selection_rank(status: &str) -> u8 {
    match status {
        "doing" => 3,
        "recovering" => 2,
        "pending" => 1,
        _ => 0,
    }
}

fn is_terminal_goal_status(status: &str) -> bool {
    status == "done" || status == "aborted"
}

fn is_actionable_goal_status(status: &str) -> bool {
    status == "doing" || status == "recovering" || status == "pending"
}

fn is_policy_safe_goal(goal: &Goal) -> bool {
    goal.safety_class == "normal"
}

const WORKER_STALE_AFTER_S: u64 = 300;

fn effective_worker_status(worker: &WorkerState, now_epoch_s: u64) -> &'static str {
    if now_epoch_s.saturating_sub(worker.last_heartbeat_epoch_s) > WORKER_STALE_AFTER_S {
        "stale"
    } else {
        "alive"
    }
}

fn count_stale_workers(state: &State, now_epoch_s: u64) -> usize {
    state
        .workers
        .iter()
        .filter(|w| effective_worker_status(w, now_epoch_s) == "stale")
        .count()
}

fn refresh_worker_health(state: &mut State, now_epoch_s: u64) -> usize {
    let mut stale = 0;
    for worker in &mut state.workers {
        let status = effective_worker_status(worker, now_epoch_s).to_string();
        if status == "stale" {
            stale += 1;
        }
        worker.status = status;
    }
    stale
}

fn apply_worker_homeostasis(
    ctx: &mut RuntimeCtx,
    now_epoch_s: u64,
) -> Result<&'static str, Box<dyn std::error::Error>> {
    let stale_workers = refresh_worker_health(&mut ctx.state, now_epoch_s);

    if stale_workers > 0 && !matches!(ctx.state.mode, RuntimeMode::Safe | RuntimeMode::Degraded) {
        let blocked_goals = block_active_goals(&mut ctx.state, now_epoch_s);
        ctx.state.mode = RuntimeMode::Degraded;
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
                    kind: "goal_block".to_string(),
                    severity: "warn".to_string(),
                    summary: format!("goal blocked: {goal_title}"),
                    reason: Some("worker_health_degraded".to_string()),
                    action: Some("goal_set_blocked".to_string()),
                    result: Some("ok".to_string()),
                    continuity_epoch: ctx.state.continuity_epoch,
                },
            )?;
        }
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now_epoch_s,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "worker_health".to_string(),
                severity: "warn".to_string(),
                summary: format!("{} worker(s) stale; mode degraded", stale_workers),
                reason: Some("stale_worker_detected".to_string()),
                action: Some("mode_set_degraded".to_string()),
                result: Some("ok".to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            },
        )?;
        return Ok("mode_degraded");
    }

    if stale_workers == 0 && !ctx.state.workers.is_empty() && matches!(ctx.state.mode, RuntimeMode::Degraded) {
        let recovering_goals = recover_blocked_goals(&mut ctx.state, now_epoch_s);
        ctx.state.mode = RuntimeMode::Normal;
        persist_ctx(ctx)?;
        for goal_title in recovering_goals {
            append_event(
                &ctx.paths.journal_path,
                &JournalEvent {
                    event_id: Uuid::new_v4().to_string(),
                    ts_epoch_s: now_epoch_s,
                    organism_id: ctx.identity.organism_id.clone(),
                    runtime_habitat: ctx.identity.runtime_habitat.clone(),
                    runtime_instance_id: ctx.runtime_instance_id.clone(),
                    kind: "goal_recover".to_string(),
                    severity: "notice".to_string(),
                    summary: format!("goal recovering: {goal_title}"),
                    reason: Some("worker_health_restored".to_string()),
                    action: Some("goal_set_recovering".to_string()),
                    result: Some("ok".to_string()),
                    continuity_epoch: ctx.state.continuity_epoch,
                },
            )?;
        }
        append_event(
            &ctx.paths.journal_path,
            &JournalEvent {
                event_id: Uuid::new_v4().to_string(),
                ts_epoch_s: now_epoch_s,
                organism_id: ctx.identity.organism_id.clone(),
                runtime_habitat: ctx.identity.runtime_habitat.clone(),
                runtime_instance_id: ctx.runtime_instance_id.clone(),
                kind: "worker_health".to_string(),
                severity: "notice".to_string(),
                summary: "workers healthy; mode restored to normal".to_string(),
                reason: Some("worker_health_restored".to_string()),
                action: Some("mode_set_normal".to_string()),
                result: Some("ok".to_string()),
                continuity_epoch: ctx.state.continuity_epoch,
            },
        )?;
        return Ok("mode_restored");
    }

    persist_ctx(ctx)?;
    Ok(if stale_workers > 0 { "stale_unchanged" } else { "healthy_unchanged" })
}

fn apply_policy_homeostasis(
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

fn block_active_goals(state: &mut State, now_epoch_s: u64) -> Vec<String> {
    let mut blocked = Vec::new();
    for goal in &mut state.goals {
        if goal.status == "doing" || goal.status == "recovering" {
            goal.status = "blocked".to_string();
            goal.hold_reason = Some("worker_health".to_string());
            goal.updated_at_epoch_s = now_epoch_s;
            blocked.push(goal.title.clone());
        }
    }
    blocked
}

fn recover_blocked_goals(state: &mut State, now_epoch_s: u64) -> Vec<String> {
    let mut recovering = Vec::new();
    for goal in &mut state.goals {
        if goal.status == "blocked"
            && matches!(goal.hold_reason.as_deref(), Some("worker_health"))
        {
            goal.status = "recovering".to_string();
            goal.hold_reason = None;
            goal.updated_at_epoch_s = now_epoch_s;
            recovering.push(goal.title.clone());
        }
    }
    recovering
}

fn block_policy_unsafe_goals(state: &mut State, now_epoch_s: u64) -> Vec<String> {
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

fn release_policy_held_goals(state: &mut State, now_epoch_s: u64) -> Vec<String> {
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

fn tail_journal(path: &Path, count: usize) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        println!("Journal is empty.");
        return Ok(());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;
    let start = lines.len().saturating_sub(count);
    for line in lines.drain(start..) {
        println!("{line}");
    }
    Ok(())
}

fn explain_journal(path: &Path, count: usize) -> Result<(), Box<dyn std::error::Error>> {
    let events = read_recent_events(path, count)?;
    if events.is_empty() {
        println!("Journal is empty.");
        return Ok(());
    }

    for event in events {
        println!(
            "[{}] {} | {} | {}",
            event.ts_epoch_s,
            event.kind,
            event.severity,
            explain_event(&event)
        );
    }

    Ok(())
}

fn render_journal_explain_markdown(events: &[JournalEvent]) -> String {
    let mut lines = vec!["# journal explain".to_string(), String::new()];

    if events.is_empty() {
        lines.push("- none".to_string());
    } else {
        for event in events {
            lines.push(format!(
                "- [{}] {} | {} | {}",
                event.ts_epoch_s,
                event.kind,
                event.severity,
                explain_event(event)
            ));
        }
    }

    lines.join("\n")
}

fn write_daily_reports(
    ctx: &RuntimeCtx,
    out_dir: &Path,
    journal_count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let status_path = out_dir.join("status.md");
    let next_goal_path = out_dir.join("next-goal.md");
    let journal_path = out_dir.join("journal-explain.md");

    write_text_file(&status_path, &render_status_markdown(ctx))?;
    write_text_file(&next_goal_path, &render_next_goal_markdown(ctx))?;

    let events = read_recent_events(&ctx.paths.journal_path, journal_count)?;
    write_text_file(&journal_path, &render_journal_explain_markdown(&events))?;

    Ok(())
}

fn explain_event(event: &JournalEvent) -> String {
    match event.kind.as_str() {
        "goal_note" => format!(
            "goal note recorded; summary='{}'; action={}",
            event.summary,
            event.action.as_deref().unwrap_or("none")
        ),
        "goal_hold" => format!(
            "operator hold applied; summary='{}'; reason={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none")
        ),
        "goal_policy_hold" => format!(
            "governance hold applied; summary='{}'; reason={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none")
        ),
        "goal_policy_release" => format!(
            "governance hold released; summary='{}'; result={}",
            event.summary,
            event.result.as_deref().unwrap_or("none")
        ),
        "goal_block" => format!(
            "worker incident blocked a goal; summary='{}'; reason={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none")
        ),
        "goal_recover" => format!(
            "goal entered recovery; summary='{}'; reason={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none")
        ),
        "goal_resume" => format!(
            "goal resumed; summary='{}'; action={}",
            event.summary,
            event.action.as_deref().unwrap_or("none")
        ),
        "worker_health" => format!(
            "worker health transition; summary='{}'; action={}; result={}",
            event.summary,
            event.action.as_deref().unwrap_or("none"),
            event.result.as_deref().unwrap_or("none")
        ),
        "scheduler_tick" => format!(
            "scheduler decision; summary='{}'; reason={}; result={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none"),
            event.result.as_deref().unwrap_or("none")
        ),
        "goal_start" | "goal_complete" | "goal_abort" | "goal_create" => format!(
            "goal lifecycle; summary='{}'; action={}; result={}",
            event.summary,
            event.action.as_deref().unwrap_or("none"),
            event.result.as_deref().unwrap_or("none")
        ),
        "worker_heartbeat" => format!(
            "worker heartbeat observed; summary='{}'; result={}",
            event.summary,
            event.result.as_deref().unwrap_or("none")
        ),
        "policy_decision" => format!(
            "policy changed; summary='{}'; action={}",
            event.summary,
            event.action.as_deref().unwrap_or("none")
        ),
        _ => format!(
            "summary='{}'; reason={}; action={}; result={}",
            event.summary,
            event.reason.as_deref().unwrap_or("none"),
            event.action.as_deref().unwrap_or("none"),
            event.result.as_deref().unwrap_or("none")
        ),
    }
}

fn save_state(path: &Path, state: &State) -> Result<(), Box<dyn std::error::Error>> {
    write_json(path, state)
}

fn save_recovery_snapshot(path: &Path, state: &State) -> Result<(), Box<dyn std::error::Error>> {
    write_json(path, state)
}

fn append_event(path: &Path, event: &JournalEvent) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn emit_report(contents: &str, out: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = out {
        write_text_file(path, contents)?;
        println!("OK: wrote report to {}", path.display());
    } else {
        println!("{contents}");
    }
    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = path.with_extension("tmp");
    let mut file = File::create(&tmp)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    drop(file);
    fs::rename(tmp, path)?;
    Ok(())
}

fn write_text_file(path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let mut file = File::create(&tmp)?;
    file.write_all(contents.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()?;
    drop(file);
    fs::rename(tmp, path)?;
    Ok(())
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let truncated: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    Ok(serde_json::from_reader(file)?)
}

fn now_epoch_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn detect_habitat() -> &'static str {
    if cfg!(target_os = "windows") {
        "host_windows"
    } else if cfg!(target_os = "macos") {
        "host_macos"
    } else if cfg!(target_os = "linux") {
        "host_linux"
    } else {
        "host_unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

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
            priority,
            created_at_epoch_s,
            updated_at_epoch_s: created_at_epoch_s,
            origin: "test".to_string(),
            safety_class: "normal".to_string(),
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
    fn block_active_goals_moves_doing_and_recovering_to_blocked() {
        let mut state = sample_state(vec![
            goal("g1", "doing", "doing", 1, 1),
            goal("g2", "recovering", "recovering", 1, 2),
            goal("g3", "pending", "pending", 1, 3),
        ]);

        let blocked = block_active_goals(&mut state, 99);
        assert_eq!(blocked.len(), 2);
        assert_eq!(state.goals[0].status, "blocked");
        assert_eq!(state.goals[0].hold_reason.as_deref(), Some("worker_health"));
        assert_eq!(state.goals[1].status, "blocked");
        assert_eq!(state.goals[1].hold_reason.as_deref(), Some("worker_health"));
        assert_eq!(state.goals[2].status, "pending");
    }

    #[test]
    fn recover_blocked_goals_moves_blocked_to_recovering() {
        let mut state = sample_state(vec![
            Goal {
                hold_reason: Some("worker_health".to_string()),
                ..goal("g1", "blocked", "blocked", 1, 1)
            },
            goal("g2", "pending", "pending", 1, 2),
        ]);

        let recovering = recover_blocked_goals(&mut state, 100);
        assert_eq!(recovering.len(), 1);
        assert_eq!(state.goals[0].status, "recovering");
        assert_eq!(state.goals[0].hold_reason, None);
        assert_eq!(state.goals[1].status, "pending");
    }

    #[test]
    fn recover_blocked_goals_ignores_operator_hold() {
        let mut state = sample_state(vec![
            Goal {
                hold_reason: Some("operator_hold".to_string()),
                ..goal("g1", "blocked", "blocked", 1, 1)
            },
            goal("g2", "pending", "pending", 1, 2),
        ]);

        let recovering = recover_blocked_goals(&mut state, 100);
        assert!(recovering.is_empty());
        assert_eq!(state.goals[0].status, "blocked");
        assert_eq!(state.goals[0].hold_reason.as_deref(), Some("operator_hold"));
    }

    #[test]
    fn explain_event_formats_worker_health_transition() {
        let event = JournalEvent {
            event_id: "e1".to_string(),
            ts_epoch_s: 1,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "worker_health".to_string(),
            severity: "warn".to_string(),
            summary: "1 worker stale; mode degraded".to_string(),
            reason: Some("stale_worker_detected".to_string()),
            action: Some("mode_set_degraded".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: 0,
        };

        let text = explain_event(&event);
        assert!(text.contains("worker health transition"));
        assert!(text.contains("mode_set_degraded"));
        assert!(text.contains("ok"));
    }

    #[test]
    fn explain_event_formats_policy_hold() {
        let event = JournalEvent {
            event_id: "e2".to_string(),
            ts_epoch_s: 1,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "goal_policy_hold".to_string(),
            severity: "warn".to_string(),
            summary: "goal held by policy: unsafe".to_string(),
            reason: Some("policy_hold".to_string()),
            action: Some("goal_set_blocked".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: 0,
        };

        let text = explain_event(&event);
        assert!(text.contains("governance hold applied"));
        assert!(text.contains("policy_hold"));
    }

    #[test]
    fn truncate_text_adds_ellipsis_when_needed() {
        let text = truncate_text("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(text, "abcdefghij...");
    }

    #[test]
    fn goal_note_can_be_appended_without_state_transition() {
        let mut goal = goal("g1", "note me", "doing", 1, 1);
        goal.notes.push(GoalNote {
            ts_epoch_s: 10,
            author: "operator".to_string(),
            text: "remember this".to_string(),
        });

        assert_eq!(goal.status, "doing");
        assert_eq!(goal.notes.len(), 1);
        assert_eq!(goal.notes[0].author, "operator");
        assert_eq!(goal.notes[0].text, "remember this");
    }

    #[test]
    fn event_mentions_goal_matches_title_and_id() {
        let goal = goal("g1", "inspect me", "doing", 1, 1);
        let by_title = JournalEvent {
            event_id: "e1".to_string(),
            ts_epoch_s: 1,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "goal_note".to_string(),
            severity: "info".to_string(),
            summary: "goal note added: inspect me".to_string(),
            reason: None,
            action: Some("goal_note_add".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: 0,
        };
        let by_id = JournalEvent {
            event_id: "e2".to_string(),
            ts_epoch_s: 1,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "scheduler_tick".to_string(),
            severity: "notice".to_string(),
            summary: "scheduler activated goal: g1".to_string(),
            reason: Some("selected_pending_goal".to_string()),
            action: Some("tick_start_goal".to_string()),
            result: Some("goal_started".to_string()),
            continuity_epoch: 0,
        };
        let other = JournalEvent {
            event_id: "e3".to_string(),
            ts_epoch_s: 1,
            organism_id: "o1".to_string(),
            runtime_habitat: "host".to_string(),
            runtime_instance_id: "r1".to_string(),
            kind: "goal_note".to_string(),
            severity: "info".to_string(),
            summary: "goal note added: someone else".to_string(),
            reason: None,
            action: Some("goal_note_add".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: 0,
        };

        assert!(event_mentions_goal(&by_title, &goal));
        assert!(event_mentions_goal(&by_id, &goal));
        assert!(!event_mentions_goal(&other, &goal));
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
    fn render_goal_inspect_markdown_contains_notes_and_events() {
        let mut goal = goal("g1", "inspect me", "doing", 3, 1);
        goal.notes.push(GoalNote {
            ts_epoch_s: 10,
            author: "operator".to_string(),
            text: "important context".to_string(),
        });
        let events = vec![JournalEvent {
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

        let markdown = render_goal_inspect_markdown(&goal, &events);
        assert!(markdown.contains("# goal g1"));
        assert!(markdown.contains("## notes"));
        assert!(markdown.contains("important context"));
        assert!(markdown.contains("## recent events"));
        assert!(markdown.contains("goal note recorded"));
    }

    #[test]
    fn render_journal_explain_markdown_contains_event_explanations() {
        let events = vec![JournalEvent {
            event_id: "e1".to_string(),
            ts_epoch_s: 11,
            organism_id: "org-1".to_string(),
            runtime_habitat: "host_test".to_string(),
            runtime_instance_id: "run-1".to_string(),
            kind: "worker_health".to_string(),
            severity: "warn".to_string(),
            summary: "1 worker stale; mode degraded".to_string(),
            reason: Some("stale_worker_detected".to_string()),
            action: Some("mode_set_degraded".to_string()),
            result: Some("ok".to_string()),
            continuity_epoch: 0,
        }];

        let markdown = render_journal_explain_markdown(&events);
        assert!(markdown.contains("# journal explain"));
        assert!(markdown.contains("worker health transition"));
        assert!(markdown.contains("mode_set_degraded"));
    }

    #[test]
    fn render_goal_inspect_text_contains_notes_and_events() {
        let mut goal = goal("g1", "inspect me", "doing", 3, 1);
        goal.notes.push(GoalNote {
            ts_epoch_s: 10,
            author: "operator".to_string(),
            text: "important context".to_string(),
        });
        let events = vec![JournalEvent {
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

        let text = render_goal_inspect_text(&goal, &events);
        assert!(text.contains("goal_id       : g1"));
        assert!(text.contains("note_count    : 1"));
        assert!(text.contains("important context"));
        assert!(text.contains("recent_events :"));
    }

    #[test]
    fn write_text_file_persists_report_contents() {
        let dir = env::temp_dir().join(format!("oo-host-test-out-{}", Uuid::new_v4()));
        let path = dir.join("report.md");

        write_text_file(&path, "# report\nhello").expect("write report");
        let saved = fs::read_to_string(&path).expect("read report");
        assert!(saved.contains("# report"));
        assert!(saved.contains("hello"));

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&dir);
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
            &JournalEvent {
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
            },
        )
        .expect("append event");

        write_daily_reports(&ctx, &report_dir, 20).expect("write daily reports");

        let status = fs::read_to_string(report_dir.join("status.md")).expect("read status");
        let next_goal = fs::read_to_string(report_dir.join("next-goal.md")).expect("read next-goal");
        let journal = fs::read_to_string(report_dir.join("journal-explain.md")).expect("read journal");

        assert!(status.contains("# oo-host status"));
        assert!(next_goal.contains("# goal g1") || next_goal.contains("# next goal"));
        assert!(journal.contains("# journal explain"));

        let _ = fs::remove_dir_all(&dir);
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

    #[test]
    fn worker_health_marks_worker_stale_after_threshold() {
        let mut state = sample_state(Vec::new());
        state.workers.push(WorkerState {
            worker_id: "w1".to_string(),
            role: "clock".to_string(),
            status: "alive".to_string(),
            last_heartbeat_epoch_s: 10,
            heartbeat_count: 1,
        });

        let stale = refresh_worker_health(&mut state, 10 + WORKER_STALE_AFTER_S + 1);
        assert_eq!(stale, 1);
        assert_eq!(state.workers[0].status, "stale");
    }

    #[test]
    fn worker_health_keeps_recent_worker_alive() {
        let mut state = sample_state(Vec::new());
        state.workers.push(WorkerState {
            worker_id: "w1".to_string(),
            role: "clock".to_string(),
            status: "unknown".to_string(),
            last_heartbeat_epoch_s: 100,
            heartbeat_count: 2,
        });

        let stale = refresh_worker_health(&mut state, 100 + WORKER_STALE_AFTER_S);
        assert_eq!(stale, 0);
        assert_eq!(state.workers[0].status, "alive");
    }

    #[test]
    fn stale_worker_count_matches_status() {
        let mut state = sample_state(Vec::new());
        state.workers.push(WorkerState {
            worker_id: "w1".to_string(),
            role: "clock".to_string(),
            status: "alive".to_string(),
            last_heartbeat_epoch_s: 1,
            heartbeat_count: 1,
        });
        state.workers.push(WorkerState {
            worker_id: "w2".to_string(),
            role: "fs".to_string(),
            status: "alive".to_string(),
            last_heartbeat_epoch_s: 1 + WORKER_STALE_AFTER_S + 5,
            heartbeat_count: 1,
        });

        assert_eq!(count_stale_workers(&state, 1 + WORKER_STALE_AFTER_S + 10), 1);
    }
}
