use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use ort::{session::Session, value::Outlet};
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(about = "Phase 1 smoke check: load ONNX with Rust ort and report IO signature")]
struct Cli {
    #[arg(long, value_name = "PATH")]
    model: PathBuf,
}

#[derive(Debug, Serialize)]
struct OutletSummary {
    name: String,
    shape: Vec<Option<i64>>,
    value_type: String,
}

#[derive(Debug, Serialize)]
struct SignatureSummary {
    input_count: usize,
    output_count: usize,
    inputs: Vec<OutletSummary>,
    outputs: Vec<OutletSummary>,
}

fn summarize_outlet(outlet: &Outlet) -> OutletSummary {
    let shape = outlet
        .dtype()
        .tensor_shape()
        .map(|dims| {
            dims.iter()
                .map(|dim| if *dim < 0 { None } else { Some(*dim) })
                .collect::<Vec<Option<i64>>>()
        })
        .unwrap_or_default();

    OutletSummary {
        name: outlet.name().to_string(),
        shape,
        value_type: outlet.dtype().to_string(),
    }
}

fn run() -> Result<()> {
    let args = Cli::parse();
    let model_path = args.model.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize model path '{}'",
            args.model.display()
        )
    })?;

    let session = Session::builder()
        .context("failed to initialize ORT session builder")?
        .with_intra_threads(1)
        .context("failed setting ORT intra-op threads")?
        .with_inter_threads(1)
        .context("failed setting ORT inter-op threads")?
        .commit_from_file(&model_path)
        .with_context(|| {
            format!(
                "failed to load model via Rust ORT: {}",
                model_path.display()
            )
        })?;

    let summary = SignatureSummary {
        input_count: session.inputs().len(),
        output_count: session.outputs().len(),
        inputs: session.inputs().iter().map(summarize_outlet).collect(),
        outputs: session.outputs().iter().map(summarize_outlet).collect(),
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&summary).context("failed to serialize signature summary")?
    );
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("ERROR: {err:#}");
        std::process::exit(1);
    }
}
