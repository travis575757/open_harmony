use std::env;
use std::fs;

use cp_music21_compat::{build_timeline, parse_musicxml};

#[derive(Debug, Clone)]
struct CliArgs {
    fixture_id: String,
    musicxml_path: String,
    pretty: bool,
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = env::args().collect();
    let mut fixture_id: Option<String> = None;
    let mut musicxml_path: Option<String> = None;
    let mut pretty = false;

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
            "--pretty" => {
                pretty = true;
            }
            "--help" | "-h" => {
                return Err(
                    "Usage: export_timeline_artifact --fixture-id <id> --musicxml-path <path> [--pretty]"
                        .to_string(),
                );
            }
            other => {
                return Err(format!("unknown argument: {other}"));
            }
        }
        i += 1;
    }

    Ok(CliArgs {
        fixture_id: fixture_id.ok_or("missing --fixture-id")?,
        musicxml_path: musicxml_path.ok_or("missing --musicxml-path")?,
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
    let artifact = build_timeline(&parsed, &args.fixture_id);
    let rendered = if args.pretty {
        serde_json::to_string_pretty(&artifact)
    } else {
        serde_json::to_string(&artifact)
    };
    match rendered {
        Ok(out) => println!("{out}"),
        Err(err) => {
            eprintln!("failed to serialize timeline artifact: {err}");
            std::process::exit(1);
        }
    }
}
