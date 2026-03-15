use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use clap::{Parser, Subcommand, ValueEnum};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser, Debug)]
#[command(name = "oo-bot")]
#[command(about = "Operator/assistant companion for oo-host")]
struct Cli {
	#[arg(long, default_value = "data")]
	data_dir: PathBuf,

	#[command(subcommand)]
	command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
	Status,
	Brief,
	Next,
	GithubBrief {
		#[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
		format: OutputFormat,
	},
	GithubIssue {
		title: String,
		#[arg(long)]
		focus: Option<String>,
		#[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
		format: OutputFormat,
	},
	GithubPr {
		title: String,
		#[arg(long)]
		head: Option<String>,
		#[arg(long, default_value = "main")]
		base: String,
		#[arg(long)]
		focus: Option<String>,
		#[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
		format: OutputFormat,
	},
	ProtectManifest {
		#[arg(long)]
		workspace: PathBuf,
		#[arg(long)]
		out: Option<PathBuf>,
	},
	ProtectStatus {
		#[arg(long)]
		workspace: PathBuf,
	},
	ProtectVerify {
		#[arg(long)]
		workspace: PathBuf,
		#[arg(long)]
		manifest: PathBuf,
	},
	ProtectKeygen {
		#[arg(long)]
		out: Option<PathBuf>,
	},
	ProtectStamp {
		#[arg(long)]
		manifest: PathBuf,
		#[arg(long)]
		key: Option<PathBuf>,
		#[arg(long)]
		out: Option<PathBuf>,
	},
	SovereignStatus {
		#[arg(long, default_value = "../llm-baremetal")]
		workspace: PathBuf,
	},
	HandoffCheck {
		#[arg(long, default_value = "../llm-baremetal")]
		workspace: PathBuf,
		#[arg(long)]
		export: Option<PathBuf>,
	},
	SovereignBrief {
		#[arg(long, default_value = "../llm-baremetal")]
		workspace: PathBuf,
		#[arg(long)]
		export: Option<PathBuf>,
		#[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
		format: OutputFormat,
	},
	GithubSovereignBrief {
		#[arg(long, default_value = "../llm-baremetal")]
		workspace: PathBuf,
		#[arg(long)]
		export: Option<PathBuf>,
		#[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
		format: OutputFormat,
	},
	GithubSovereignIssue {
		title: String,
		#[arg(long, default_value = "../llm-baremetal")]
		workspace: PathBuf,
		#[arg(long)]
		export: Option<PathBuf>,
		#[arg(long)]
		focus: Option<String>,
		#[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
		format: OutputFormat,
	},
	GithubSovereignPr {
		title: String,
		#[arg(long, default_value = "../llm-baremetal")]
		workspace: PathBuf,
		#[arg(long)]
		export: Option<PathBuf>,
		#[arg(long)]
		head: Option<String>,
		#[arg(long, default_value = "main")]
		base: String,
		#[arg(long)]
		focus: Option<String>,
		#[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
		format: OutputFormat,
	},
	GithubSovereignPack {
		#[arg(long, default_value = "../llm-baremetal")]
		workspace: PathBuf,
		#[arg(long)]
		export: Option<PathBuf>,
		#[arg(long, default_value = "Sovereign integration follow-up")]
		issue_title: String,
		#[arg(long, default_value = "Sovereign integration update")]
		pr_title: String,
		#[arg(long)]
		head: Option<String>,
		#[arg(long, default_value = "main")]
		base: String,
		#[arg(long)]
		focus: Option<String>,
		#[arg(long)]
		out: Option<PathBuf>,
	},
	ReceiptCheck {
		#[arg(long, default_value = "../llm-baremetal")]
		workspace: PathBuf,
		#[arg(long)]
		receipt: Option<PathBuf>,
	},
	SyncCheck {
		#[arg(long, default_value = "../llm-baremetal")]
		workspace: PathBuf,
		#[arg(long)]
		export: Option<PathBuf>,
		#[arg(long)]
		receipt: Option<PathBuf>,
	},
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
	Markdown,
	Text,
}

#[derive(Debug)]
struct AppPaths {
	identity_path: PathBuf,
	state_path: PathBuf,
	journal_path: PathBuf,
	sovereign_export_path: PathBuf,
}

impl AppPaths {
	fn new(root: PathBuf) -> Self {
		Self {
			identity_path: root.join("organism_identity.json"),
			state_path: root.join("organism_state.json"),
			journal_path: root.join("organism_journal.jsonl"),
			sovereign_export_path: root.join("sovereign_export.json"),
		}
	}
}

#[derive(Debug, Deserialize)]
struct Identity {
	organism_id: String,
	genesis_id: String,
	runtime_habitat: String,
	created_at_epoch_s: u64,
}

#[derive(Debug, Clone, Deserialize)]
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

	fn rank(&self) -> u8 {
		match self {
			Self::Normal => 0,
			Self::Degraded => 1,
			Self::Safe => 2,
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
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

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct PolicyState {
	safe_first: bool,
	deny_by_default: bool,
	llm_advisory_only: bool,
	enforcement: PolicyEnforcement,
}

#[derive(Debug, Deserialize)]
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

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct State {
	boot_or_start_count: u64,
	continuity_epoch: u64,
	last_clean_shutdown: bool,
	last_recovery_reason: Option<String>,
	last_started_at_epoch_s: u64,
	mode: RuntimeMode,
	policy: PolicyState,
	goals: Vec<Goal>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
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

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SovereignExport {
	schema_version: u32,
	export_kind: String,
	continuity_epoch: u64,
	mode: String,
	last_recovery_reason: Option<String>,
}

#[derive(Debug)]
struct Snapshot {
	identity: Identity,
	state: State,
	recent_events: Vec<JournalEvent>,
	sovereign_export: Option<SovereignExport>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProtectionManifest {
	schema_version: u32,
	manifest_kind: String,
	generated_at_epoch_s: u64,
	organism_id: String,
	runtime_habitat: String,
	workspace_root: String,
	file_count: usize,
	entries: Vec<ProtectionEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProtectionEntry {
	rel_path: String,
	sha256: String,
	bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProtectionKeyPair {
	schema_version: u32,
	key_kind: String,
	created_at_epoch_s: u64,
	organism_id: String,
	public_key_base64: String,
	secret_key_base64: String,
}

#[derive(Debug, Serialize)]
struct ProtectionAttestation {
	schema_version: u32,
	attestation_kind: &'static str,
	sealed_at_epoch_s: u64,
	organism_id: String,
	runtime_habitat: String,
	manifest_path: String,
	manifest_sha256: String,
	manifest_generated_at_epoch_s: u64,
	workspace_root: String,
	file_count: usize,
	signature: Option<ProtectionSignature>,
}

#[derive(Debug, Serialize)]
struct ProtectionSignature {
	algorithm: &'static str,
	public_key_base64: String,
	signature_base64: String,
}

#[derive(Debug)]
struct ManifestDiff {
	added: Vec<String>,
	changed: Vec<String>,
	removed: Vec<String>,
}

#[derive(Debug)]
struct SovereignBriefData {
	workspace_root: PathBuf,
	layout: &'static str,
	continuity: &'static str,
	export_validation_ok: bool,
	smoke_ok: bool,
	mode: String,
	policy: String,
	manifest_present: bool,
	attestation_present: bool,
	issues: Vec<String>,
	smoke_missing: Vec<&'static str>,
	actions: Vec<String>,
}

#[derive(Debug)]
struct HandoffReceipt {
	organism_id: String,
	mode: String,
	policy_enforcement: String,
	continuity_epoch: u64,
	last_recovery_reason: Option<String>,
}

#[derive(Debug)]
struct HandoffExportSummary {
	organism_id: String,
	mode: String,
	policy_enforcement: String,
	continuity_epoch: u64,
	last_recovery_reason: Option<String>,
	issues: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let cli = Cli::parse();
	let paths = AppPaths::new(cli.data_dir);
	let snapshot = load_snapshot(&paths)?;

	match cli.command {
		Command::Status => print_status(&snapshot),
		Command::Brief => print_brief(&snapshot),
		Command::Next => print_next_actions(&snapshot),
		Command::GithubBrief { format } => print_github_brief(&snapshot, format),
		Command::GithubIssue { title, focus, format } => {
			print_github_issue(&snapshot, &title, focus.as_deref(), format)
		}
		Command::GithubPr {
			title,
			head,
			base,
			focus,
			format,
		} => print_github_pr(&snapshot, &title, head.as_deref(), &base, focus.as_deref(), format),
		Command::ProtectManifest { workspace, out } => {
			let out_path = out.unwrap_or_else(|| paths.identity_path.parent().unwrap_or(Path::new("data")).join("code_protection_manifest.json"));
			let manifest = build_protection_manifest(&snapshot, &workspace)?;
			write_json_pretty(&out_path, &manifest)?;
			println!("OK: protection manifest written to {}", out_path.display());
			println!("files_hashed: {}", manifest.file_count);
		}
		Command::ProtectStatus { workspace } => print_protection_status(&snapshot, &workspace)?,
		Command::ProtectVerify { workspace, manifest } => {
			print_protection_verify(&snapshot, &workspace, &manifest)?
		}
		Command::ProtectKeygen { out } => {
			let out_path = out.unwrap_or_else(|| paths.identity_path.parent().unwrap_or(Path::new("data")).join("protection_ed25519_key.json"));
			let keypair = generate_protection_keypair(&snapshot);
			write_json_pretty(&out_path, &keypair)?;
			println!("OK: protection signing key written to {}", out_path.display());
			println!("public_key_base64: {}", keypair.public_key_base64);
		}
		Command::ProtectStamp { manifest, key, out } => {
			let manifest_data: ProtectionManifest = read_json(&manifest)?;
			let out_path = out.unwrap_or_else(|| paths.identity_path.parent().unwrap_or(Path::new("data")).join("code_protection_attestation.json"));
			let attestation = build_protection_attestation(&snapshot, &manifest, &manifest_data, key.as_deref())?;
			write_json_pretty(&out_path, &attestation)?;
			println!("OK: protection attestation written to {}", out_path.display());
			println!("manifest_sha256: {}", attestation.manifest_sha256);
			println!("signed: {}", attestation.signature.is_some());
		}
		Command::SovereignStatus { workspace } => {
			print_sovereign_status(&snapshot, &paths, &workspace)?
		}
		Command::HandoffCheck { workspace, export } => {
			let export_path = export.unwrap_or_else(|| paths.sovereign_export_path.clone());
			print_handoff_check(&snapshot, &workspace, &export_path)?
		}
		Command::SovereignBrief {
			workspace,
			export,
			format,
		} => {
			let export_path = export.unwrap_or_else(|| paths.sovereign_export_path.clone());
			print_sovereign_brief(&snapshot, &paths, &workspace, &export_path, format)?
		}
		Command::GithubSovereignBrief {
			workspace,
			export,
			format,
		} => {
			let export_path = export.unwrap_or_else(|| paths.sovereign_export_path.clone());
			print_github_sovereign_brief(&snapshot, &paths, &workspace, &export_path, format)?
		}
		Command::GithubSovereignIssue {
			title,
			workspace,
			export,
			focus,
			format,
		} => {
			let export_path = export.unwrap_or_else(|| paths.sovereign_export_path.clone());
			print_github_sovereign_issue(&snapshot, &paths, &title, &workspace, &export_path, focus.as_deref(), format)?
		}
		Command::GithubSovereignPr {
			title,
			workspace,
			export,
			head,
			base,
			focus,
			format,
		} => {
			let export_path = export.unwrap_or_else(|| paths.sovereign_export_path.clone());
			print_github_sovereign_pr(&snapshot, &paths, &title, &workspace, &export_path, head.as_deref(), &base, focus.as_deref(), format)?
		}
		Command::GithubSovereignPack {
			workspace,
			export,
			issue_title,
			pr_title,
			head,
			base,
			focus,
			out,
		} => {
			let export_path = export.unwrap_or_else(|| paths.sovereign_export_path.clone());
			let out_dir = out.unwrap_or_else(|| paths.identity_path.parent().unwrap_or(Path::new("data")).join("github-sovereign"));
			write_github_sovereign_pack(&snapshot, &paths, &workspace, &export_path, &issue_title, &pr_title, head.as_deref(), &base, focus.as_deref(), &out_dir)?
		}
		Command::ReceiptCheck { workspace, receipt } => {
			let receipt_path = receipt.unwrap_or_else(|| workspace.join("OOHANDOFF.TXT"));
			print_receipt_check(&snapshot, &workspace, &receipt_path)?
		}
		Command::SyncCheck {
			workspace,
			export,
			receipt,
		} => {
			let export_path = export.unwrap_or_else(|| paths.sovereign_export_path.clone());
			let receipt_path = receipt.unwrap_or_else(|| workspace.join("OOHANDOFF.TXT"));
			let ok = print_sync_check(&snapshot, &workspace, &export_path, &receipt_path)?;
			if !ok {
				std::process::exit(2);
			}
		}
	}

	Ok(())
}

fn load_snapshot(paths: &AppPaths) -> Result<Snapshot, Box<dyn std::error::Error>> {
	let identity: Identity = read_json(&paths.identity_path)?;
	let state: State = read_json(&paths.state_path)?;
	let recent_events = read_journal_tail(&paths.journal_path, 8)?;
	let sovereign_export = if paths.sovereign_export_path.exists() {
		Some(read_json(&paths.sovereign_export_path)?)
	} else {
		None
	};

	Ok(Snapshot {
		identity,
		state,
		recent_events,
		sovereign_export,
	})
}

fn print_status(snapshot: &Snapshot) {
	let active_goals = active_goals(&snapshot.state);
	println!("oo-bot status");
	println!("organism_id        : {}", snapshot.identity.organism_id);
	println!("genesis_id         : {}", snapshot.identity.genesis_id);
	println!("runtime_habitat    : {}", snapshot.identity.runtime_habitat);
	println!("created_at_epoch_s : {}", snapshot.identity.created_at_epoch_s);
	println!("mode               : {}", snapshot.state.mode.as_str());
	println!("policy.enforcement : {}", snapshot.state.policy.enforcement.as_str());
	println!("continuity_epoch   : {}", snapshot.state.continuity_epoch);
	println!("boot_or_start_count: {}", snapshot.state.boot_or_start_count);
	println!("last_clean_shutdown: {}", snapshot.state.last_clean_shutdown);
	println!(
		"last_recovery_reason: {}",
		snapshot.state.last_recovery_reason.as_deref().unwrap_or("none")
	);
	println!("active_goals       : {}", active_goals.len());
	println!("recent_events      : {}", snapshot.recent_events.len());
	println!("sovereign_export   : {}", if snapshot.sovereign_export.is_some() { "present" } else { "absent" });
}

fn print_brief(snapshot: &Snapshot) {
	let active_goals = active_goals(&snapshot.state);
	let top_goals = top_goals(&active_goals, 3);
	let continuity = continuity_summary(snapshot);

	println!("OO Bot Brief");
	println!("============");
	println!(
		"Habitat {} / mode {} / policy {} / continuity {}",
		snapshot.identity.runtime_habitat,
		snapshot.state.mode.as_str(),
		snapshot.state.policy.enforcement.as_str(),
		snapshot.state.continuity_epoch
	);
	println!("Continuity assessment: {}", continuity);
	println!(
		"Last recovery: {}",
		snapshot.state.last_recovery_reason.as_deref().unwrap_or("none")
	);
	println!("Active goals: {}", active_goals.len());
	for goal in top_goals {
		println!(
			"- [{}] {} (prio {}, safety {}, origin {})",
			goal.status, goal.title, goal.priority, goal.safety_class, goal.origin
		);
	}

	println!("Recent events:");
	for ev in snapshot.recent_events.iter().rev().take(5).rev() {
		println!(
			"- {} [{}] {}",
			ev.kind,
			ev.severity,
			ev.summary
		);
	}
}

fn print_next_actions(snapshot: &Snapshot) {
	for (idx, action) in recommend_actions(snapshot).iter().enumerate() {
		println!("{}. {}", idx + 1, action);
	}
}

fn print_github_brief(snapshot: &Snapshot, format: OutputFormat) {
	let actions = recommend_actions(snapshot);
	let active_goals = active_goals(&snapshot.state);
	let top_goals = top_goals(&active_goals, 3);
	let continuity = continuity_summary(snapshot);

	match format {
		OutputFormat::Markdown => {
			println!("## OO Bot Brief");
			println!();
			println!("- organism: `{}`", snapshot.identity.organism_id);
			println!("- habitat: `{}`", snapshot.identity.runtime_habitat);
			println!("- mode: `{}`", snapshot.state.mode.as_str());
			println!("- policy: `{}`", snapshot.state.policy.enforcement.as_str());
			println!("- continuity_epoch: `{}`", snapshot.state.continuity_epoch);
			println!("- continuity_assessment: `{}`", continuity);
			println!();
			println!("### Top goals");
			for goal in top_goals {
				println!(
					"- `{}` — {} (prio {}, safety {})",
					goal.goal_id, goal.title, goal.priority, goal.safety_class
				);
			}
			println!();
			println!("### Recent events");
			for ev in snapshot.recent_events.iter().rev().take(5).rev() {
				println!("- `{}` [{}] {}", ev.kind, ev.severity, ev.summary);
			}
			println!();
			println!("### Suggested next actions");
			for action in actions {
				println!("- {}", action);
			}
		}
		OutputFormat::Text => {
			println!("OO Bot GitHub Brief");
			println!("organism={} habitat={}", snapshot.identity.organism_id, snapshot.identity.runtime_habitat);
			println!("mode={} policy={} continuity={}", snapshot.state.mode.as_str(), snapshot.state.policy.enforcement.as_str(), continuity);
			println!("top_goals={}", active_goals.len());
			for goal in top_goals {
				println!("- {}", goal.title);
			}
			println!("next_actions:");
			for action in actions {
				println!("- {}", action);
			}
		}
	}
}

fn print_github_issue(snapshot: &Snapshot, title: &str, focus: Option<&str>, format: OutputFormat) {
	let actions = recommend_actions(snapshot);
	let continuity = continuity_summary(snapshot);
	let active_goals = active_goals(&snapshot.state);
	let top_goals = top_goals(&active_goals, 3);

	match format {
		OutputFormat::Markdown => {
			println!("# {}", title);
			println!();
			println!("## Context");
			println!();
			println!("- organism: `{}`", snapshot.identity.organism_id);
			println!("- habitat: `{}`", snapshot.identity.runtime_habitat);
			println!("- mode: `{}`", snapshot.state.mode.as_str());
			println!("- policy: `{}`", snapshot.state.policy.enforcement.as_str());
			println!("- continuity: `{}`", continuity);
			if let Some(focus) = focus {
				println!("- requested_focus: `{}`", focus);
			}
			println!();
			println!("## Why");
			println!();
			println!("This issue was generated by `oo-bot` from the local organism state, active goals, and recent journal events.");
			println!();
			println!("## Signals");
			for goal in top_goals {
				println!("- active goal: `{}` — {} (prio {}, safety {})", goal.goal_id, goal.title, goal.priority, goal.safety_class);
			}
			for ev in snapshot.recent_events.iter().rev().take(5).rev() {
				println!("- event: `{}` [{}] {}", ev.kind, ev.severity, ev.summary);
			}
			println!();
			println!("## Suggested actions");
			println!();
			for action in actions {
				println!("- [ ] {}", action);
			}
		}
		OutputFormat::Text => {
			println!("ISSUE: {}", title);
			println!("continuity={} mode={} policy={}", continuity, snapshot.state.mode.as_str(), snapshot.state.policy.enforcement.as_str());
			if let Some(focus) = focus {
				println!("focus={}", focus);
			}
			for action in actions {
				println!("- {}", action);
			}
		}
	}
}

fn print_github_pr(
	snapshot: &Snapshot,
	title: &str,
	head: Option<&str>,
	base: &str,
	focus: Option<&str>,
	format: OutputFormat,
) {
	let continuity = continuity_summary(snapshot);
	let actions = recommend_actions(snapshot);
	let active_goals = active_goals(&snapshot.state);
	let top_goals = top_goals(&active_goals, 3);

	match format {
		OutputFormat::Markdown => {
			println!("# {}", title);
			println!();
			println!("## Summary");
			println!();
			println!("- generated by `oo-bot` from current host organism state");
			println!("- continuity assessment at generation time: `{}`", continuity);
			println!("- target base branch: `{}`", base);
			if let Some(head) = head {
				println!("- source head branch: `{}`", head);
			}
			if let Some(focus) = focus {
				println!("- requested focus: `{}`", focus);
			}
			println!();
			println!("## Organism context");
			println!();
			println!("- organism: `{}`", snapshot.identity.organism_id);
			println!("- habitat: `{}`", snapshot.identity.runtime_habitat);
			println!("- mode: `{}`", snapshot.state.mode.as_str());
			println!("- policy: `{}`", snapshot.state.policy.enforcement.as_str());
			println!("- continuity_epoch: `{}`", snapshot.state.continuity_epoch);
			println!();
			println!("## Top goals informing this PR");
			println!();
			for goal in top_goals {
				println!("- `{}` — {} (prio {}, safety {})", goal.goal_id, goal.title, goal.priority, goal.safety_class);
			}
			println!();
			println!("## Recent events");
			println!();
			for ev in snapshot.recent_events.iter().rev().take(5).rev() {
				println!("- `{}` [{}] {}", ev.kind, ev.severity, ev.summary);
			}
			println!();
			println!("## Suggested validation / follow-up");
			println!();
			for action in actions {
				println!("- {}", action);
			}
		}
		OutputFormat::Text => {
			println!("PR: {}", title);
			println!("base={} head={}", base, head.unwrap_or("(unspecified)"));
			println!("continuity={} mode={} policy={}", continuity, snapshot.state.mode.as_str(), snapshot.state.policy.enforcement.as_str());
			if let Some(focus) = focus {
				println!("focus={}", focus);
			}
			for action in actions {
				println!("- {}", action);
			}
		}
	}
}

fn build_protection_manifest(
	snapshot: &Snapshot,
	workspace: &Path,
) -> Result<ProtectionManifest, Box<dyn std::error::Error>> {
	let workspace_root = workspace.canonicalize()?;
	let mut files = Vec::new();
	collect_workspace_files(&workspace_root, &workspace_root, &mut files)?;
	files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

	Ok(ProtectionManifest {
		schema_version: 1,
		manifest_kind: "oo_code_protection_manifest".to_string(),
		generated_at_epoch_s: now_epoch_s(),
		organism_id: snapshot.identity.organism_id.clone(),
		runtime_habitat: snapshot.identity.runtime_habitat.clone(),
		workspace_root: workspace_root.display().to_string(),
		file_count: files.len(),
		entries: files,
	})
}

fn print_protection_status(
	snapshot: &Snapshot,
	workspace: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	let workspace_root = workspace.canonicalize()?;
	let manifest = build_protection_manifest(snapshot, &workspace_root)?;
	let license_present = workspace_root.join("LICENSE").exists() || workspace_root.join("LICENSE.md").exists();
	let readme_present = workspace_root.join("README.md").exists();

	println!("oo-bot protection status");
	println!("workspace           : {}", workspace_root.display());
	println!("organism_id         : {}", snapshot.identity.organism_id);
	println!("files_hashed        : {}", manifest.file_count);
	println!("license_present     : {}", license_present);
	println!("readme_present      : {}", readme_present);
	println!("continuity_context  : {}", continuity_summary(snapshot));
	println!("protection_scope    : provenance + hashing + manifest evidence");
	println!("recommendations:");
	println!("- regenerate the protection manifest after each validated merge or release");
	println!("- keep repository license, authorship, and signed git history aligned");
	println!("- store release manifests outside the repo as timestamped evidence");
	println!("- enable GitHub branch protection and signed tags for official releases");
	Ok(())
}

fn print_protection_verify(
	snapshot: &Snapshot,
	workspace: &Path,
	manifest_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	let saved_manifest: ProtectionManifest = read_json(manifest_path)?;
	let current_manifest = build_protection_manifest(snapshot, workspace)?;
	let diff = diff_manifests(&saved_manifest, &current_manifest);
	let ok = diff.added.is_empty() && diff.changed.is_empty() && diff.removed.is_empty();

	println!("oo-bot protection verify");
	println!("workspace           : {}", current_manifest.workspace_root);
	println!("manifest            : {}", manifest_path.display());
	println!("saved_file_count    : {}", saved_manifest.file_count);
	println!("current_file_count  : {}", current_manifest.file_count);
	println!("added_files         : {}", diff.added.len());
	println!("changed_files       : {}", diff.changed.len());
	println!("removed_files       : {}", diff.removed.len());
	println!("status              : {}", if ok { "match" } else { "drift_detected" });

	if !diff.added.is_empty() {
		println!("added:");
		for path in diff.added.iter().take(10) {
			println!("- {}", path);
		}
	}
	if !diff.changed.is_empty() {
		println!("changed:");
		for path in diff.changed.iter().take(10) {
			println!("- {}", path);
		}
	}
	if !diff.removed.is_empty() {
		println!("removed:");
		for path in diff.removed.iter().take(10) {
			println!("- {}", path);
		}
	}

	Ok(())
}

fn print_sovereign_status(
	snapshot: &Snapshot,
	paths: &AppPaths,
	workspace: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	let workspace_root = workspace.canonicalize()?;
	let data_root = paths.identity_path.parent().unwrap_or(Path::new("data"));
	let host_root = data_root
		.canonicalize()
		.ok()
		.and_then(|p| p.parent().map(Path::to_path_buf))
		.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
	let handoff_script = workspace_root.join("test-qemu-handoff.ps1");
	let handoff_autorun = workspace_root.join("llmk-autorun-handoff-smoke.txt");
	let sovereign_target = workspace_root.join("sovereign_export.json");
	let readme_present = workspace_root.join("README.md").exists();
	let license_present = workspace_root.join("LICENSE").exists() || workspace_root.join("LICENSE.md").exists();
	let workspace_git = workspace_root.join(".git").exists();
	let host_export_present = paths.sovereign_export_path.exists();
	let manifest_path = data_root.join("code_protection_manifest.json");
	let attestation_path = data_root.join("code_protection_attestation.json");
	let layout = layout_relationship(&host_root, &workspace_root);

	println!("oo-bot sovereign status");
	println!("workspace             : {}", workspace_root.display());
	println!("host_root             : {}", host_root.display());
	println!("layout                : {}", layout);
	println!("workspace_git         : {}", workspace_git);
	println!("handoff_script        : {}", present_absent(handoff_script.exists()));
	println!("handoff_autorun       : {}", present_absent(handoff_autorun.exists()));
	println!("readme_present        : {}", readme_present);
	println!("license_present       : {}", license_present);
	println!("host_export           : {}", present_absent(host_export_present));
	println!("sovereign_target      : {}", present_absent(sovereign_target.exists()));
	println!("protection_manifest   : {}", present_absent(manifest_path.exists()));
	println!("protection_attestation: {}", present_absent(attestation_path.exists()));
	println!("continuity_context    : {}", continuity_summary(snapshot));
	println!("recommendations:");
	for action in recommend_sovereign_actions(snapshot, &workspace_root, paths, &host_root) {
		println!("- {}", action);
	}

	Ok(())
}

fn print_handoff_check(
	snapshot: &Snapshot,
	workspace: &Path,
	export_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	let workspace_root = workspace.canonicalize()?;
	let export_json: serde_json::Value = read_json(export_path)?;
	let issues = validate_handoff_export(&export_json);
	let smoke_commands = read_smoke_commands(&workspace_root.join("llmk-autorun-handoff-smoke.txt"))?;
	let smoke_missing = missing_smoke_commands(&smoke_commands);
	let mode = export_json
		.get("mode")
		.and_then(serde_json::Value::as_str)
		.unwrap_or("<missing>");
	let export_kind = export_json
		.get("export_kind")
		.and_then(serde_json::Value::as_str)
		.unwrap_or("<missing>");
	let schema_version = export_json
		.get("schema_version")
		.and_then(serde_json::Value::as_u64)
		.map(|v| v.to_string())
		.unwrap_or_else(|| "<missing>".to_string());
	let top_goal_count = export_json
		.get("top_goals")
		.and_then(serde_json::Value::as_array)
		.map(|items| items.len())
		.unwrap_or(0);
	let recent_event_count = export_json
		.get("recent_events")
		.and_then(serde_json::Value::as_array)
		.map(|items| items.len())
		.unwrap_or(0);

	println!("oo-bot handoff check");
	println!("workspace             : {}", workspace_root.display());
	println!("export_path           : {}", export_path.display());
	println!("export_kind           : {}", export_kind);
	println!("schema_version        : {}", schema_version);
	println!("mode                  : {}", mode);
	println!("top_goals             : {}", top_goal_count);
	println!("recent_events         : {}", recent_event_count);
	println!("continuity_context    : {}", continuity_summary(snapshot));
	println!("export_validation     : {}", if issues.is_empty() { "ok" } else { "failed" });
	println!("smoke_script_commands : {}", if smoke_missing.is_empty() { "ok" } else { "missing_entries" });

	if !issues.is_empty() {
		println!("export_issues:");
		for issue in &issues {
			println!("- {}", issue);
		}
	}

	if !smoke_missing.is_empty() {
		println!("smoke_missing:");
		for cmd in &smoke_missing {
			println!("- {}", cmd);
		}
	}

	println!("recommendations:");
	for item in recommend_handoff_actions(snapshot, &issues, &smoke_missing) {
		println!("- {}", item);
	}

	Ok(())
}

fn print_sovereign_brief(
	snapshot: &Snapshot,
	paths: &AppPaths,
	workspace: &Path,
	export_path: &Path,
	format: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
	let data = build_sovereign_brief_data(snapshot, paths, workspace, export_path)?;

	match format {
		OutputFormat::Markdown => {
			println!("## Sovereign Integration Brief");
			println!();
			println!("- workspace: `{}`", data.workspace_root.display());
			println!("- layout: `{}`", data.layout);
			println!("- continuity: `{}`", data.continuity);
			println!("- export_validation: `{}`", if data.export_validation_ok { "ok" } else { "failed" });
			println!("- smoke_script: `{}`", if data.smoke_ok { "ok" } else { "missing_entries" });
			println!("- mode: `{}`", data.mode);
			println!("- policy_enforcement: `{}`", data.policy);
			println!("- protection_manifest: `{}`", present_absent(data.manifest_present));
			println!("- protection_attestation: `{}`", present_absent(data.attestation_present));
			println!();
			if !data.issues.is_empty() {
				println!("### Export issues");
				for issue in &data.issues {
					println!("- {}", issue);
				}
				println!();
			}
			if !data.smoke_missing.is_empty() {
				println!("### Missing smoke commands");
				for cmd in &data.smoke_missing {
					println!("- `{}`", cmd);
				}
				println!();
			}
			println!("### Suggested next actions");
			for action in data.actions {
				println!("- {}", action);
			}
		}
		OutputFormat::Text => {
			println!("SOVEREIGN BRIEF");
			println!("workspace={}", data.workspace_root.display());
			println!("layout={} continuity={}", data.layout, data.continuity);
			println!("export_validation={} smoke_script={}", if data.export_validation_ok { "ok" } else { "failed" }, if data.smoke_ok { "ok" } else { "missing_entries" });
			println!("mode={} policy={}", data.mode, data.policy);
			println!("protection_manifest={} protection_attestation={}", present_absent(data.manifest_present), present_absent(data.attestation_present));
			for action in data.actions {
				println!("- {}", action);
			}
		}
	}

	Ok(())
}

fn print_github_sovereign_brief(
	snapshot: &Snapshot,
	paths: &AppPaths,
	workspace: &Path,
	export_path: &Path,
	format: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
	let data = build_sovereign_brief_data(snapshot, paths, workspace, export_path)?;
	print!("{}", render_github_sovereign_brief(&data, format));

	Ok(())
}

fn print_github_sovereign_issue(
	snapshot: &Snapshot,
	paths: &AppPaths,
	title: &str,
	workspace: &Path,
	export_path: &Path,
	focus: Option<&str>,
	format: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
	let data = build_sovereign_brief_data(snapshot, paths, workspace, export_path)?;
	print!("{}", render_github_sovereign_issue(&data, title, focus, format));

	Ok(())
}

fn print_github_sovereign_pr(
	snapshot: &Snapshot,
	paths: &AppPaths,
	title: &str,
	workspace: &Path,
	export_path: &Path,
	head: Option<&str>,
	base: &str,
	focus: Option<&str>,
	format: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
	let data = build_sovereign_brief_data(snapshot, paths, workspace, export_path)?;
	print!("{}", render_github_sovereign_pr(&data, title, head, base, focus, format));

	Ok(())
}

fn write_github_sovereign_pack(
	snapshot: &Snapshot,
	paths: &AppPaths,
	workspace: &Path,
	export_path: &Path,
	issue_title: &str,
	pr_title: &str,
	head: Option<&str>,
	base: &str,
	focus: Option<&str>,
	out_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	let data = build_sovereign_brief_data(snapshot, paths, workspace, export_path)?;
	fs::create_dir_all(out_dir)?;
	let brief_path = out_dir.join("sovereign-brief.md");
	let issue_path = out_dir.join("sovereign-issue.md");
	let pr_path = out_dir.join("sovereign-pr.md");

	write_text_file(&brief_path, &render_github_sovereign_brief(&data, OutputFormat::Markdown))?;
	write_text_file(&issue_path, &render_github_sovereign_issue(&data, issue_title, focus, OutputFormat::Markdown))?;
	write_text_file(&pr_path, &render_github_sovereign_pr(&data, pr_title, head, base, focus, OutputFormat::Markdown))?;

	println!("OK: GitHub sovereign pack written to {}", out_dir.display());
	println!("- {}", brief_path.display());
	println!("- {}", issue_path.display());
	println!("- {}", pr_path.display());
	Ok(())
}

fn print_receipt_check(
	snapshot: &Snapshot,
	workspace: &Path,
	receipt_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	let workspace_root = workspace.canonicalize()?;
	if !receipt_path.exists() {
		println!("oo-bot receipt check");
		println!("workspace             : {}", workspace_root.display());
		println!("receipt_path          : {}", receipt_path.display());
		println!("receipt_status        : absent");
		println!("continuity_context    : {}", continuity_summary(snapshot));
		println!("recommendations:");
		println!("- Run the sovereign handoff apply path so `OOHANDOFF.TXT` is produced before comparing receipt state.");
		println!("- Keep the host sovereign export current before the next handoff validation cycle.");
		return Ok(());
	}

	let receipt = read_handoff_receipt(receipt_path)?;
	let host_mode = snapshot.state.mode.as_str();
	let host_policy = snapshot.state.policy.enforcement.as_str();
	let host_epoch = snapshot.state.continuity_epoch;
	let host_recovery = snapshot.state.last_recovery_reason.as_deref();
	let receipt_recovery = normalize_optional_text(receipt.last_recovery_reason.as_deref());
	let organism_match = receipt.organism_id == snapshot.identity.organism_id;
	let mode_match = receipt.mode == host_mode;
	let policy_match = receipt.policy_enforcement == host_policy;
	let recovery_match = normalize_optional_text(host_recovery) == receipt_recovery;
	let continuity_relation = compare_epochs(host_epoch, receipt.continuity_epoch);

	println!("oo-bot receipt check");
	println!("workspace             : {}", workspace_root.display());
	println!("receipt_path          : {}", receipt_path.display());
	println!("organism_match        : {}", organism_match);
	println!("host_organism_id      : {}", snapshot.identity.organism_id);
	println!("receipt_organism_id   : {}", receipt.organism_id);
	println!("mode_match            : {}", mode_match);
	println!("host_mode             : {}", host_mode);
	println!("receipt_mode          : {}", receipt.mode);
	println!("policy_match          : {}", policy_match);
	println!("host_policy           : {}", host_policy);
	println!("receipt_policy        : {}", receipt.policy_enforcement);
	println!("continuity_relation   : {}", continuity_relation);
	println!("host_continuity_epoch : {}", host_epoch);
	println!("receipt_continuity    : {}", receipt.continuity_epoch);
	println!("recovery_match        : {}", recovery_match);
	println!("host_recovery         : {}", host_recovery.unwrap_or("none"));
	println!("receipt_recovery      : {}", receipt_recovery.unwrap_or("none"));
	println!("continuity_context    : {}", continuity_summary(snapshot));
	println!("recommendations:");
	for item in recommend_receipt_actions(
		organism_match,
		mode_match,
		policy_match,
		continuity_relation,
		recovery_match,
	) {
		println!("- {}", item);
	}

	Ok(())
}

fn print_sync_check(
	snapshot: &Snapshot,
	workspace: &Path,
	export_path: &Path,
	receipt_path: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
	let workspace_root = workspace.canonicalize()?;
	let export_present = export_path.exists();
	let receipt_present = receipt_path.exists();
	let export_summary = if export_present {
		Some(read_handoff_export_summary(export_path)?)
	} else {
		None
	};
	let receipt = if receipt_present {
		Some(read_handoff_receipt(receipt_path)?)
	} else {
		None
	};

	let host_mode = snapshot.state.mode.as_str();
	let host_policy = snapshot.state.policy.enforcement.as_str();
	let host_epoch = snapshot.state.continuity_epoch;
	let host_recovery = normalize_optional_text(snapshot.state.last_recovery_reason.as_deref());
	let host_organism = snapshot.identity.organism_id.as_str();

	let mut problems = Vec::new();
	let mut warnings = Vec::new();
	let verdict = if !export_present || !receipt_present {
		if !export_present {
			problems.push(format!("missing export `{}`", export_path.display()));
		}
		if !receipt_present {
			problems.push(format!("missing receipt `{}`", receipt_path.display()));
		}
		"missing_artifact"
	} else {
		let export = export_summary.as_ref().expect("export summary present");
		let receipt = receipt.as_ref().expect("receipt present");

		if !export.issues.is_empty() {
			for issue in &export.issues {
				problems.push(format!("export: {}", issue));
			}
		}
		if export.organism_id != host_organism {
			problems.push("export organism_id does not match host organism_id".to_string());
		}
		if receipt.organism_id != host_organism {
			problems.push("receipt organism_id does not match host organism_id".to_string());
		}

		if export.mode != host_mode {
			warnings.push(format!("export mode `{}` differs from host mode `{}`", export.mode, host_mode));
		}
		if export.policy_enforcement != host_policy {
			warnings.push(format!("export policy `{}` differs from host policy `{}`", export.policy_enforcement, host_policy));
		}
		if export.continuity_epoch != host_epoch {
			warnings.push(format!("export continuity `{}` differs from host continuity `{}`", export.continuity_epoch, host_epoch));
		}

		if receipt.mode != host_mode {
			let relation = compare_mode_relation(host_mode, &receipt.mode);
			warnings.push(format!("receipt mode `{}` differs from host mode `{}` ({})", receipt.mode, host_mode, relation));
		}
		if receipt.policy_enforcement != host_policy {
			let relation = compare_policy_relation(host_policy, &receipt.policy_enforcement);
			warnings.push(format!("receipt policy `{}` differs from host policy `{}` ({})", receipt.policy_enforcement, host_policy, relation));
		}
		if receipt.continuity_epoch != host_epoch {
			warnings.push(format!("receipt continuity `{}` differs from host continuity `{}` ({})", receipt.continuity_epoch, host_epoch, compare_epochs(host_epoch, receipt.continuity_epoch)));
		}

		if normalize_optional_text(export.last_recovery_reason.as_deref()) != host_recovery {
			warnings.push("export last_recovery_reason differs from host state".to_string());
		}
		if normalize_optional_text(receipt.last_recovery_reason.as_deref()) != host_recovery {
			warnings.push("receipt last_recovery_reason differs from host state".to_string());
		}

		if !problems.is_empty() {
			"unsafe_mismatch"
		} else if warnings.is_empty() {
			"aligned"
		} else {
			"drift"
		}
	};

	println!("oo-bot sync check");
	println!("workspace             : {}", workspace_root.display());
	println!("export_path           : {}", export_path.display());
	println!("receipt_path          : {}", receipt_path.display());
	println!("export_present        : {}", export_present);
	println!("receipt_present       : {}", receipt_present);
	println!("continuity_context    : {}", continuity_summary(snapshot));
	println!("verdict               : {}", verdict);

	if let Some(export) = &export_summary {
		println!("host_export_epoch     : {}", export.continuity_epoch);
		println!("host_export_mode      : {}", export.mode);
		println!("host_export_policy    : {}", export.policy_enforcement);
	}
	if let Some(receipt) = &receipt {
		println!("receipt_epoch         : {}", receipt.continuity_epoch);
		println!("receipt_mode          : {}", receipt.mode);
		println!("receipt_policy        : {}", receipt.policy_enforcement);
	}
	println!("host_epoch            : {}", host_epoch);
	println!("host_mode             : {}", host_mode);
	println!("host_policy           : {}", host_policy);

	if !problems.is_empty() {
		println!("problems:");
		for item in &problems {
			println!("- {}", item);
		}
	}
	if !warnings.is_empty() {
		println!("warnings:");
		for item in &warnings {
			println!("- {}", item);
		}
	}

	println!("recommendations:");
	for item in recommend_sync_actions(verdict, &problems, &warnings) {
		println!("- {}", item);
	}

	Ok(verdict == "aligned")
}

fn read_handoff_export_summary(path: &Path) -> Result<HandoffExportSummary, Box<dyn std::error::Error>> {
	let export_json: serde_json::Value = read_json(path)?;
	let issues = validate_handoff_export(&export_json);
	let organism_id = export_json
		.get("organism_id")
		.and_then(serde_json::Value::as_str)
		.unwrap_or("")
		.to_string();
	let mode = export_json
		.get("mode")
		.and_then(serde_json::Value::as_str)
		.unwrap_or("")
		.to_string();
	let policy_enforcement = export_json
		.get("policy")
		.and_then(|v| v.get("enforcement"))
		.and_then(serde_json::Value::as_str)
		.unwrap_or("")
		.to_string();
	let continuity_epoch = export_json
		.get("continuity_epoch")
		.and_then(serde_json::Value::as_u64)
		.unwrap_or(0);
	let last_recovery_reason = export_json
		.get("last_recovery_reason")
		.and_then(serde_json::Value::as_str)
		.and_then(normalize_optional_owned);

	Ok(HandoffExportSummary {
		organism_id,
		mode,
		policy_enforcement,
		continuity_epoch,
		last_recovery_reason,
		issues,
	})
}

fn read_handoff_receipt(path: &Path) -> Result<HandoffReceipt, Box<dyn std::error::Error>> {
	let file = File::open(path)?;
	let reader = BufReader::new(file);
	let mut organism_id = None;
	let mut mode = None;
	let mut policy_enforcement = None;
	let mut continuity_epoch = None;
	let mut last_recovery_reason = None;

	for line in reader.lines() {
		let line = line?;
		let trimmed = line.trim();
		if trimmed.is_empty() {
			continue;
		}
		let Some((key, value)) = trimmed.split_once('=') else {
			continue;
		};
		match key.trim() {
			"organism_id" => organism_id = Some(value.trim().to_string()),
			"mode" => mode = Some(value.trim().to_string()),
			"policy_enforcement" => policy_enforcement = Some(value.trim().to_string()),
			"continuity_epoch" => continuity_epoch = value.trim().parse::<u64>().ok(),
			"last_recovery_reason" => {
				last_recovery_reason = normalize_optional_owned(value.trim());
			}
			_ => {}
		}
	}

	Ok(HandoffReceipt {
		organism_id: organism_id.ok_or("missing organism_id in handoff receipt")?,
		mode: mode.ok_or("missing mode in handoff receipt")?,
		policy_enforcement: policy_enforcement.ok_or("missing policy_enforcement in handoff receipt")?,
		continuity_epoch: continuity_epoch.ok_or("missing continuity_epoch in handoff receipt")?,
		last_recovery_reason,
	})
}

fn normalize_optional_owned(value: &str) -> Option<String> {
	let trimmed = value.trim();
	if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") || trimmed.eq_ignore_ascii_case("null") {
		None
	} else {
		Some(trimmed.to_string())
	}
}

fn normalize_optional_text(value: Option<&str>) -> Option<&str> {
	match value {
		Some(text) if !text.trim().is_empty() && !text.eq_ignore_ascii_case("none") && !text.eq_ignore_ascii_case("null") => Some(text),
		_ => None,
	}
}

fn compare_epochs(host_epoch: u64, receipt_epoch: u64) -> &'static str {
	if host_epoch == receipt_epoch {
		"aligned"
	} else if host_epoch > receipt_epoch {
		"host_ahead"
	} else {
		"sovereign_ahead"
	}
}

fn compare_mode_relation(host_mode: &str, receipt_mode: &str) -> &'static str {
	match export_mode_rank(receipt_mode).cmp(&export_mode_rank(host_mode)) {
		std::cmp::Ordering::Greater => "receipt_stricter",
		std::cmp::Ordering::Less => "host_stricter",
		std::cmp::Ordering::Equal => "aligned",
	}
}

fn compare_policy_relation(host_policy: &str, receipt_policy: &str) -> &'static str {
	match policy_rank(receipt_policy).cmp(&policy_rank(host_policy)) {
		std::cmp::Ordering::Greater => "receipt_stricter",
		std::cmp::Ordering::Less => "host_stricter",
		std::cmp::Ordering::Equal => "aligned",
	}
}

fn policy_rank(policy: &str) -> u8 {
	match policy {
		"off" => 0,
		"observe" => 1,
		"enforce" => 2,
		_ => 255,
	}
}

fn recommend_receipt_actions(
	organism_match: bool,
	mode_match: bool,
	policy_match: bool,
	continuity_relation: &str,
	recovery_match: bool,
) -> Vec<String> {
	let mut out = Vec::new();

	if !organism_match {
		out.push("Stop handoff use until host and sovereign organism identifiers match again.".to_string());
	}
	if !mode_match {
		out.push("Review whether host mode or sovereign receipt mode changed after the last handoff application.".to_string());
	}
	if !policy_match {
		out.push("Reconcile host policy enforcement with the sovereign receipt before the next validation run.".to_string());
	}
	match continuity_relation {
		"host_ahead" => out.push("Apply or replay the latest host handoff so the sovereign receipt catches up.".to_string()),
		"sovereign_ahead" => out.push("Inspect whether sovereign runtime advanced continuity beyond the last exported host state.".to_string()),
		_ => {}
	}
	if !recovery_match {
		out.push("Capture the recovery-reason divergence in the journal before the next handoff cycle.".to_string());
	}
	if out.is_empty() {
		out.push("Receipt and host state are aligned; proceed with the next sovereign validation or operator update.".to_string());
	}

	out.truncate(5);
	out
}

fn recommend_sync_actions(verdict: &str, problems: &[String], warnings: &[String]) -> Vec<String> {
	let mut out = Vec::new();

	match verdict {
		"missing_artifact" => {
			out.push("Generate the missing handoff artifact before relying on continuity synchronization.".to_string());
		}
		"unsafe_mismatch" => {
			out.push("Stop the handoff loop and repair organism or export contract mismatches first.".to_string());
		}
		"drift" => {
			out.push("Regenerate or reapply handoff state so host, export, and receipt converge again.".to_string());
		}
		_ => {
			out.push("Host state, export, and receipt are aligned; proceed with the next validation step.".to_string());
		}
	}

	if problems.iter().any(|p| p.contains("organism_id")) {
		out.push("Verify both runtimes still belong to the same organism before applying any further handoff.".to_string());
	}
	if warnings.iter().any(|w| w.contains("continuity")) {
		out.push("Run a fresh export and sovereign apply cycle if continuity drift should be closed now.".to_string());
	}
	if warnings.iter().any(|w| w.contains("mode") || w.contains("policy")) {
		out.push("Review whether the sovereign side intentionally kept a stricter safety posture than the host.".to_string());
	}

	out.truncate(5);
	out
}

fn render_github_sovereign_brief(data: &SovereignBriefData, format: OutputFormat) -> String {
	let mut out = String::new();
	match format {
		OutputFormat::Markdown => {
			let _ = writeln!(out, "## Sovereign GitHub Brief\n");
			let _ = writeln!(out, "- workspace: `{}`", data.workspace_root.display());
			let _ = writeln!(out, "- layout: `{}`", data.layout);
			let _ = writeln!(out, "- continuity: `{}`", data.continuity);
			let _ = writeln!(out, "- export_validation: `{}`", if data.export_validation_ok { "ok" } else { "failed" });
			let _ = writeln!(out, "- smoke_script: `{}`", if data.smoke_ok { "ok" } else { "missing_entries" });
			let _ = writeln!(out, "- mode: `{}`", data.mode);
			let _ = writeln!(out, "- policy_enforcement: `{}`\n", data.policy);
			let _ = writeln!(out, "### Readiness signals");
			let _ = writeln!(out, "- protection_manifest: `{}`", present_absent(data.manifest_present));
			let _ = writeln!(out, "- protection_attestation: `{}`", present_absent(data.attestation_present));
			let _ = writeln!(out, "- sovereign handoff contract: `{}`\n", if data.issues.is_empty() && data.smoke_missing.is_empty() { "ready" } else { "attention_required" });
			if !data.issues.is_empty() {
				let _ = writeln!(out, "### Contract issues");
				for issue in &data.issues {
					let _ = writeln!(out, "- {}", issue);
				}
				out.push('\n');
			}
			if !data.smoke_missing.is_empty() {
				let _ = writeln!(out, "### Missing smoke commands");
				for cmd in &data.smoke_missing {
					let _ = writeln!(out, "- `{}`", cmd);
				}
				out.push('\n');
			}
			let _ = writeln!(out, "### Suggested next actions");
			for action in &data.actions {
				let _ = writeln!(out, "- [ ] {}", action);
			}
		}
		OutputFormat::Text => {
			let _ = writeln!(out, "GITHUB SOVEREIGN BRIEF");
			let _ = writeln!(out, "workspace={}", data.workspace_root.display());
			let _ = writeln!(out, "layout={} continuity={}", data.layout, data.continuity);
			let _ = writeln!(out, "export_validation={} smoke_script={}", if data.export_validation_ok { "ok" } else { "failed" }, if data.smoke_ok { "ok" } else { "missing_entries" });
			let _ = writeln!(out, "mode={} policy={}", data.mode, data.policy);
			let _ = writeln!(out, "manifest={} attestation={}", present_absent(data.manifest_present), present_absent(data.attestation_present));
			for action in &data.actions {
				let _ = writeln!(out, "- {}", action);
			}
		}
	}
	out
}

fn render_github_sovereign_issue(
	data: &SovereignBriefData,
	title: &str,
	focus: Option<&str>,
	format: OutputFormat,
) -> String {
	let mut out = String::new();
	match format {
		OutputFormat::Markdown => {
			let _ = writeln!(out, "# {}\n", title);
			let _ = writeln!(out, "## Context\n");
			let _ = writeln!(out, "- workspace: `{}`", data.workspace_root.display());
			let _ = writeln!(out, "- layout: `{}`", data.layout);
			let _ = writeln!(out, "- continuity: `{}`", data.continuity);
			let _ = writeln!(out, "- mode: `{}`", data.mode);
			let _ = writeln!(out, "- policy_enforcement: `{}`", data.policy);
			let _ = writeln!(out, "- export_validation: `{}`", if data.export_validation_ok { "ok" } else { "failed" });
			let _ = writeln!(out, "- smoke_script: `{}`", if data.smoke_ok { "ok" } else { "missing_entries" });
			if let Some(focus) = focus {
				let _ = writeln!(out, "- requested_focus: `{}`", focus);
			}
			out.push('\n');
			let _ = writeln!(out, "## Readiness signals\n");
			let _ = writeln!(out, "- protection_manifest: `{}`", present_absent(data.manifest_present));
			let _ = writeln!(out, "- protection_attestation: `{}`", present_absent(data.attestation_present));
			let _ = writeln!(out, "- sovereign handoff contract: `{}`\n", if data.issues.is_empty() && data.smoke_missing.is_empty() { "ready" } else { "attention_required" });
			if !data.issues.is_empty() {
				let _ = writeln!(out, "## Contract issues\n");
				for issue in &data.issues {
					let _ = writeln!(out, "- {}", issue);
				}
				out.push('\n');
			}
			if !data.smoke_missing.is_empty() {
				let _ = writeln!(out, "## Missing smoke commands\n");
				for cmd in &data.smoke_missing {
					let _ = writeln!(out, "- `{}`", cmd);
				}
				out.push('\n');
			}
			let _ = writeln!(out, "## Suggested actions\n");
			for action in &data.actions {
				let _ = writeln!(out, "- [ ] {}", action);
			}
		}
		OutputFormat::Text => {
			let _ = writeln!(out, "ISSUE: {}", title);
			let _ = writeln!(out, "workspace={}", data.workspace_root.display());
			let _ = writeln!(out, "layout={} continuity={}", data.layout, data.continuity);
			let _ = writeln!(out, "mode={} policy={}", data.mode, data.policy);
			let _ = writeln!(out, "export_validation={} smoke_script={}", if data.export_validation_ok { "ok" } else { "failed" }, if data.smoke_ok { "ok" } else { "missing_entries" });
			if let Some(focus) = focus {
				let _ = writeln!(out, "focus={}", focus);
			}
			for action in &data.actions {
				let _ = writeln!(out, "- {}", action);
			}
		}
	}
	out
}

fn render_github_sovereign_pr(
	data: &SovereignBriefData,
	title: &str,
	head: Option<&str>,
	base: &str,
	focus: Option<&str>,
	format: OutputFormat,
) -> String {
	let mut out = String::new();
	match format {
		OutputFormat::Markdown => {
			let _ = writeln!(out, "# {}\n", title);
			let _ = writeln!(out, "## Summary\n");
			let _ = writeln!(out, "- generated by `oo-bot` from sovereign integration state");
			let _ = writeln!(out, "- target base branch: `{}`", base);
			if let Some(head) = head {
				let _ = writeln!(out, "- source head branch: `{}`", head);
			}
			if let Some(focus) = focus {
				let _ = writeln!(out, "- requested focus: `{}`", focus);
			}
			let _ = writeln!(out, "- layout: `{}`", data.layout);
			let _ = writeln!(out, "- continuity: `{}`", data.continuity);
			let _ = writeln!(out, "- export_validation: `{}`", if data.export_validation_ok { "ok" } else { "failed" });
			let _ = writeln!(out, "- smoke_script: `{}`\n", if data.smoke_ok { "ok" } else { "missing_entries" });
			let _ = writeln!(out, "## Sovereign readiness\n");
			let _ = writeln!(out, "- workspace: `{}`", data.workspace_root.display());
			let _ = writeln!(out, "- mode: `{}`", data.mode);
			let _ = writeln!(out, "- policy_enforcement: `{}`", data.policy);
			let _ = writeln!(out, "- protection_manifest: `{}`", present_absent(data.manifest_present));
			let _ = writeln!(out, "- protection_attestation: `{}`", present_absent(data.attestation_present));
			let _ = writeln!(out, "- sovereign handoff contract: `{}`\n", if data.issues.is_empty() && data.smoke_missing.is_empty() { "ready" } else { "attention_required" });
			if !data.issues.is_empty() {
				let _ = writeln!(out, "## Contract issues\n");
				for issue in &data.issues {
					let _ = writeln!(out, "- {}", issue);
				}
				out.push('\n');
			}
			if !data.smoke_missing.is_empty() {
				let _ = writeln!(out, "## Missing smoke commands\n");
				for cmd in &data.smoke_missing {
					let _ = writeln!(out, "- `{}`", cmd);
				}
				out.push('\n');
			}
			let _ = writeln!(out, "## Suggested validation / follow-up\n");
			for action in &data.actions {
				let _ = writeln!(out, "- {}", action);
			}
		}
		OutputFormat::Text => {
			let _ = writeln!(out, "PR: {}", title);
			let _ = writeln!(out, "base={} head={}", base, head.unwrap_or("(unspecified)"));
			let _ = writeln!(out, "layout={} continuity={}", data.layout, data.continuity);
			let _ = writeln!(out, "mode={} policy={}", data.mode, data.policy);
			let _ = writeln!(out, "export_validation={} smoke_script={}", if data.export_validation_ok { "ok" } else { "failed" }, if data.smoke_ok { "ok" } else { "missing_entries" });
			if let Some(focus) = focus {
				let _ = writeln!(out, "focus={}", focus);
			}
			for action in &data.actions {
				let _ = writeln!(out, "- {}", action);
			}
		}
	}
	out
}

fn build_sovereign_brief_data(
	snapshot: &Snapshot,
	paths: &AppPaths,
	workspace: &Path,
	export_path: &Path,
) -> Result<SovereignBriefData, Box<dyn std::error::Error>> {
	let workspace_root = workspace.canonicalize()?;
	let export_json: serde_json::Value = read_json(export_path)?;
	let issues = validate_handoff_export(&export_json);
	let smoke_commands = read_smoke_commands(&workspace_root.join("llmk-autorun-handoff-smoke.txt"))?;
	let smoke_missing = missing_smoke_commands(&smoke_commands);
	let host_root = paths
		.identity_path
		.parent()
		.unwrap_or(Path::new("data"))
		.canonicalize()
		.ok()
		.and_then(|p| p.parent().map(Path::to_path_buf))
		.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
	let layout = layout_relationship(&host_root, &workspace_root);
	let manifest_path = paths
		.identity_path
		.parent()
		.unwrap_or(Path::new("data"))
		.join("code_protection_manifest.json");
	let attestation_path = paths
		.identity_path
		.parent()
		.unwrap_or(Path::new("data"))
		.join("code_protection_attestation.json");
	let mode = export_json
		.get("mode")
		.and_then(serde_json::Value::as_str)
		.unwrap_or("<missing>")
		.to_string();
	let policy = export_json
		.get("policy")
		.and_then(|v| v.get("enforcement"))
		.and_then(serde_json::Value::as_str)
		.unwrap_or("<missing>")
		.to_string();
	let actions = recommend_handoff_actions(snapshot, &issues, &smoke_missing);

	Ok(SovereignBriefData {
		workspace_root,
		layout,
		continuity: continuity_summary(snapshot),
		export_validation_ok: issues.is_empty(),
		smoke_ok: smoke_missing.is_empty(),
		mode,
		policy,
		manifest_present: manifest_path.exists(),
		attestation_present: attestation_path.exists(),
		issues,
		smoke_missing,
		actions,
	})
}

fn generate_protection_keypair(snapshot: &Snapshot) -> ProtectionKeyPair {
	let signing_key = SigningKey::generate(&mut OsRng);
	let verifying_key = signing_key.verifying_key();

	ProtectionKeyPair {
		schema_version: 1,
		key_kind: "oo_protection_ed25519_keypair".to_string(),
		created_at_epoch_s: now_epoch_s(),
		organism_id: snapshot.identity.organism_id.clone(),
		public_key_base64: BASE64.encode(verifying_key.to_bytes()),
		secret_key_base64: BASE64.encode(signing_key.to_bytes()),
	}
}

fn build_protection_attestation(
	snapshot: &Snapshot,
	manifest_path: &Path,
	manifest: &ProtectionManifest,
	key_path: Option<&Path>,
) -> Result<ProtectionAttestation, Box<dyn std::error::Error>> {
	let manifest_sha256 = sha256_file_hex(manifest_path)?;
	let mut attestation = ProtectionAttestation {
		schema_version: 1,
		attestation_kind: "oo_code_protection_attestation",
		sealed_at_epoch_s: now_epoch_s(),
		organism_id: snapshot.identity.organism_id.clone(),
		runtime_habitat: snapshot.identity.runtime_habitat.clone(),
		manifest_path: manifest_path.canonicalize().unwrap_or_else(|_| manifest_path.to_path_buf()).display().to_string(),
		manifest_sha256,
		manifest_generated_at_epoch_s: manifest.generated_at_epoch_s,
		workspace_root: manifest.workspace_root.clone(),
		file_count: manifest.file_count,
		signature: None,
	};

	if let Some(key_path) = key_path {
		let keypair: ProtectionKeyPair = read_json(key_path)?;
		let signature = sign_attestation(&attestation, &keypair)?;
		attestation.signature = Some(signature);
	}

	Ok(attestation)
}

fn sign_attestation(
	attestation: &ProtectionAttestation,
	keypair: &ProtectionKeyPair,
) -> Result<ProtectionSignature, Box<dyn std::error::Error>> {
	let secret_bytes = decode_fixed_32(&keypair.secret_key_base64, "secret key")?;
	let public_bytes = decode_fixed_32(&keypair.public_key_base64, "public key")?;
	let signing_key = SigningKey::from_bytes(&secret_bytes);
	let verifying_key = VerifyingKey::from_bytes(&public_bytes)?;
	let payload = serde_json::to_vec(attestation)?;
	let signature = signing_key.sign(&payload);
	verifying_key.verify(&payload, &signature)?;

	Ok(ProtectionSignature {
		algorithm: "ed25519",
		public_key_base64: keypair.public_key_base64.clone(),
		signature_base64: BASE64.encode(signature.to_bytes()),
	})
}

fn diff_manifests(saved: &ProtectionManifest, current: &ProtectionManifest) -> ManifestDiff {
	let saved_map = manifest_map(saved);
	let current_map = manifest_map(current);
	let mut added = Vec::new();
	let mut changed = Vec::new();
	let mut removed = Vec::new();

	for (path, current_hash) in &current_map {
		match saved_map.get(path) {
			None => added.push(path.clone()),
			Some(saved_hash) if saved_hash != current_hash => changed.push(path.clone()),
			Some(_) => {}
		}
	}

	for path in saved_map.keys() {
		if !current_map.contains_key(path) {
			removed.push(path.clone());
		}
	}

	ManifestDiff { added, changed, removed }
}

fn manifest_map(manifest: &ProtectionManifest) -> BTreeMap<String, String> {
	manifest
		.entries
		.iter()
		.map(|entry| (entry.rel_path.clone(), entry.sha256.clone()))
		.collect()
}

fn decode_fixed_32(value: &str, label: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
	let bytes = BASE64.decode(value)?;
	bytes
		.try_into()
		.map_err(|_| format!("invalid {label} length: expected 32 bytes").into())
}

fn continuity_summary(snapshot: &Snapshot) -> &'static str {
	match &snapshot.sovereign_export {
		None => "no_host_export",
		Some(export) => {
			if export.schema_version != 1 || export.export_kind != "oo_sovereign_handoff" {
				return "invalid_host_export";
			}
			let local_epoch = snapshot.state.continuity_epoch;
			if export.continuity_epoch > local_epoch {
				"host_ahead"
			} else if export.continuity_epoch < local_epoch {
				"host_stale"
			} else if mode_stricter_than(&snapshot.state.mode, &export.mode) {
				"host_aligned_local_safer"
			} else {
				"aligned"
			}
		}
	}
}

fn layout_relationship(host_root: &Path, workspace_root: &Path) -> &'static str {
	match (host_root.parent(), workspace_root.parent()) {
		(Some(host_parent), Some(workspace_parent)) if host_parent == workspace_parent => "sibling",
		_ => "custom",
	}
}

fn present_absent(flag: bool) -> &'static str {
	if flag {
		"present"
	} else {
		"absent"
	}
}

fn recommend_sovereign_actions(
	snapshot: &Snapshot,
	workspace_root: &Path,
	paths: &AppPaths,
	host_root: &Path,
) -> Vec<String> {
	let mut out = Vec::new();
	let handoff_script = workspace_root.join("test-qemu-handoff.ps1");
	let handoff_autorun = workspace_root.join("llmk-autorun-handoff-smoke.txt");
	let manifest_path = paths
		.identity_path
		.parent()
		.unwrap_or(Path::new("data"))
		.join("code_protection_manifest.json");
	let attestation_path = paths
		.identity_path
		.parent()
		.unwrap_or(Path::new("data"))
		.join("code_protection_attestation.json");

	if !paths.sovereign_export_path.exists() {
		out.push("Generate a fresh sovereign export from oo-host before the next handoff validation.".to_string());
	}
	if !handoff_script.exists() {
		out.push("Restore or add `test-qemu-handoff.ps1` in the sovereign repo before running handoff smoke checks.".to_string());
	}
	if !handoff_autorun.exists() {
		out.push("Restore `llmk-autorun-handoff-smoke.txt` so the handoff smoke can run deterministically.".to_string());
	}
	if layout_relationship(host_root, workspace_root) != "sibling" {
		out.push("Use `-OoHostRoot` or `OO_HOST_ROOT` when the sovereign repo is not cloned beside oo-host.".to_string());
	}
	if !manifest_path.exists() {
		out.push("Generate a protection manifest for the sovereign repo and keep it as release evidence.".to_string());
	}
	if manifest_path.exists() && !attestation_path.exists() {
		out.push("Seal the current protection manifest with `protect-stamp` to produce timestamped evidence.".to_string());
	}

	for action in recommend_actions(snapshot) {
		if !out.contains(&action) {
			out.push(action);
		}
	}

	out.truncate(6);
	out
}

fn validate_handoff_export(export: &serde_json::Value) -> Vec<String> {
	let mut issues = Vec::new();
	let required = [
		"schema_version",
		"export_kind",
		"generated_at_epoch_s",
		"organism_id",
		"genesis_id",
		"runtime_habitat",
		"runtime_instance_id",
		"continuity_epoch",
		"boot_or_start_count",
		"mode",
		"policy",
		"active_goal_count",
		"top_goals",
		"recent_events",
	];

	for field in required {
		if export.get(field).is_none() {
			issues.push(format!("missing required field `{field}`"));
		}
	}

	match export.get("schema_version").and_then(serde_json::Value::as_u64) {
		Some(1) => {}
		Some(other) => issues.push(format!("unsupported schema_version `{other}`")),
		None => {}
	}

	match export.get("export_kind").and_then(serde_json::Value::as_str) {
		Some("oo_sovereign_handoff") => {}
		Some(other) => issues.push(format!("unexpected export_kind `{other}`")),
		None => {}
	}

	match export.get("mode").and_then(serde_json::Value::as_str) {
		Some("normal" | "degraded" | "safe") => {}
		Some(other) => issues.push(format!("invalid mode `{other}`")),
		None => {}
	}

	match export.get("policy") {
		Some(policy) if policy.is_object() => {
			match policy.get("enforcement").and_then(serde_json::Value::as_str) {
				Some("off" | "observe" | "enforce") => {}
				Some(other) => issues.push(format!("invalid policy.enforcement `{other}`")),
				None => issues.push("missing required field `policy.enforcement`".to_string()),
			}
		}
		Some(_) => issues.push("field `policy` must be an object".to_string()),
		None => {}
	}

	if let Some(top_goals) = export.get("top_goals") {
		if !top_goals.is_array() {
			issues.push("field `top_goals` must be an array".to_string());
		}
	}

	if let Some(recent_events) = export.get("recent_events") {
		if !recent_events.is_array() {
			issues.push("field `recent_events` must be an array".to_string());
		}
	}

	issues
}

fn read_smoke_commands(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
	if !path.exists() {
		return Ok(Vec::new());
	}

	let file = File::open(path)?;
	let reader = BufReader::new(file);
	let mut commands = Vec::new();

	for line in reader.lines() {
		let line = line?;
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}
		commands.push(trimmed.to_string());
	}

	Ok(commands)
}

fn missing_smoke_commands(commands: &[String]) -> Vec<&'static str> {
	let expected = [
		"/oo_handoff_info",
		"/oo_handoff_apply",
		"/oo_handoff_receipt",
		"/oo_continuity_status",
	];

	expected
		.into_iter()
		.filter(|cmd| !commands.iter().any(|item| item == cmd))
		.collect()
}

fn recommend_handoff_actions(
	snapshot: &Snapshot,
	issues: &[String],
	smoke_missing: &[&'static str],
) -> Vec<String> {
	let mut out = Vec::new();

	if !issues.is_empty() {
		out.push("Regenerate the host sovereign export after fixing the reported contract mismatches.".to_string());
	}
	if !smoke_missing.is_empty() {
		out.push("Restore the missing handoff smoke commands before relying on the sovereign autorun validation path.".to_string());
	}

	for item in recommend_actions(snapshot) {
		if !out.contains(&item) {
			out.push(item);
		}
	}

	out.truncate(5);
	out
}

fn mode_stricter_than(local: &RuntimeMode, export_mode: &str) -> bool {
	local.rank() > export_mode_rank(export_mode)
}

fn export_mode_rank(mode: &str) -> u8 {
	match mode {
		"normal" => 0,
		"degraded" => 1,
		"safe" => 2,
		_ => 255,
	}
}

fn recommend_actions(snapshot: &Snapshot) -> Vec<String> {
	let mut out = Vec::new();

	match continuity_summary(snapshot) {
		"no_host_export" => out.push("Generate a fresh sovereign handoff export from oo-host before the next sovereign validation run.".to_string()),
		"invalid_host_export" => out.push("Repair the host sovereign export format before using it for handoff-driven workflows.".to_string()),
		"host_ahead" => out.push("Apply the newer host continuity state to sovereign runtime and re-run continuity diagnostics.".to_string()),
		"host_stale" => out.push("Refresh the host export because sovereign continuity is ahead of the last recorded host handoff.".to_string()),
		"host_aligned_local_safer" => out.push("Keep the sovereign posture: local runtime is already stricter than host continuity suggests.".to_string()),
		_ => {}
	}

	if let Some(reason) = snapshot.state.last_recovery_reason.as_deref() {
		out.push(format!("Investigate last recovery reason `{reason}` and capture a follow-up journal event or issue."));
	}

	if active_goals(&snapshot.state).is_empty() {
		out.push("Create at least one active organism goal so the host runtime has a concrete next intent.".to_string());
	} else {
		let goals = active_goals(&snapshot.state);
		let top = top_goals(&goals, 1);
		if let Some(goal) = top.first() {
			out.push(format!("Advance the top active goal `{}` and record progress in the journal.", goal.title));
		}
	}

	if matches!(snapshot.state.policy.enforcement, PolicyEnforcement::Observe) {
		out.push("Review whether host policy can graduate from observe to enforce without weakening sovereign invariants.".to_string());
	}

	if snapshot.recent_events.is_empty() {
		out.push("Emit at least one journal event from oo-host to give the organism a causal trail.".to_string());
	} else if let Some(last) = snapshot.recent_events.last() {
		out.push(format!("Use the latest event `{}` as the basis for the next GitHub or engineering update.", last.kind));
	}

	out.truncate(5);
	out
}

fn active_goals(state: &State) -> Vec<&Goal> {
	state
		.goals
		.iter()
		.filter(|g| g.status != "done" && g.status != "aborted")
		.collect()
}

fn top_goals<'a>(goals: &'a [&'a Goal], count: usize) -> Vec<&'a Goal> {
	let mut items = goals.to_vec();
	items.sort_by(|a, b| {
		b.priority
			.cmp(&a.priority)
			.then_with(|| b.updated_at_epoch_s.cmp(&a.updated_at_epoch_s))
			.then_with(|| a.created_at_epoch_s.cmp(&b.created_at_epoch_s))
	});
	items.into_iter().take(count).collect()
}

fn read_journal_tail(path: &Path, count: usize) -> Result<Vec<JournalEvent>, Box<dyn std::error::Error>> {
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

	if events.len() > count {
		Ok(events.split_off(events.len() - count))
	} else {
		Ok(events)
	}
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
	let file = File::open(path)?;
	Ok(serde_json::from_reader(file)?)
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)?;
	}
	let file = File::create(path)?;
	serde_json::to_writer_pretty(file, value)?;
	Ok(())
}

fn write_text_file(path: &Path, content: &str) -> Result<(), Box<dyn std::error::Error>> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)?;
	}
	fs::write(path, content)?;
	Ok(())
}

fn collect_workspace_files(
	root: &Path,
	current: &Path,
	out: &mut Vec<ProtectionEntry>,
) -> Result<(), Box<dyn std::error::Error>> {
	for entry in fs::read_dir(current)? {
		let entry = entry?;
		let path = entry.path();
		let file_type = entry.file_type()?;
		let name = entry.file_name();
		let name = name.to_string_lossy();

		if file_type.is_dir() {
			if should_skip_dir(&name) {
				continue;
			}
			collect_workspace_files(root, &path, out)?;
		} else if file_type.is_file() {
			if !is_protected_source_file(&path, &name) {
				continue;
			}
			let rel = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
			let meta = entry.metadata()?;
			out.push(ProtectionEntry {
				rel_path: rel,
				sha256: sha256_file_hex(&path)?,
				bytes: meta.len(),
			});
		}
	}
	Ok(())
}

fn should_skip_dir(name: &str) -> bool {
	matches!(
		name,
		".git" | ".github" | ".venv" | "target" | "node_modules" | "__pycache__" | ".pytest_cache" | ".mypy_cache"
	)
}

fn is_protected_source_file(path: &Path, name: &str) -> bool {
	if name.eq_ignore_ascii_case("Makefile") || name.eq_ignore_ascii_case("LICENSE") || name.eq_ignore_ascii_case("README.md") {
		return true;
	}

	match path.extension().and_then(|ext| ext.to_str()).map(|s| s.to_ascii_lowercase()) {
		Some(ext) => matches!(
			ext.as_str(),
			"c" | "h" | "rs" | "toml" | "md" | "ps1" | "sh" | "yml" | "yaml" | "json" | "txt" | "py"
		),
		None => false,
	}
}

fn sha256_file_hex(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
	let mut file = File::open(path)?;
	let mut hasher = Sha256::new();
	let mut buf = [0u8; 8192];

	loop {
		let n = file.read(&mut buf)?;
		if n == 0 {
			break;
		}
		hasher.update(&buf[..n]);
	}

	Ok(format!("{:x}", hasher.finalize()))
}

fn now_epoch_s() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|d| d.as_secs())
		.unwrap_or(0)
}
