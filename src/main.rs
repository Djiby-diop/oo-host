use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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
    Status,
    Goal(GoalCommand),
    Goals(GoalsCommand),
    Journal(JournalCommand),
    Mode(ModeCommand),
    Policy(PolicyCommand),
    Recover,
    Export(ExportCommand),
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
}

#[derive(Args, Debug)]
struct GoalsCommand {
    #[command(subcommand)]
    command: GoalsSubcommand,
}

#[derive(Subcommand, Debug)]
enum GoalsSubcommand {
    List,
}

#[derive(Args, Debug)]
struct JournalCommand {
    #[command(subcommand)]
    command: JournalSubcommand,
}

#[derive(Subcommand, Debug)]
enum JournalSubcommand {
    Tail {
        #[arg(short = 'n', long, default_value_t = 20)]
        count: usize,
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

impl PolicyEnforcement {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Observe => "observe",
            Self::Enforce => "enforce",
        }
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
    goals: Vec<Goal>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Goal {
    goal_id: String,
    title: String,
    status: String,
    priority: i32,
    created_at_epoch_s: u64,
    updated_at_epoch_s: u64,
    origin: String,
    safety_class: String,
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
        Command::Status => print_status(&ctx),
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
        },
        Command::Goals(goals) => match goals.command {
            GoalsSubcommand::List => list_goals(&ctx),
        },
        Command::Journal(journal) => match journal.command {
            JournalSubcommand::Tail { count } => tail_journal(&ctx.paths.journal_path, count)?,
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

fn persist_ctx(ctx: &RuntimeCtx) -> Result<(), Box<dyn std::error::Error>> {
    save_state(&ctx.paths.state_path, &ctx.state)?;
    save_recovery_snapshot(&ctx.paths.recovery_path, &ctx.state)?;
    Ok(())
}

fn print_status(ctx: &RuntimeCtx) {
    println!("organism_id       : {}", ctx.identity.organism_id);
    println!("genesis_id        : {}", ctx.identity.genesis_id);
    println!("runtime_habitat   : {}", ctx.identity.runtime_habitat);
    println!("runtime_instance  : {}", ctx.runtime_instance_id);
    println!("start_count       : {}", ctx.state.boot_or_start_count);
    println!("continuity_epoch  : {}", ctx.state.continuity_epoch);
    println!("mode              : {}", ctx.state.mode.as_str());
    println!("policy            : {}", ctx.state.policy.enforcement.as_str());
    println!("last_clean        : {}", ctx.state.last_clean_shutdown);
    println!(
        "last_recovery     : {}",
        ctx.state.last_recovery_reason.as_deref().unwrap_or("none")
    );
    println!("goals             : {}", ctx.state.goals.len());
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

fn list_goals(ctx: &RuntimeCtx) {
    if ctx.state.goals.is_empty() {
        println!("No goals.");
        return;
    }

    for goal in &ctx.state.goals {
        println!(
            "{} | {} | prio={} | {} | {}",
            goal.goal_id, goal.status, goal.priority, goal.origin, goal.title
        );
    }
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

fn save_state(path: &Path, state: &State) -> Result<(), Box<dyn std::error::Error>> {
    write_json(path, state)
}

fn save_recovery_snapshot(path: &Path, state: &State) -> Result<(), Box<dyn std::error::Error>> {
    write_json(path, state)
}

fn append_event(path: &Path, event: &JournalEvent) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
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
