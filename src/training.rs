/// training.rs — Firmware training artifact bridge for oo-host.
///
/// Reads OO_TRAIN.JSONL produced by the firmware's journal_train module
/// and provides host-side analysis, filtering, and export for SFT / RLHF pipelines.
///
/// OO_TRAIN.JSONL format (each line):
/// {
///   "instruction": "<ctx_prefix + prompt>",
///   "response": "<model output>",
///   "meta": {
///     "boot_count": N,
///     "quality": N,
///     "pressure": N,
///     "phase": N,
///     "diverged": 0|1
///   }
/// }

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
struct TrainMeta {
    boot_count: Option<u64>,
    quality:    Option<u8>,
    pressure:   Option<u8>,
    phase:      Option<u8>,
    diverged:   Option<u8>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TrainSample {
    instruction: Option<String>,
    response:    Option<String>,
    meta:        Option<TrainMeta>,
}

#[derive(Debug, Serialize)]
struct ExportSample {
    instruction: String,
    response:    String,
    quality:     u8,
    phase:       u8,
}

/// Quality distribution (histogram over 0..=10)
pub struct TrainStats {
    pub total:           usize,
    pub diverged:        usize,
    pub quality_hist:    [usize; 11],
    pub pressure_hist:   [usize; 5],
    pub phase_hist:      [usize; 3],
    pub mean_quality:    f64,
}

fn read_train_file(path: &Path) -> std::io::Result<Vec<TrainSample>> {
    let f = File::open(path)?;
    let reader = BufReader::new(f);
    let mut samples = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            continue;
        }
        if let Ok(s) = serde_json::from_str::<TrainSample>(line) {
            samples.push(s);
        }
    }
    Ok(samples)
}

/// Return stats over OO_TRAIN.JSONL at `path`.
pub fn training_stats(path: &Path) -> std::io::Result<TrainStats> {
    let samples = read_train_file(path)?;

    let mut quality_hist    = [0usize; 11];
    let mut pressure_hist   = [0usize; 5];
    let mut phase_hist      = [0usize; 3];
    let mut diverged        = 0usize;
    let mut quality_sum     = 0u64;

    for s in &samples {
        let meta = s.meta.as_ref();
        let q = meta.and_then(|m| m.quality).unwrap_or(5) as usize;
        let p = meta.and_then(|m| m.pressure).unwrap_or(0) as usize;
        let ph = meta.and_then(|m| m.phase).unwrap_or(0) as usize;
        let d = meta.and_then(|m| m.diverged).unwrap_or(0);

        if q <= 10 { quality_hist[q] += 1; quality_sum += q as u64; }
        if p < 5   { pressure_hist[p] += 1; }
        if ph < 3  { phase_hist[ph] += 1; }
        if d == 1  { diverged += 1; }
    }

    let total = samples.len();
    let mean_quality = if total > 0 { quality_sum as f64 / total as f64 } else { 0.0 };

    Ok(TrainStats {
        total,
        diverged,
        quality_hist,
        pressure_hist,
        phase_hist,
        mean_quality,
    })
}

/// Print a full training summary to stdout.
pub fn print_training_summary(path: &Path) -> std::io::Result<()> {
    let stats = training_stats(path)?;
    let pressure_names = ["CALM", "AWARE", "STRESSED", "CRITICAL", "DYING"];
    let phase_names    = ["NORMAL", "DEGRADED", "SAFE"];

    println!("=== firmware training dataset ===");
    println!("source_file  : {}", path.display());
    println!("total_samples: {}", stats.total);
    println!("mean_quality : {:.2}/10", stats.mean_quality);
    println!("diverged     : {} ({:.1}%)",
             stats.diverged,
             if stats.total > 0 { stats.diverged as f64 * 100.0 / stats.total as f64 } else { 0.0 });

    println!();
    println!("quality distribution (high → low):");
    for q in (0..=10).rev() {
        if stats.quality_hist[q] == 0 { continue; }
        let pct = stats.quality_hist[q] as f64 * 100.0 / stats.total.max(1) as f64;
        let bar: String = std::iter::repeat('#').take((pct / 2.0) as usize).collect();
        println!("  [{:2}] {:4}  ({:5.1}%)  {}", q, stats.quality_hist[q], pct, bar);
    }

    println!();
    println!("pressure at generation:");
    for (i, &n) in stats.pressure_hist.iter().enumerate() {
        if n == 0 { continue; }
        let pct = n as f64 * 100.0 / stats.total.max(1) as f64;
        println!("  {:10} {:4}  ({:.1}%)", pressure_names[i], n, pct);
    }

    println!();
    println!("phase at generation:");
    for (i, &n) in stats.phase_hist.iter().enumerate() {
        if n == 0 { continue; }
        let pct = n as f64 * 100.0 / stats.total.max(1) as f64;
        println!("  {:10} {:4}  ({:.1}%)", phase_names[i], n, pct);
    }

    Ok(())
}

/// Export training samples to a clean JSONL file, filtered by minimum quality score.
/// Suitable for direct upload to HuggingFace SFT pipelines.
pub fn export_training(
    src_path: &Path,
    out_path: &Path,
    min_quality: u8,
) -> std::io::Result<usize> {
    let samples = read_train_file(src_path)?;
    let mut out = OpenOptions::new().create(true).write(true).truncate(true).open(out_path)?;
    let mut count = 0usize;

    for s in &samples {
        let meta = s.meta.as_ref();
        let q = meta.and_then(|m| m.quality).unwrap_or(0);
        let ph = meta.and_then(|m| m.phase).unwrap_or(0);

        if q < min_quality { continue; }

        let instruction = s.instruction.as_deref().unwrap_or("").to_string();
        let response    = s.response.as_deref().unwrap_or("").to_string();
        if instruction.is_empty() || response.is_empty() { continue; }

        let export = ExportSample { instruction, response, quality: q, phase: ph };
        serde_json::to_writer(&mut out, &export)?;
        out.write_all(b"\n")?;
        count += 1;
    }

    Ok(count)
}

/// Ingest OO_TRAIN.JSONL into the oo-host's local training dataset file (data/oo_train_host.jsonl).
/// Appends only new samples (boot_count > last_ingested_boot_count in oo_train_meta.json).
pub fn ingest_sovereign_training(
    src_path: &Path,
    data_dir: &Path,
) -> std::io::Result<usize> {
    let host_train_path = data_dir.join("oo_train_host.jsonl");
    let meta_path       = data_dir.join("oo_train_meta.json");

    // Read last ingested boot_count
    let last_boot: u64 = if meta_path.exists() {
        let text = std::fs::read_to_string(&meta_path)?;
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        v.get("last_ingested_boot").and_then(|x| x.as_u64()).unwrap_or(0)
    } else {
        0
    };

    let samples = read_train_file(src_path)?;
    let mut out = OpenOptions::new().create(true).append(true).open(&host_train_path)?;
    let mut count      = 0usize;
    let mut max_boot   = last_boot;

    for s in &samples {
        let bc = s.meta.as_ref().and_then(|m| m.boot_count).unwrap_or(0);
        if bc <= last_boot { continue; }

        if bc > max_boot { max_boot = bc; }
        serde_json::to_writer(&mut out, s)?;
        out.write_all(b"\n")?;
        count += 1;
    }

    if count > 0 {
        // Update meta
        let meta = serde_json::json!({
            "last_ingested_boot": max_boot,
            "total_ingested": count
        });
        std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    }

    Ok(count)
}

/// Locate OO_TRAIN.JSONL within a sovereign workspace path.
pub fn locate_sovereign_train(sovereign_workspace: &Path) -> Option<PathBuf> {
    // The firmware writes OO_TRAIN.JSONL to the EFI boot volume root.
    // When mounted on the host, it appears at the workspace root.
    let candidate = sovereign_workspace.join("OO_TRAIN.JSONL");
    if candidate.exists() {
        return Some(candidate);
    }
    // Some setups map it to efi/ subdirectory
    let candidate2 = sovereign_workspace.join("efi").join("OO_TRAIN.JSONL");
    if candidate2.exists() {
        return Some(candidate2);
    }
    None
}
