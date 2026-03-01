use anyhow::{anyhow, Context, Result};
use cp_engine::augnet_onnx::{AugmentedNetOnnxBackend, AugmentedNetOnnxConfig};
use cp_engine::augnet_preprocess::{preprocess_musicxml_to_chunks, AugmentedNetPreprocessConfig};
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
struct Cli {
    fixture_id: String,
    musicxml_path: PathBuf,
    fixed_offset: f64,
    max_steps: usize,
    model_path: PathBuf,
    manifest_path: PathBuf,
    pretty: bool,
}

#[derive(Debug, Clone, Serialize)]
struct FrameRecord {
    global_step: usize,
    offset_q: f64,
    local_key: String,
    tonicized_key: String,
    roman_numeral: String,
    chord_quality: String,
    inversion: String,
    components: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
struct ExportArtifact {
    schema_version: u32,
    fixture_id: String,
    fixed_offset: f64,
    max_steps: usize,
    frame_count: usize,
    frames: Vec<FrameRecord>,
}

fn print_help() {
    eprintln!(
        "Usage: augnet_corpus_export \\
  --fixture-id <id> \\
  --musicxml-path <path> \\
  [--fixed-offset <float>] \\
  [--max-steps <int>] \\
  [--model-path <path>] \\
  [--manifest-path <path>] \\
  [--pretty]"
    );
}

fn parse_args() -> Result<Cli> {
    let mut fixture_id: Option<String> = None;
    let mut musicxml_path: Option<PathBuf> = None;
    let mut fixed_offset: f64 = 0.125;
    let mut max_steps: usize = 640;
    let mut model_path = PathBuf::from("models/augnet/AugmentedNet.onnx");
    let mut manifest_path = PathBuf::from("models/augnet/model-manifest.json");
    let mut pretty = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture-id" => {
                fixture_id = Some(
                    args.next()
                        .ok_or_else(|| anyhow!("missing value for --fixture-id"))?,
                );
            }
            "--musicxml-path" => {
                musicxml_path =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow!("missing value for --musicxml-path")
                    })?));
            }
            "--fixed-offset" => {
                let raw = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --fixed-offset"))?;
                fixed_offset = raw
                    .parse::<f64>()
                    .with_context(|| format!("invalid --fixed-offset value: {raw}"))?;
            }
            "--max-steps" => {
                let raw = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --max-steps"))?;
                max_steps = raw
                    .parse::<usize>()
                    .with_context(|| format!("invalid --max-steps value: {raw}"))?;
            }
            "--model-path" => {
                model_path = PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow!("missing value for --model-path"))?,
                );
            }
            "--manifest-path" => {
                manifest_path = PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow!("missing value for --manifest-path"))?,
                );
            }
            "--pretty" => {
                pretty = true;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(anyhow!("unknown argument: {arg}")),
        }
    }

    let fixture_id = fixture_id.ok_or_else(|| anyhow!("missing required --fixture-id"))?;
    let musicxml_path = musicxml_path.ok_or_else(|| anyhow!("missing required --musicxml-path"))?;

    Ok(Cli {
        fixture_id,
        musicxml_path,
        fixed_offset,
        max_steps,
        model_path,
        manifest_path,
        pretty,
    })
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

fn main() -> Result<()> {
    let cli = parse_args()?;
    let musicxml = fs::read_to_string(&cli.musicxml_path)
        .with_context(|| format!("failed to read {}", cli.musicxml_path.display()))?;
    let preprocess_config = AugmentedNetPreprocessConfig {
        fixed_offset: cli.fixed_offset,
        max_steps: cli.max_steps,
        ..AugmentedNetPreprocessConfig::default()
    };
    let artifact = preprocess_musicxml_to_chunks(&musicxml, &preprocess_config)
        .context("preprocess_musicxml_to_chunks failed")?;

    let backend = AugmentedNetOnnxBackend::new(AugmentedNetOnnxConfig {
        model_path: cli.model_path.clone(),
        manifest_path: cli.manifest_path.clone(),
        ..AugmentedNetOnnxConfig::default()
    })
    .context("AugmentedNetOnnxBackend::new failed")?;
    let outputs = backend
        .infer_preprocessed_chunks(&artifact.chunks)
        .context("infer_preprocessed_chunks failed")?;

    if outputs.len() != artifact.chunks.len() {
        return Err(anyhow!(
            "chunk/output count mismatch: chunks={}, outputs={}",
            artifact.chunks.len(),
            outputs.len()
        ));
    }

    let mut frames = Vec::new();
    for (chunk, output) in artifact.chunks.iter().zip(outputs.iter()) {
        let stage_d = output
            .to_stage_d_artifact()
            .context("stage_d decode failed for chunk")?;
        let active_steps = chunk
            .global_end_step_exclusive
            .saturating_sub(chunk.global_start_step);
        if active_steps > stage_d.labels.len() {
            return Err(anyhow!(
                "chunk active_steps={} exceeds stage_d labels={}",
                active_steps,
                stage_d.labels.len()
            ));
        }
        for local_step in 0..active_steps {
            let global_step = chunk.global_start_step + local_step;
            let label = &stage_d.labels[local_step];
            let offset_q = artifact
                .grid_frames
                .get(global_step)
                .map(|f| f.s_offset)
                .unwrap_or((global_step as f64) * cli.fixed_offset);
            frames.push(FrameRecord {
                global_step,
                offset_q: round6(offset_q),
                local_key: label.local_key.clone(),
                tonicized_key: label.tonicized_key_resolved.clone(),
                roman_numeral: label.roman_numeral_formatted.clone(),
                chord_quality: label.chord_quality.clone(),
                inversion: label.inversion_figure.clone(),
                components: label.components.clone(),
            });
        }
    }

    let output = ExportArtifact {
        schema_version: 1,
        fixture_id: cli.fixture_id,
        fixed_offset: cli.fixed_offset,
        max_steps: cli.max_steps,
        frame_count: frames.len(),
        frames,
    };

    if cli.pretty {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", serde_json::to_string(&output)?);
    }
    Ok(())
}
