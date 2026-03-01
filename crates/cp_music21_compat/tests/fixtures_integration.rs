use std::fs;
use std::path::PathBuf;

use cp_music21_compat::{serialize_timeline_artifact, Music21Compat, Music21CompatApi};
use pretty_assertions::assert_eq;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn render_fixture(name: &str) -> String {
    let xml = fs::read_to_string(fixtures_dir().join(format!("{name}.musicxml")))
        .expect("read xml fixture");
    let api = Music21Compat;
    let artifact = api
        .timeline_from_musicxml(name, &xml)
        .expect("parse/build timeline artifact");
    serialize_timeline_artifact(&artifact).expect("serialize")
}

#[test]
fn fixture_snapshots_match_expected_serialization() {
    for fixture in [
        "tied_barlines",
        "pickup_anacrusis",
        "dense_poly",
        "enharmonic_double",
    ] {
        let got = render_fixture(fixture);
        let expected = fs::read_to_string(fixtures_dir().join(format!("{fixture}.expected.json")))
            .expect("read expected snapshot");
        assert_eq!(
            got.trim_end(),
            expected.trim_end(),
            "snapshot mismatch for {fixture}"
        );
    }
}

#[test]
fn serialization_is_deterministic_across_repeated_runs() {
    let first = render_fixture("dense_poly");
    let second = render_fixture("dense_poly");
    let third = render_fixture("dense_poly");
    assert_eq!(first, second);
    assert_eq!(second, third);
}

#[test]
fn edge_case_flags_and_intervals_are_exact() {
    let tied = render_fixture("tied_barlines");
    assert!(tied.contains("\"onset\": false"));
    assert!(tied.contains("\"hold\": true"));

    let enharm = render_fixture("enharmonic_double");
    assert!(enharm.contains("\"interval_from_bass\": \"d3\""));
    assert!(enharm.contains("\"interval_from_bass\": \"AA4\""));
    assert!(enharm.contains("\"interval_from_bass\": \"d5\""));
}

#[test]
#[ignore]
fn dump_fixture_json_for_bootstrap() {
    for fixture in [
        "tied_barlines",
        "pickup_anacrusis",
        "dense_poly",
        "enharmonic_double",
    ] {
        println!("--- {fixture} ---\n{}", render_fixture(fixture));
    }
}
