use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub identity_path: PathBuf,
    pub state_path: PathBuf,
    pub journal_path: PathBuf,
    pub recovery_path: PathBuf,
}

impl AppPaths {
    pub fn new(root: PathBuf) -> Self {
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
pub struct Identity {
    pub organism_id: String,
    pub genesis_id: String,
    pub runtime_habitat: String,
    pub created_at_epoch_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Normal,
    Degraded,
    Safe,
}

impl RuntimeMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Degraded => "degraded",
            Self::Safe => "safe",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEnforcement {
    Off,
    Observe,
    Enforce,
}

impl PolicyEnforcement {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Observe => "observe",
            Self::Enforce => "enforce",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Text,
    Markdown,
}

impl OutputFormat {
    pub fn is_markdown(self) -> bool {
        matches!(self, Self::Markdown)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyState {
    pub safe_first: bool,
    pub deny_by_default: bool,
    pub llm_advisory_only: bool,
    pub enforcement: PolicyEnforcement,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub boot_or_start_count: u64,
    pub continuity_epoch: u64,
    pub last_clean_shutdown: bool,
    pub last_recovery_reason: Option<String>,
    pub last_started_at_epoch_s: u64,
    #[serde(default = "crate::state::default_runtime_mode")]
    pub mode: RuntimeMode,
    #[serde(default = "crate::state::default_policy_state")]
    pub policy: PolicyState,
    #[serde(default)]
    pub workers: Vec<WorkerState>,
    pub goals: Vec<Goal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerState {
    pub worker_id: String,
    pub role: String,
    pub status: String,
    pub last_heartbeat_epoch_s: u64,
    pub heartbeat_count: u64,
    #[serde(default)]
    pub stale_after_s: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Goal {
    pub goal_id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub hold_reason: Option<String>,
    #[serde(default)]
    pub notes: Vec<GoalNote>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub priority: i32,
    pub created_at_epoch_s: u64,
    pub updated_at_epoch_s: u64,
    pub origin: String,
    pub safety_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalNote {
    pub ts_epoch_s: u64,
    pub author: String,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JournalEvent {
    pub event_id: String,
    pub ts_epoch_s: u64,
    pub organism_id: String,
    pub runtime_habitat: String,
    pub runtime_instance_id: String,
    pub kind: String,
    pub severity: String,
    pub summary: String,
    pub reason: Option<String>,
    pub action: Option<String>,
    pub result: Option<String>,
    pub continuity_epoch: u64,
}

#[derive(Debug, Serialize)]
pub struct SovereignExport<'a> {
    pub schema_version: u32,
    pub export_kind: &'static str,
    pub generated_at_epoch_s: u64,
    pub organism_id: &'a str,
    pub genesis_id: &'a str,
    pub runtime_habitat: &'a str,
    pub runtime_instance_id: &'a str,
    pub continuity_epoch: u64,
    pub boot_or_start_count: u64,
    pub mode: &'a str,
    pub last_recovery_reason: Option<&'a str>,
    pub policy: SovereignPolicyExport<'a>,
    pub active_goal_count: usize,
    pub top_goals: Vec<SovereignGoalExport<'a>>,
    pub recent_events: Vec<SovereignEventExport>,
}

#[derive(Debug, Serialize)]
pub struct SovereignPolicyExport<'a> {
    pub safe_first: bool,
    pub deny_by_default: bool,
    pub llm_advisory_only: bool,
    pub enforcement: &'a str,
}

#[derive(Debug, Serialize)]
pub struct SovereignGoalExport<'a> {
    pub goal_id: &'a str,
    pub title: &'a str,
    pub status: &'a str,
    pub priority: i32,
    pub safety_class: &'a str,
}

#[derive(Debug, Serialize)]
pub struct SovereignEventExport {
    pub ts_epoch_s: u64,
    pub kind: String,
    pub severity: String,
    pub summary: String,
    pub reason: Option<String>,
    pub action: Option<String>,
    pub result: Option<String>,
    pub continuity_epoch: u64,
}

pub struct RuntimeCtx {
    pub paths: AppPaths,
    pub identity: Identity,
    pub state: State,
    pub runtime_instance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncMismatch {
    pub field: String,
    pub host_value: String,
    pub receipt_value: String,
}
