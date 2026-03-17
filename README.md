# oo-host

Minimal host-side runtime skeleton for the Operating Organism.

## v0 goals

- stable organism identity
- persistent state
- append-only journal
- goal management
- deterministic operator CLI

## Commands

- `cargo run -- status`
- `cargo run -- status --format markdown`
- `cargo run -- status --format markdown --out data/status.md`
- `cargo run -- goal add "first goal"`
- `cargo run -- goal start <goal-id>`
- `cargo run -- goal hold <goal-id> --reason operator_hold`
- `cargo run -- goal note <goal-id> "context for operators" --author operator`
- `cargo run -- goal abort <goal-id> --reason operator_abort`
- `cargo run -- goal resume <goal-id>`
- `cargo run -- goals list`
- `cargo run -- goals next`
- `cargo run -- goals inspect <goal-id>`
- `cargo run -- goals inspect <goal-id> --format markdown`
- `cargo run -- goals inspect <goal-id> --format markdown --out data/goal.md`
- `cargo run -- report daily --out-dir data/reports/daily`
- `cargo run -- report daily --include-sovereign --sovereign-workspace ../llm-baremetal`
- `cargo run -- report daily --include-sovereign --include-sync --sovereign-workspace ../llm-baremetal`
- `cargo run -- worker beat clock --role heartbeat`
- `cargo run -- worker list`
- `cargo run -- worker watchdog --cycles 5 --interval-ms 1000`
- `cargo run -- tick`
- `cargo run -- journal tail -n 20`
- `cargo run -- journal explain -n 20`

Worker health notes:

- `worker beat` records a worker heartbeat with a role and summary.
- `worker list` shows whether each worker is currently `alive` or `stale`.
- `worker watchdog` runs a small supervision loop and prints per-cycle health and mode results.
- `tick` applies the same homeostasis logic automatically.
- When workers go stale, the runtime degrades to `Degraded`; when all known workers recover, it restores to `Normal`.
- A goal in `doing` becomes `blocked` during worker-health degradation, then moves to `recovering` after health restoration.
- The scheduler prefers `recovering` goals before new `pending` work, and `goal resume` can also restore a blocked or recovering goal manually.
- Each goal now carries a persisted `hold_reason`, so `blocked` state can distinguish cases like `worker_health`, `operator_hold`, or future policy-driven holds.
- With `policy enforcement = enforce`, goals whose `safety_class` is not `normal` are automatically moved to `blocked` with `hold_reason=policy_hold`.
- When enforcement relaxes again, those policy-held goals move to `recovering` and re-enter scheduler priority ahead of fresh `pending` work.
- `journal explain` renders recent journal entries as operator-readable transition summaries instead of raw JSONL.
- `goal note` attaches persisted human context to a goal without changing its lifecycle status.
- Goal listings now expose note counts, and `goals next` shows the latest note when one exists.
- `goals inspect` renders one goal with current status, hold reason, notes, and related journal transitions.
- `status --format markdown` and `goals inspect --format markdown` produce shareable operator summaries.
- `--out <file>` writes those summaries directly to a file for handoff or reporting.
- `report daily` writes a compact operator bundle with `status.md`, `next-goal.md`, and `journal-explain.md`.
- `report daily --include-sovereign` also writes `sovereign.md` by summarizing the sibling [llm-baremetal](../llm-baremetal) workspace when present.
- `report daily --include-sync` also writes `sync.md` with a direct host-versus-sovereign receipt comparison.

## oo-bot companion

The workspace now also includes an `oo-bot` companion binary inside the same Cargo package.

Examples:

- `cargo run --bin oo-bot -- status`
- `cargo run --bin oo-bot -- brief`
- `cargo run --bin oo-bot -- next`
- `cargo run --bin oo-bot -- github-brief --format markdown`
- `cargo run --bin oo-bot -- github-issue "Title" --focus continuity`
- `cargo run --bin oo-bot -- github-pr "Title" --head feature/x --base main`
- `cargo run --bin oo-bot -- protect-status --workspace ../llm-baremetal`
- `cargo run --bin oo-bot -- protect-manifest --workspace ../llm-baremetal`
- `cargo run --bin oo-bot -- protect-verify --workspace ../llm-baremetal --manifest data/code_protection_manifest.json`
- `cargo run --bin oo-bot -- protect-keygen`
- `cargo run --bin oo-bot -- protect-stamp --manifest data/code_protection_manifest.json --key data/protection_ed25519_key.json`
- `cargo run --bin oo-bot -- sovereign-status --workspace ../llm-baremetal`
- `cargo run --bin oo-bot -- handoff-check --workspace ../llm-baremetal`
- `cargo run --bin oo-bot -- handoff-status --workspace ../llm-baremetal`
- `cargo run --bin oo-bot -- handoff-status --workspace ../llm-baremetal --format markdown`
- `cargo run --bin oo-bot -- handoff-status --workspace ../llm-baremetal --format markdown --out data/handoff-status.md`
- `cargo run --bin oo-bot -- handoff-pack --workspace ../llm-baremetal --out data/handoff-pack`

The `handoff-pack` command writes a compact operator bundle:

- `handoff-status.md`
- `sync-check.txt`
- `sovereign-brief.md`
- `cargo run --bin oo-bot -- sovereign-brief --workspace ../llm-baremetal --format markdown`
- `cargo run --bin oo-bot -- github-sovereign-brief --workspace ../llm-baremetal --format markdown`
- `cargo run --bin oo-bot -- github-sovereign-issue "Sovereign integration follow-up" --workspace ../llm-baremetal --format markdown`
- `cargo run --bin oo-bot -- github-sovereign-pr "Sovereign integration update" --workspace ../llm-baremetal --head feature/x --base main --format markdown`
- `cargo run --bin oo-bot -- github-sovereign-pack --workspace ../llm-baremetal --head feature/x --base main`
- `cargo run --bin oo-bot -- receipt-check --workspace ../llm-baremetal`
- `cargo run --bin oo-bot -- sync-check --workspace ../llm-baremetal`

Current role:

- summarize organism state for operator use
- emit GitHub-friendly project briefs
- emit GitHub-ready issue and PR markdown
- emit code-protection provenance manifests for the workspace
- verify workspace drift against a saved protection manifest
- generate timestamped protection attestations, optionally signed with an Ed25519 key
- inspect sovereign workspace readiness and sibling-repo handoff posture
- validate the current sovereign export contract and smoke-script readiness
- emit a single operator status for the full handoff/sync loop
- write a compact handoff pack with status, sync check, and sovereign brief files
- emit a concise sovereign integration brief for operator or GitHub use
- emit a GitHub-ready sovereign report with checklist-style next actions
- emit a GitHub-ready sovereign issue draft from current handoff state
- emit a GitHub-ready sovereign PR draft from current handoff state
- write a connected GitHub-ready sovereign pack to files for operator workflow
- compare host state with the sovereign handoff receipt observed in `llm-baremetal`
- compare host state, export, and sovereign receipt with a single sync verdict
- suggest the next engineering actions from goals, journal, and continuity posture

Protection note:

- `oo-bot` cannot make theft impossible
- it can create strong provenance evidence: hashes, manifests, continuity context, signed attestations, and release-ready protection reports

## Integration with llm-baremetal

Recommended layout:

- sibling clone of [llm-baremetal](../llm-baremetal)
- sibling clone of [oo-host](.)

Example:

- `workspace-root/llm-baremetal`
- `workspace-root/oo-host`

This layout matches the current handoff tooling in [../llm-baremetal/test-qemu-handoff.ps1](../llm-baremetal/test-qemu-handoff.ps1), which expects `oo-host` to live beside `llm-baremetal`.

In that setup, `oo-bot` can still protect and analyze the sovereign repo directly, for example:

- `cargo run --bin oo-bot -- protect-status --workspace ../llm-baremetal`
- `cargo run --bin oo-bot -- protect-manifest --workspace ../llm-baremetal`
- `cargo run --bin oo-bot -- protect-verify --workspace ../llm-baremetal --manifest data/code_protection_manifest.json`

## Data layout

By default the CLI stores data in `./data/`:

- `organism_identity.json`
- `organism_state.json`
- `organism_journal.jsonl`
- `organism_recovery.json`

The `data/` directory is local runtime state and is ignored by Git by default.

## CI

The GitHub workflow in [.github/workflows/oo-host-ci.yml](.github/workflows/oo-host-ci.yml):

- runs `cargo check`
- runs `cargo test`
- verifies CLI help for key `oo-bot` commands
- renders a sample `handoff-pack`
- uploads the rendered handoff artifact bundle

## Notes

This does not replace `llm-baremetal`.
It is the host-side daily-life counterpart of the sovereign runtime.
