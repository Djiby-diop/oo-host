# OO Vision — Operating Organism

## What is the Operating Organism?

The Operating Organism (OO) is a persistent, self-aware software entity that lives across runtimes.
It maintains identity, memory, goals, and a continuous journal of its own existence.
It is not a single process — it is a living presence co-located with its operator and its sovereign LLM.

## Core philosophy

- **Continuity over uptime.** The organism does not die when its process stops.
  It recovers, increments its epoch, and resumes where it left off.

- **Goals as the unit of intent.** The organism does not execute scripts.
  It pursues goals — operator-authored, priority-ranked, lifecycle-tracked.

- **Journal as memory.** Every meaningful transition is appended to an immutable journal.
  The organism can always explain what happened and why.

- **Safe-first governance.** Policy is not an afterthought.
  The runtime enforces safety classes, deny-by-default posture, and operator-only authority.

- **Sovereign handoff.** The organism spans two sides: the host (this repo)
  and the sovereign LLM (`llm-baremetal`). Handoff is a first-class protocol,
  not a bolt-on export.

## Layers

```
┌─────────────────────────────────────────┐
│              Operator CLI               │  oo-host, oo-bot
├─────────────────────────────────────────┤
│           Host Runtime (v0)             │  identity · state · journal
│      goals · workers · policy · tick    │
├─────────────────────────────────────────┤
│         Sovereign Handoff Layer         │  export · receipt · sync
├─────────────────────────────────────────┤
│         Sovereign LLM Runtime           │  llm-baremetal
└─────────────────────────────────────────┘
```

## Identity

Every organism has:

- `organism_id` — stable UUID, generated once, never changes.
- `genesis_id` — UUID of the first boot, anchoring provenance.
- `runtime_habitat` — where it runs (`host_linux`, `host_windows`, etc.).
- `continuity_epoch` — increments on every recovery, tracking how many times
  the organism has resumed across process boundaries.

Identity is local-first: it lives in `data/organism_identity.json` and is never
regenerated unless the data directory is wiped.

## Goal lifecycle

```
pending ──► doing ──► done
   │           │
   │       blocked ──► recovering ──► doing
   │           │
   └──────► aborted
```

Goals carry:
- **priority** — scheduler preference
- **safety_class** — `normal` or elevated; elevated goals are held under strict policy
- **hold_reason** — why a goal is blocked (`operator_hold`, `worker_health`, `policy_hold`)
- **notes** — human context attached by operators without changing lifecycle status

## Worker health and homeostasis

Workers are external processes that register heartbeats.
If a worker goes stale (no beat within 5 minutes), the runtime:

1. Degrades to `Degraded` mode.
2. Blocks all active goals with `hold_reason=worker_health`.
3. Waits for all workers to recover.
4. Restores `Normal` mode and moves blocked goals to `recovering`.
5. The scheduler then prioritises `recovering` goals before fresh `pending` work.

This creates a closed-loop homeostasis without operator intervention.

## Policy governance

The policy layer controls what the organism is allowed to do autonomously:

| Field              | Default | Meaning                                            |
|--------------------|---------|----------------------------------------------------|
| `safe_first`       | true    | safety class gates autonomous goal progression     |
| `deny_by_default`  | true    | unknown goals are blocked until explicitly approved|
| `llm_advisory_only`| true    | LLM suggestions are advice, not commands           |
| `enforcement`      | observe | `off` / `observe` / `enforce`                      |

In `enforce` mode with `safe_first=true` and `deny_by_default=true`:
all non-`normal` goals are automatically held with `hold_reason=policy_hold`.
When enforcement relaxes, they transition to `recovering`.

## Sovereign handoff

The organism speaks to its LLM side through a structured handoff protocol:

1. `oo-host export sovereign` — writes `sovereign_export.json` with current state snapshot.
2. The export is read by `llm-baremetal` during QEMU boot.
3. After the LLM session, `llm-baremetal` writes `OOHANDOFF.TXT` — a flat key=value receipt.
4. `oo-bot sync-check` and `oo-bot handoff-status` compare the host state to the receipt
   and produce a verdicted sync report (`aligned`, `host_ahead`, `drift`, `organism_mismatch`).

The sync loop closes when both sides agree on organism_id, continuity_epoch, mode, and policy.

## Protection and provenance

`oo-bot` can generate cryptographic provenance for the sovereign workspace:

- **manifest** — SHA-256 hashes of every tracked file.
- **stamp** — a timestamped attestation, optionally signed with an Ed25519 key.
- **verify** — detect drift from a saved manifest.

This does not prevent theft, but it creates strong provenance evidence that is
difficult to repudiate in a dispute.

## Roadmap

### v0 — Skeleton (done)

- [x] Stable organism identity
- [x] Persistent state with recovery snapshots
- [x] Append-only journal with human-readable explain layer
- [x] Goal lifecycle: pending → doing → done / blocked / aborted / recovering
- [x] Worker heartbeat and health homeostasis
- [x] Policy enforcement with safe-first governance
- [x] Scheduler tick with homeostasis integration
- [x] Sovereign export and handoff receipt comparison
- [x] Daily operator reports (status, next-goal, journal-explain, sovereign, sync)
- [x] `oo-bot` companion: briefs, GitHub drafts, protection manifests, handoff pack

### v1 — Operational hardening

- [ ] Modular source structure (`types`, `io`, `goals`, `workers`, `policy`, `reports`, `export`)
- [ ] HTTP health endpoint for runtime status (optional, opt-in)
- [ ] Configurable stale threshold per worker role
- [ ] `journal search` — filter events by kind, severity, or time range
- [ ] `goal delete` — remove terminal goals to keep the list clean
- [ ] `goal tag` — arbitrary operator tags for grouping and filtering
- [ ] Compressed journal rotation when the file exceeds a configurable size
- [ ] `oo-bot diff` — compare two snapshots or two sovereign exports

### v2 — Distributed continuity

- [ ] Multi-host organism federation (same organism_id, different habitats)
- [ ] Cross-host journal merge with conflict resolution
- [ ] Autonomous goal delegation to sibling hosts
- [ ] Cryptographically signed journal entries
- [ ] Web dashboard for real-time organism status

## Relationship to `llm-baremetal`

`oo-host` is the **host-side** of the Operating Organism.
`llm-baremetal` is the **sovereign-side** — a QEMU-hosted LLM that boots from a compact image,
runs inside a deterministic environment, and writes back a handoff receipt on shutdown.

They are not the same process. They do not share memory.
They communicate through files: the export JSON going in, the receipt TXT coming out.

The organism persists across both sides because identity and continuity are tracked
on the host, and the sovereign inherits them on each boot via the export.

## Design constraints

- **No hidden state.** Everything meaningful is in `data/` as JSON or JSONL.
- **No daemons.** The runtime is a CLI tool invoked by the operator or a cron job.
- **No network by default.** The organism is air-gapped unless the operator explicitly
  connects it (e.g., through `oo-bot github-*` commands).
- **No LLM autonomy without policy clearance.** The LLM can suggest; the host decides.

## For contributors

The codebase is intentionally minimal and dependency-light.
`serde`, `clap`, `uuid`, `sha2`, `ed25519-dalek` — that is the full dependency surface.
New features should fit this constraint unless there is a compelling operational reason.

Tests live alongside the code in `#[cfg(test)] mod tests` blocks.
Every new function should have at least one unit test before merge.
