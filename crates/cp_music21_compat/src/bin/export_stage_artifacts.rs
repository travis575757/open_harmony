use std::env;
use std::fs;

use cp_music21_compat::{
    augnet_initial_frames, augnet_reindex_frames, encode_stage_b_inputs, parse_musicxml,
    AugnetScoreFrame, StageBInputs,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StageAArtifact {
    event_frames: Vec<AugnetScoreFrame>,
    grid_frames: Vec<AugnetScoreFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StageExport {
    schema_version: u32,
    fixture_id: String,
    stage_a: StageAArtifact,
    stage_b: StageBInputs,
}

#[derive(Debug, Clone)]
struct CliArgs {
    fixture_id: String,
    musicxml_path: String,
    fixed_offset: f64,
    max_steps: usize,
    pretty: bool,
}

fn parse_args() -> Result<CliArgs, String> {
    let mut fixture_id: Option<String> = None;
    let mut musicxml_path: Option<String> = None;
    let mut fixed_offset: f64 = 0.125;
    let mut max_steps: usize = 640;
    let mut pretty = false;

    let args: Vec<String> = env::args().collect();
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--fixture-id" => {
                i += 1;
                fixture_id = args.get(i).cloned();
            }
            "--musicxml-path" => {
                i += 1;
                musicxml_path = args.get(i).cloned();
            }
            "--fixed-offset" => {
                i += 1;
                fixed_offset = args
                    .get(i)
                    .ok_or("missing value for --fixed-offset")?
                    .parse::<f64>()
                    .map_err(|_| "invalid --fixed-offset value".to_string())?;
            }
            "--max-steps" => {
                i += 1;
                max_steps = args
                    .get(i)
                    .ok_or("missing value for --max-steps")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --max-steps value".to_string())?;
            }
            "--pretty" => {
                pretty = true;
            }
            "--help" | "-h" => {
                return Err(
                    "Usage: export_stage_artifacts --fixture-id <id> --musicxml-path <path> [--fixed-offset <float>] [--max-steps <int>] [--pretty]".to_string(),
                );
            }
            other => {
                return Err(format!("unknown argument: {other}"));
            }
        }
        i += 1;
    }

    let fixture_id = fixture_id.ok_or("missing --fixture-id")?;
    let musicxml_path = musicxml_path.ok_or("missing --musicxml-path")?;
    if max_steps == 0 {
        return Err("--max-steps must be > 0".to_string());
    }

    Ok(CliArgs {
        fixture_id,
        musicxml_path,
        fixed_offset,
        max_steps,
        pretty,
    })
}

fn main() {
    let args = match parse_args() {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };

    let xml = match fs::read_to_string(&args.musicxml_path) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("failed to read {}: {err}", args.musicxml_path);
            std::process::exit(1);
        }
    };

    let parsed = match parse_musicxml(&xml) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("failed to parse musicxml: {err}");
            std::process::exit(1);
        }
    };

    let event_frames = augnet_initial_frames(&parsed);
    let grid_frames = augnet_reindex_frames(&event_frames, args.fixed_offset);
    let stage_b = encode_stage_b_inputs(&grid_frames, args.fixed_offset, args.max_steps);
    let payload = StageExport {
        schema_version: 1,
        fixture_id: args.fixture_id,
        stage_a: StageAArtifact {
            event_frames,
            grid_frames,
        },
        stage_b,
    };

    let rendered = if args.pretty {
        serde_json::to_string_pretty(&payload)
    } else {
        serde_json::to_string(&payload)
    };
    match rendered {
        Ok(out) => {
            println!("{out}");
        }
        Err(err) => {
            eprintln!("failed to serialize payload: {err}");
            std::process::exit(1);
        }
    }
}
