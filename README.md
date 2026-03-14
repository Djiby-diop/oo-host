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
- `cargo run -- goal add "first goal"`
- `cargo run -- goals list`
- `cargo run -- journal tail -n 20`

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
- `cargo run --bin oo-bot -- sovereign-brief --workspace ../llm-baremetal --format markdown`
- `cargo run --bin oo-bot -- github-sovereign-brief --workspace ../llm-baremetal --format markdown`

Current role:

- summarize organism state for operator use
- emit GitHub-friendly project briefs
- emit GitHub-ready issue and PR markdown
- emit code-protection provenance manifests for the workspace
- verify workspace drift against a saved protection manifest
- generate timestamped protection attestations, optionally signed with an Ed25519 key
- inspect sovereign workspace readiness and sibling-repo handoff posture
- validate the current sovereign export contract and smoke-script readiness
- emit a concise sovereign integration brief for operator or GitHub use
- emit a GitHub-ready sovereign report with checklist-style next actions
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

## Notes

This does not replace `llm-baremetal`.
It is the host-side daily-life counterpart of the sovereign runtime.
