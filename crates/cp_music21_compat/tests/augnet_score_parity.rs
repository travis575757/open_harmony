use std::fs;
use std::path::PathBuf;

use cp_music21_compat::{serialize_augnet_frames, Music21Compat, Music21CompatApi};
use pretty_assertions::assert_eq;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn render_event_fixture(name: &str) -> String {
    let xml = fs::read_to_string(fixtures_dir().join(format!("{name}.musicxml")))
        .expect("read xml fixture");
    let api = Music21Compat;
    let frames = api
        .augnet_frames_from_musicxml(&xml, 0.25, true)
        .expect("parse/build event frames");
    serialize_augnet_frames(&frames).expect("serialize")
}

fn render_grid_fixture(name: &str) -> String {
    let xml = fs::read_to_string(fixtures_dir().join(format!("{name}.musicxml")))
        .expect("read xml fixture");
    let api = Music21Compat;
    let frames = api
        .augnet_frames_from_musicxml(&xml, 0.25, false)
        .expect("parse/build fixed-grid frames");
    serialize_augnet_frames(&frames).expect("serialize")
}

#[test]
fn event_snapshot_matches_expected() {
    for fixture in [
        "tied_barlines",
        "pickup_anacrusis",
        "dense_poly",
        "enharmonic_double",
    ] {
        let got = render_event_fixture(fixture);
        let expected = fs::read_to_string(
            fixtures_dir().join(format!("{fixture}.augnet.event.expected.json")),
        )
        .expect("read expected snapshot");
        assert_eq!(
            got.trim_end(),
            expected.trim_end(),
            "event snapshot mismatch for {fixture}"
        );
    }
}

#[test]
fn fixed_grid_snapshot_matches_expected() {
    for fixture in [
        "tied_barlines",
        "pickup_anacrusis",
        "dense_poly",
        "enharmonic_double",
    ] {
        let got = render_grid_fixture(fixture);
        let expected =
            fs::read_to_string(fixtures_dir().join(format!("{fixture}.augnet.grid.expected.json")))
                .expect("read expected snapshot");
        assert_eq!(
            got.trim_end(),
            expected.trim_end(),
            "grid snapshot mismatch for {fixture}"
        );
    }
}

#[test]
fn fixed_grid_holds_use_false_onset_flags() {
    let tied = render_grid_fixture("tied_barlines");
    assert!(tied.contains("\"s_is_onset\": [\n      false\n    ]"));
}

#[test]
fn event_rows_capture_measure_shift_and_enharmonic_spellings() {
    let pickup = render_event_fixture("pickup_anacrusis");
    assert!(pickup.contains("\"s_measure\": 0"));

    let enharmonic = render_event_fixture("enharmonic_double");
    assert!(enharmonic.contains("\"E--4\""));
    assert!(enharmonic.contains("\"G-4\""));
    assert!(enharmonic.contains("\"F##4\""));
}

#[test]
fn deterministic_serialization_across_runs() {
    let first = render_grid_fixture("dense_poly");
    let second = render_grid_fixture("dense_poly");
    let third = render_grid_fixture("dense_poly");
    assert_eq!(first, second);
    assert_eq!(second, third);
}

#[test]
#[ignore]
fn dump_augnet_snapshots_for_bootstrap() {
    for fixture in [
        "tied_barlines",
        "pickup_anacrusis",
        "dense_poly",
        "enharmonic_double",
    ] {
        println!("--- {fixture} EVENT ---\n{}", render_event_fixture(fixture));
        println!("--- {fixture} GRID ---\n{}", render_grid_fixture(fixture));
    }
}
