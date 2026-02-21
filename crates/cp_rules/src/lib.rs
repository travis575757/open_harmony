use cp_core::{
    interval_pc, is_consonant, is_perfect, note_location, ticks_per_measure, AnalysisDiagnostic,
    NoteEvent, NormalizedScore, PresetId, RuleId, Severity,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct RuleContext<'a> {
    pub score: &'a NormalizedScore,
    pub preset_id: &'a PresetId,
    pub rule_params: &'a BTreeMap<RuleId, Value>,
}

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn evaluate(&self, ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleParamIssue {
    pub rule_id: RuleId,
    pub field_path: String,
    pub reason: String,
    pub expected: String,
    pub actual: Option<String>,
}

impl RuleParamIssue {
    fn new(
        rule_id: &str,
        field_path: &str,
        reason: &str,
        expected: &str,
        actual: Option<String>,
    ) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            field_path: field_path.to_string(),
            reason: reason.to_string(),
            expected: expected.to_string(),
            actual,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PairMode {
    AllPairs,
    SelectedPairs,
    OuterVoices,
}

impl Default for PairMode {
    fn default() -> Self {
        Self::AllPairs
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ContraryObliqueParams {
    pair_mode: PairMode,
    selected_pairs: Vec<[u8; 2]>,
    similar_motion_ratio_max: f32,
    min_observations: u32,
}

impl Default for ContraryObliqueParams {
    fn default() -> Self {
        Self {
            pair_mode: PairMode::AllPairs,
            selected_pairs: Vec::new(),
            similar_motion_ratio_max: 0.7,
            min_observations: 1,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PostLeapCompParams {
    large_leap_min_semitones: i16,
    compensation_max_semitones: i16,
    required_contrary: bool,
    lookahead_notes: u8,
}

impl Default for PostLeapCompParams {
    fn default() -> Self {
        Self {
            large_leap_min_semitones: 5,
            compensation_max_semitones: 2,
            required_contrary: true,
            lookahead_notes: 1,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MaxLeapParams {
    max_leap_semitones: i16,
}

impl Default for MaxLeapParams {
    fn default() -> Self {
        Self {
            max_leap_semitones: 12,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ConsecutiveLargeLeapParams {
    large_leap_min_semitones: i16,
    same_direction_only: bool,
}

impl Default for ConsecutiveLargeLeapParams {
    fn default() -> Self {
        Self {
            large_leap_min_semitones: 5,
            same_direction_only: true,
        }
    }
}

fn parse_rule_params<T: DeserializeOwned>(
    rule_id: &str,
    value: &Value,
    expected: &str,
) -> Result<T, RuleParamIssue> {
    serde_json::from_value(value.clone()).map_err(|e| {
        RuleParamIssue::new(
            rule_id,
            "$",
            "invalid_json_shape",
            expected,
            Some(e.to_string()),
        )
    })
}

fn rule_params_or_default<T: DeserializeOwned + Default>(ctx: &RuleContext<'_>, rule_id: &str) -> T {
    ctx.rule_params
        .get(rule_id)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn validate_contrary_params(
    rule_id: &str,
    p: &ContraryObliqueParams,
    voice_count: usize,
) -> Vec<RuleParamIssue> {
    let mut issues = Vec::new();
    if !(0.0..=1.0).contains(&p.similar_motion_ratio_max) {
        issues.push(RuleParamIssue::new(
            rule_id,
            "similar_motion_ratio_max",
            "out_of_range",
            "value in [0.0, 1.0]",
            Some(p.similar_motion_ratio_max.to_string()),
        ));
    }
    if p.min_observations == 0 {
        issues.push(RuleParamIssue::new(
            rule_id,
            "min_observations",
            "out_of_range",
            "value >= 1",
            Some(p.min_observations.to_string()),
        ));
    }
    if p.pair_mode == PairMode::SelectedPairs && p.selected_pairs.is_empty() {
        issues.push(RuleParamIssue::new(
            rule_id,
            "selected_pairs",
            "missing_required",
            "at least one pair when pair_mode=selected_pairs",
            Some("[]".to_string()),
        ));
    }
    for (idx, pair) in p.selected_pairs.iter().enumerate() {
        if pair[0] == pair[1] {
            issues.push(RuleParamIssue::new(
                rule_id,
                &format!("selected_pairs[{}]", idx),
                "invalid_pair",
                "two distinct voice indices",
                Some(format!("[{},{}]", pair[0], pair[1])),
            ));
        }
        if pair[0] as usize >= voice_count || pair[1] as usize >= voice_count {
            issues.push(RuleParamIssue::new(
                rule_id,
                &format!("selected_pairs[{}]", idx),
                "index_out_of_range",
                &format!("voice index < {}", voice_count),
                Some(format!("[{},{}]", pair[0], pair[1])),
            ));
        }
    }
    issues
}

fn validate_post_leap_params(rule_id: &str, p: &PostLeapCompParams) -> Vec<RuleParamIssue> {
    let mut issues = Vec::new();
    if p.large_leap_min_semitones <= 0 {
        issues.push(RuleParamIssue::new(
            rule_id,
            "large_leap_min_semitones",
            "out_of_range",
            "value >= 1",
            Some(p.large_leap_min_semitones.to_string()),
        ));
    }
    if p.compensation_max_semitones <= 0 {
        issues.push(RuleParamIssue::new(
            rule_id,
            "compensation_max_semitones",
            "out_of_range",
            "value >= 1",
            Some(p.compensation_max_semitones.to_string()),
        ));
    }
    if p.lookahead_notes != 1 {
        issues.push(RuleParamIssue::new(
            rule_id,
            "lookahead_notes",
            "unsupported_value",
            "value == 1 (extended lookahead reserved for future phase)",
            Some(p.lookahead_notes.to_string()),
        ));
    }
    issues
}

fn validate_max_leap_params(rule_id: &str, p: &MaxLeapParams) -> Vec<RuleParamIssue> {
    if p.max_leap_semitones <= 0 {
        return vec![RuleParamIssue::new(
            rule_id,
            "max_leap_semitones",
            "out_of_range",
            "value >= 1",
            Some(p.max_leap_semitones.to_string()),
        )];
    }
    Vec::new()
}

fn validate_consecutive_leap_params(
    rule_id: &str,
    p: &ConsecutiveLargeLeapParams,
) -> Vec<RuleParamIssue> {
    if p.large_leap_min_semitones <= 0 {
        return vec![RuleParamIssue::new(
            rule_id,
            "large_leap_min_semitones",
            "out_of_range",
            "value >= 1",
            Some(p.large_leap_min_semitones.to_string()),
        )];
    }
    Vec::new()
}

pub fn validate_rule_params<'a, I>(
    active_rule_ids: I,
    rule_params: &BTreeMap<RuleId, Value>,
    voice_count: usize,
) -> Result<(), Vec<RuleParamIssue>>
where
    I: IntoIterator<Item = &'a RuleId>,
{
    let active: HashSet<&str> = active_rule_ids.into_iter().map(|s| s.as_str()).collect();
    let mut issues = Vec::new();
    for (rule_id, value) in rule_params {
        if !active.contains(rule_id.as_str()) {
            continue;
        }
        match rule_id.as_str() {
            "gen.motion.contrary_and_oblique_preferred" => {
                match parse_rule_params::<ContraryObliqueParams>(
                    rule_id,
                    value,
                    "object {pair_mode,selected_pairs,similar_motion_ratio_max,min_observations}",
                ) {
                    Ok(p) => issues.extend(validate_contrary_params(rule_id, &p, voice_count)),
                    Err(i) => issues.push(i),
                }
            }
            "gen.melody.post_leap_compensation_required" => {
                match parse_rule_params::<PostLeapCompParams>(
                    rule_id,
                    value,
                    "object {large_leap_min_semitones,compensation_max_semitones,required_contrary,lookahead_notes}",
                ) {
                    Ok(p) => issues.extend(validate_post_leap_params(rule_id, &p)),
                    Err(i) => issues.push(i),
                }
            }
            "gen.melody.max_leap_octave" => match parse_rule_params::<MaxLeapParams>(
                rule_id,
                value,
                "object {max_leap_semitones}",
            ) {
                Ok(p) => issues.extend(validate_max_leap_params(rule_id, &p)),
                Err(i) => issues.push(i),
            },
            "gen.melody.consecutive_large_leaps_restricted" => {
                match parse_rule_params::<ConsecutiveLargeLeapParams>(
                    rule_id,
                    value,
                    "object {large_leap_min_semitones,same_direction_only}",
                ) {
                    Ok(p) => issues.extend(validate_consecutive_leap_params(rule_id, &p)),
                    Err(i) => issues.push(i),
                }
            }
            _ => issues.push(RuleParamIssue::new(
                rule_id,
                "$",
                "unsupported_rule_param",
                "rule has no configurable parameters",
                Some(value.to_string()),
            )),
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

fn diag(
    rule_id: &str,
    severity: Severity,
    msg: impl Into<String>,
    score: &NormalizedScore,
    note: &NoteEvent,
    related: Option<&NoteEvent>,
) -> AnalysisDiagnostic {
    AnalysisDiagnostic {
        rule_id: rule_id.to_string(),
        severity,
        message: msg.into(),
        primary: note_location(note, score),
        related: related.map(|n| note_location(n, score)),
        context: BTreeMap::new(),
    }
}

fn tpq(score: &NormalizedScore) -> u32 {
    if score.meta.ticks_per_quarter == 0 {
        480
    } else {
        score.meta.ticks_per_quarter
    }
}

fn tpm(score: &NormalizedScore) -> u32 {
    ticks_per_measure(&score.meta.time_signature, tpq(score)).max(1)
}

fn beat_ticks(score: &NormalizedScore) -> u32 {
    (tpm(score) / score.meta.time_signature.numerator as u32).max(1)
}

fn leading_tone_pc(score: &NormalizedScore) -> u8 {
    (score.meta.key_signature.tonic_pc + 11) % 12
}

fn pair_notes(score: &NormalizedScore) -> Option<(&[NoteEvent], &[NoteEvent])> {
    if score.voices.len() < 2 {
        return None;
    }
    Some((&score.voices[0].notes, &score.voices[1].notes))
}

fn active_note_at(voice: &[NoteEvent], tick: u32) -> Option<&NoteEvent> {
    voice
        .iter()
        .find(|n| n.start_tick <= tick && tick < n.start_tick + n.duration_ticks)
}

fn all_start_ticks(score: &NormalizedScore) -> Vec<u32> {
    let mut starts: Vec<u32> = score
        .voices
        .iter()
        .flat_map(|v| v.notes.iter().map(|n| n.start_tick))
        .collect();
    starts.sort_unstable();
    starts.dedup();
    starts
}

fn sample_two_voice(score: &NormalizedScore) -> Vec<(u32, &NoteEvent, &NoteEvent)> {
    let mut out = Vec::new();
    let Some((a, b)) = pair_notes(score) else {
        return out;
    };
    for tick in all_start_ticks(score) {
        if let (Some(na), Some(nb)) = (active_note_at(a, tick), active_note_at(b, tick)) {
            out.push((tick, na, nb));
        }
    }
    out
}

fn is_downbeat(score: &NormalizedScore, tick: u32) -> bool {
    tick % tpm(score) == 0
}

fn is_weak_eighth_position(score: &NormalizedScore, tick: u32) -> bool {
    let bt = beat_ticks(score);
    let off = tick % bt;
    off == tpq(score) / 2
}

fn species_number(preset: &PresetId) -> Option<u8> {
    match preset {
        PresetId::Species1 => Some(1),
        PresetId::Species2 => Some(2),
        PresetId::Species3 => Some(3),
        PresetId::Species4 => Some(4),
        PresetId::Species5 => Some(5),
        _ => None,
    }
}

fn detect_quality_root(pcs: &[u8]) -> (Option<u8>, Option<&'static str>) {
    if pcs.is_empty() {
        return (None, None);
    }
    for &root in pcs {
        let mut ints: Vec<u8> = pcs.iter().map(|pc| interval_pc(*pc as i16, root as i16)).collect();
        ints.sort_unstable();
        if ints.contains(&4) && ints.contains(&7) && ints.contains(&10) {
            return (Some(root), Some("dominant7"));
        }
        if ints.contains(&4) && ints.contains(&7) {
            return (Some(root), Some("major"));
        }
        if ints.contains(&3) && ints.contains(&7) {
            return (Some(root), Some("minor"));
        }
        if ints.contains(&3) && ints.contains(&6) {
            return (Some(root), Some("diminished"));
        }
    }
    (Some(pcs[0]), Some("other"))
}

fn inversion_from_bass(root: u8, bass_pc: u8) -> &'static str {
    let i = interval_pc(bass_pc as i16, root as i16);
    match i {
        0 => "root",
        3 | 4 => "first",
        6 | 7 | 8 => "second",
        10 | 11 => "third",
        _ => "other",
    }
}

pub struct IdRule {
    pub id: &'static str,
    pub eval: fn(&RuleContext<'_>) -> Vec<AnalysisDiagnostic>,
}

impl Rule for IdRule {
    fn id(&self) -> &'static str {
        self.id
    }

    fn evaluate(&self, ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
        (self.eval)(ctx)
    }
}

fn r_gen_input_single_exercise(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if ctx.score.meta.exercise_count == 1 {
        return Vec::new();
    }
    let Some(note) = ctx.score.voices.iter().flat_map(|v| v.notes.iter()).next() else {
        return Vec::new();
    };
    vec![diag(
        "gen.input.single_exercise_per_file",
        Severity::Error,
        "exercise_count must be 1",
        ctx.score,
        note,
        None,
    )]
}

fn r_gen_input_key_sig(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if ctx.score.meta.key_signature.tonic_pc < 12 {
        return Vec::new();
    }
    let Some(note) = ctx.score.voices.iter().flat_map(|v| v.notes.iter()).next() else {
        return Vec::new();
    };
    vec![diag(
        "gen.input.key_signature_required_and_stable",
        Severity::Error,
        "key signature tonic_pc must be in [0,11]",
        ctx.score,
        note,
        None,
    )]
}

fn r_gen_input_timesig(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let ts = &ctx.score.meta.time_signature;
    let allowed = [
        (2, 4),
        (3, 4),
        (4, 4),
        (2, 2),
        (5, 4),
        (6, 4),
        (3, 2),
    ];
    let mut out = Vec::new();
    if !allowed.contains(&(ts.numerator, ts.denominator)) {
        if let Some(note) = ctx.score.voices.iter().flat_map(|v| v.notes.iter()).next() {
            out.push(diag(
                "gen.input.timesig_supported",
                Severity::Error,
                format!("unsupported time signature {}/{}", ts.numerator, ts.denominator),
                ctx.score,
                note,
                None,
            ));
        }
        return out;
    }
    if let Some(sp) = species_number(ctx.preset_id) {
        if sp == 5 && ![(4, 4), (2, 2)].contains(&(ts.numerator, ts.denominator)) {
            if let Some(note) = ctx.score.voices.iter().flat_map(|v| v.notes.iter()).next() {
                out.push(diag(
                    "gen.input.timesig_supported",
                    Severity::Error,
                    "species 5 supports only 4/4 or 2/2",
                    ctx.score,
                    note,
                    None,
                ));
            }
        }
    }
    out
}

fn r_gen_input_species_supported(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if matches!(
        ctx.preset_id,
        PresetId::Species1
            | PresetId::Species2
            | PresetId::Species3
            | PresetId::Species4
            | PresetId::Species5
            | PresetId::GeneralVoiceLeading
            | PresetId::Custom
    ) {
        return Vec::new();
    }
    let Some(note) = ctx.score.voices.iter().flat_map(|v| v.notes.iter()).next() else {
        return Vec::new();
    };
    vec![diag(
        "gen.input.species_supported",
        Severity::Error,
        "unsupported preset/species",
        ctx.score,
        note,
        None,
    )]
}

fn r_gen_input_voice_count(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if ctx.score.voices.len() <= 4 {
        return Vec::new();
    }
    let Some(note) = ctx.score.voices.iter().flat_map(|v| v.notes.iter()).next() else {
        return Vec::new();
    };
    vec![diag(
        "gen.input.voice_count_supported",
        Severity::Error,
        format!("voice count {} exceeds max 4", ctx.score.voices.len()),
        ctx.score,
        note,
        None,
    )]
}

fn r_gen_input_min_note_len(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let min_len = tpq(ctx.score) / 2;
    let mut out = Vec::new();
    for v in &ctx.score.voices {
        for n in &v.notes {
            if n.duration_ticks < min_len {
                out.push(diag(
                    "gen.input_min_note_length.eighth_or_longer",
                    Severity::Error,
                    "duration shorter than eighth note is unsupported",
                    ctx.score,
                    n,
                    None,
                ));
            }
        }
    }
    out
}

fn r_gen_harmony_supported_sonorities(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for tick in all_start_ticks(ctx.score) {
        let mut pcs = Vec::new();
        let mut first: Option<&NoteEvent> = None;
        for v in &ctx.score.voices {
            if let Some(n) = active_note_at(&v.notes, tick) {
                if first.is_none() {
                    first = Some(n);
                }
                pcs.push(n.midi.rem_euclid(12) as u8);
            }
        }
        pcs.sort_unstable();
        pcs.dedup();
        if pcs.len() > 4 {
            if let Some(n) = first {
                out.push(diag(
                    "gen.harmony.supported_sonorities",
                    Severity::Error,
                    "only triads/seventh-chord pitch class sets are supported",
                    ctx.score,
                    n,
                    None,
                ));
            }
        }
    }
    out
}

fn r_parallel_perfects(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for i in 0..ctx.score.voices.len() {
        for j in (i + 1)..ctx.score.voices.len() {
            let va = &ctx.score.voices[i].notes;
            let vb = &ctx.score.voices[j].notes;
            let min_len = va.len().min(vb.len());
            if min_len < 2 {
                continue;
            }
            for k in 1..min_len {
                let prev = interval_pc(va[k - 1].midi, vb[k - 1].midi);
                let now = interval_pc(va[k].midi, vb[k].midi);
                if is_perfect(prev)
                    && is_perfect(now)
                    && prev == now
                    && va[k].midi != va[k - 1].midi
                    && vb[k].midi != vb[k - 1].midi
                {
                    out.push(diag(
                        "gen.motion.parallel_perfects_forbidden",
                        Severity::Error,
                        "parallel perfect interval detected",
                        ctx.score,
                        &va[k],
                        Some(&vb[k]),
                    ));
                }
            }
        }
    }
    out
}

fn r_direct_perfects(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((upper, lower)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for k in 1..upper.len().min(lower.len()) {
        let now = interval_pc(upper[k].midi, lower[k].midi);
        if !is_perfect(now) {
            continue;
        }
        let du = upper[k].midi - upper[k - 1].midi;
        let dl = lower[k].midi - lower[k - 1].midi;
        let similar = (du > 0 && dl > 0) || (du < 0 && dl < 0);
        if similar && du.abs() > 2 {
            out.push(diag(
                "gen.motion.direct_perfects_restricted",
                Severity::Warning,
                "direct perfect interval approach in similar motion",
                ctx.score,
                &upper[k],
                Some(&lower[k]),
            ));
        }
    }
    out
}

fn r_spacing_upper_octave(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for tick in all_start_ticks(ctx.score) {
        for i in 0..ctx.score.voices.len().saturating_sub(1) {
            let Some(na) = active_note_at(&ctx.score.voices[i].notes, tick) else {
                continue;
            };
            let Some(nb) = active_note_at(&ctx.score.voices[i + 1].notes, tick) else {
                continue;
            };
            if i + 1 < 3 && (na.midi - nb.midi).abs() > 12 {
                out.push(diag(
                    "gen.spacing.upper_adjacent_max_octave",
                    Severity::Warning,
                    "adjacent upper voices exceed octave spacing",
                    ctx.score,
                    na,
                    Some(nb),
                ));
            }
        }
    }
    out
}

fn r_spacing_tenor_bass(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if ctx.score.voices.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let last = ctx.score.voices.len() - 1;
    let ten = last - 1;
    for tick in all_start_ticks(ctx.score) {
        let Some(nt) = active_note_at(&ctx.score.voices[ten].notes, tick) else {
            continue;
        };
        let Some(nb) = active_note_at(&ctx.score.voices[last].notes, tick) else {
            continue;
        };
        if (nt.midi - nb.midi).abs() > 19 {
            out.push(diag(
                "gen.spacing.tenor_bass_max_twelfth",
                Severity::Warning,
                "tenor-bass spacing exceeds twelfth",
                ctx.score,
                nt,
                Some(nb),
            ));
        }
    }
    out
}

fn r_voice_crossing_overlap(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for tick in all_start_ticks(ctx.score) {
        for i in 0..ctx.score.voices.len().saturating_sub(1) {
            let Some(upper) = active_note_at(&ctx.score.voices[i].notes, tick) else {
                continue;
            };
            let Some(lower) = active_note_at(&ctx.score.voices[i + 1].notes, tick) else {
                continue;
            };
            if upper.midi <= lower.midi {
                out.push(diag(
                    "gen.voice_crossing_and_overlap.restricted",
                    Severity::Error,
                    "voice crossing detected",
                    ctx.score,
                    upper,
                    Some(lower),
                ));
            }
        }
    }
    out
}

fn r_unison_interior(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let samples = sample_two_voice(ctx.score);
    if samples.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (_, a, b) in samples.iter().skip(1).take(samples.len() - 2) {
        if a.midi == b.midi {
            out.push(diag(
                "gen.unison.interior_restricted",
                Severity::Warning,
                "interior unison is discouraged",
                ctx.score,
                a,
                Some(b),
            ));
        }
    }
    out
}

fn r_melodic_max_leap(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let params: MaxLeapParams = rule_params_or_default(ctx, "gen.melody.max_leap_octave");
    let mut out = Vec::new();
    for v in &ctx.score.voices {
        for w in v.notes.windows(2) {
            if (w[1].midi - w[0].midi).abs() > params.max_leap_semitones {
                out.push(diag(
                    "gen.melody.max_leap_octave",
                    Severity::Error,
                    "melodic leap greater than octave",
                    ctx.score,
                    &w[1],
                    Some(&w[0]),
                ));
            }
        }
    }
    out
}

fn r_melodic_dissonant_leaps(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for v in &ctx.score.voices {
        for w in v.notes.windows(2) {
            let d = (w[1].midi - w[0].midi).abs();
            if matches!(d, 6 | 10 | 11) {
                out.push(diag(
                    "gen.melody.dissonant_leaps_forbidden",
                    Severity::Error,
                    "dissonant melodic leap",
                    ctx.score,
                    &w[1],
                    Some(&w[0]),
                ));
            }
        }
    }
    out
}

fn r_post_leap_compensation(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let params: PostLeapCompParams =
        rule_params_or_default(ctx, "gen.melody.post_leap_compensation_required");
    let mut out = Vec::new();
    for v in &ctx.score.voices {
        for w in v.notes.windows(3) {
            let d1 = w[1].midi - w[0].midi;
            let d2 = w[2].midi - w[1].midi;
            let large_leap = d1.abs() >= params.large_leap_min_semitones;
            let step_ok = d2.abs() <= params.compensation_max_semitones;
            let dir_ok = !params.required_contrary || d1.signum() == -d2.signum();
            if large_leap && !(step_ok && dir_ok) {
                out.push(diag(
                    "gen.melody.post_leap_compensation_required",
                    Severity::Warning,
                    "large leap should be compensated by contrary step",
                    ctx.score,
                    &w[1],
                    Some(&w[2]),
                ));
            }
        }
    }
    out
}

fn r_single_climax(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for v in &ctx.score.voices {
        let Some(max) = v.notes.iter().map(|n| n.midi).max() else {
            continue;
        };
        let hits: Vec<&NoteEvent> = v.notes.iter().filter(|n| n.midi == max).collect();
        if hits.len() > 1 {
            out.push(diag(
                "gen.melody.single_climax_preferred",
                Severity::Warning,
                "multiple highest-note climaxes detected",
                ctx.score,
                hits[1],
                Some(hits[0]),
            ));
        }
    }
    out
}

fn r_consecutive_large_leaps(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let params: ConsecutiveLargeLeapParams =
        rule_params_or_default(ctx, "gen.melody.consecutive_large_leaps_restricted");
    let mut out = Vec::new();
    for v in &ctx.score.voices {
        for w in v.notes.windows(3) {
            let d1 = w[1].midi - w[0].midi;
            let d2 = w[2].midi - w[1].midi;
            let both_large =
                d1.abs() >= params.large_leap_min_semitones
                    && d2.abs() >= params.large_leap_min_semitones;
            let dir_ok = if params.same_direction_only {
                d1.signum() == d2.signum()
            } else {
                true
            };
            if both_large && dir_ok {
                out.push(diag(
                    "gen.melody.consecutive_large_leaps_restricted",
                    Severity::Warning,
                    "consecutive large leaps in same direction",
                    ctx.score,
                    &w[2],
                    Some(&w[1]),
                ));
            }
        }
    }
    out
}

fn r_contrary_oblique_preferred(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if ctx.score.voices.len() < 2 {
        return Vec::new();
    }
    let params: ContraryObliqueParams =
        rule_params_or_default(ctx, "gen.motion.contrary_and_oblique_preferred");

    let pairs: Vec<(usize, usize)> = match params.pair_mode {
        PairMode::AllPairs => {
            let mut v = Vec::new();
            for i in 0..ctx.score.voices.len() {
                for j in (i + 1)..ctx.score.voices.len() {
                    v.push((i, j));
                }
            }
            v
        }
        PairMode::OuterVoices => vec![(0, ctx.score.voices.len() - 1)],
        PairMode::SelectedPairs => params
            .selected_pairs
            .iter()
            .filter_map(|p| {
                let i = p[0] as usize;
                let j = p[1] as usize;
                if i < ctx.score.voices.len() && j < ctx.score.voices.len() && i != j {
                    if i < j {
                        Some((i, j))
                    } else {
                        Some((j, i))
                    }
                } else {
                    None
                }
            })
            .collect(),
    };

    let mut out = Vec::new();
    for (vi, vj) in pairs {
        let a = &ctx.score.voices[vi].notes;
        let b = &ctx.score.voices[vj].notes;
        let min_len = a.len().min(b.len());
        if min_len < 2 {
            continue;
        }
        let mut similar = 0usize;
        let mut total = 0usize;
        for i in 1..min_len {
            let da = a[i].midi - a[i - 1].midi;
            let db = b[i].midi - b[i - 1].midi;
            if da == 0 && db == 0 {
                continue;
            }
            total += 1;
            if (da > 0 && db > 0) || (da < 0 && db < 0) {
                similar += 1;
            }
        }
        if total < params.min_observations as usize {
            continue;
        }
        let ratio = similar as f32 / total as f32;
        if ratio <= params.similar_motion_ratio_max {
            continue;
        }
        let mut d = diag(
            "gen.motion.contrary_and_oblique_preferred",
            Severity::Warning,
            "excessive similar motion between selected voices",
            ctx.score,
            &a[min_len - 1],
            Some(&b[min_len - 1]),
        );
        d.context
            .insert("pair".to_string(), format!("{}-{}", vi, vj));
        d.context
            .insert("similar_count".to_string(), similar.to_string());
        d.context.insert("total_count".to_string(), total.to_string());
        d.context.insert("ratio".to_string(), format!("{:.4}", ratio));
        d.context.insert(
            "threshold".to_string(),
            params.similar_motion_ratio_max.to_string(),
        );
        out.push(d);
    }
    out
}

fn r_final_perfect_cadence(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let samples = sample_two_voice(ctx.score);
    let Some((_, a, b)) = samples.last() else {
        return Vec::new();
    };
    let pc = interval_pc(a.midi, b.midi);
    if is_perfect(pc) {
        return Vec::new();
    }
    vec![diag(
        "gen.cadence.final_perfect_consonance_required",
        Severity::Error,
        "final sonority must be perfect",
        ctx.score,
        a,
        Some(b),
    )]
}

fn r_p4_against_bass(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if ctx.score.voices.len() != 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (_, a, b) in sample_two_voice(ctx.score) {
        if interval_pc(a.midi, b.midi) == 5 {
            out.push(diag(
                "gen.interval.p4_dissonant_against_bass_in_two_voice",
                Severity::Error,
                "perfect fourth against bass is dissonant in strict two-voice context",
                ctx.score,
                a,
                Some(b),
            ));
        }
    }
    out
}

fn r_leading_tone_not_doubled(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let lt = leading_tone_pc(ctx.score);
    let mut out = Vec::new();
    for tick in all_start_ticks(ctx.score) {
        let notes: Vec<&NoteEvent> = ctx
            .score
            .voices
            .iter()
            .filter_map(|v| active_note_at(&v.notes, tick))
            .collect();
        let mut ltn = Vec::new();
        for n in notes {
            if n.midi.rem_euclid(12) as u8 == lt {
                ltn.push(n);
            }
        }
        if ltn.len() >= 2 {
            out.push(diag(
                "gen.voice.leading_tone_not_doubled",
                Severity::Error,
                "leading tone doubled",
                ctx.score,
                ltn[0],
                Some(ltn[1]),
            ));
        }
    }
    out
}

fn r_leading_tone_resolves_up(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let lt = leading_tone_pc(ctx.score);
    let mut out = Vec::new();
    for v in &ctx.score.voices {
        for w in v.notes.windows(2) {
            if w[0].midi.rem_euclid(12) as u8 == lt {
                let d = w[1].midi - w[0].midi;
                if !(d == 1 || d == 2) {
                    out.push(diag(
                        "gen.voice.leading_tone_resolves_up",
                        Severity::Warning,
                        "leading tone should resolve upward by step",
                        ctx.score,
                        &w[0],
                        Some(&w[1]),
                    ));
                }
            }
        }
    }
    out
}

fn r_chordal_seventh_resolves_down(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for v in &ctx.score.voices {
        for w in v.notes.windows(2) {
            // Heuristic: treat interval of m7/M7 above current bass proxy (tonic) as chordal seventh.
            let i = interval_pc(w[0].midi, ctx.score.meta.key_signature.tonic_pc as i16 + 60);
            if i == 10 || i == 11 {
                let d = w[1].midi - w[0].midi;
                if !(d == -1 || d == -2) {
                    out.push(diag(
                        "gen.voice.chordal_seventh_resolves_down",
                        Severity::Error,
                        "chordal seventh should resolve downward by step",
                        ctx.score,
                        &w[0],
                        Some(&w[1]),
                    ));
                }
            }
        }
    }
    out
}

fn tonal_chord_checks<'a>(
    ctx: &'a RuleContext<'a>,
) -> Vec<(
    u32,
    Vec<&'a NoteEvent>,
    u8,
    Option<u8>,
    Option<&'static str>,
    &'static str,
)> {
    let mut out = Vec::new();
    for tick in all_start_ticks(ctx.score) {
        let notes: Vec<&NoteEvent> = ctx
            .score
            .voices
            .iter()
            .filter_map(|v| active_note_at(&v.notes, tick))
            .collect();
        if notes.is_empty() {
            continue;
        }
        let bass_pc = notes.iter().map(|n| n.midi).min().unwrap_or(60).rem_euclid(12) as u8;
        let mut pcs: Vec<u8> = notes.iter().map(|n| n.midi.rem_euclid(12) as u8).collect();
        pcs.sort_unstable();
        pcs.dedup();
        let (root, q) = detect_quality_root(&pcs);
        let inv = root.map(|r| inversion_from_bass(r, bass_pc)).unwrap_or("other");
        out.push((tick, notes, bass_pc, root, q, inv));
    }
    out
}

fn r_double_root_pref(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (_tick, notes, _bass, root, _q, inv) in tonal_chord_checks(ctx) {
        if inv != "root" {
            continue;
        }
        let Some(root) = root else {
            continue;
        };
        let cnt = notes
            .iter()
            .filter(|n| n.midi.rem_euclid(12) as u8 == root)
            .count();
        if cnt < 2 {
            out.push(diag(
                "gen.doubling.root_position_prefers_root",
                Severity::Warning,
                "root position chord should preferably double the root",
                ctx.score,
                notes[0],
                None,
            ));
        }
    }
    out
}

fn r_first_inv_no_bass_double(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (_tick, notes, bass_pc, _root, quality, inv) in tonal_chord_checks(ctx) {
        if inv != "first" {
            continue;
        }
        let cnt = notes
            .iter()
            .filter(|n| n.midi.rem_euclid(12) as u8 == bass_pc)
            .count();
        if cnt > 1 && quality != Some("diminished") {
            out.push(diag(
                "gen.doubling.first_inversion_no_bass_double_default",
                Severity::Warning,
                "first inversion generally avoids bass doubling",
                ctx.score,
                notes[0],
                None,
            ));
        }
    }
    out
}

fn r_dim_first_inv_double_third(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (_tick, notes, bass_pc, _root, quality, inv) in tonal_chord_checks(ctx) {
        if inv == "first" && quality == Some("diminished") {
            let cnt = notes
                .iter()
                .filter(|n| n.midi.rem_euclid(12) as u8 == bass_pc)
                .count();
            if cnt < 2 {
                out.push(diag(
                    "gen.doubling.diminished_first_inversion_double_third",
                    Severity::Error,
                    "diminished first inversion should double the third (bass)",
                    ctx.score,
                    notes[0],
                    None,
                ));
            }
        }
    }
    out
}

fn r_second_inv_double_bass(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (_tick, notes, bass_pc, _root, _quality, inv) in tonal_chord_checks(ctx) {
        if inv != "second" {
            continue;
        }
        let cnt = notes
            .iter()
            .filter(|n| n.midi.rem_euclid(12) as u8 == bass_pc)
            .count();
        if cnt < 2 {
            out.push(diag(
                "gen.doubling.second_inversion_double_bass",
                Severity::Error,
                "second inversion should double bass",
                ctx.score,
                notes[0],
                None,
            ));
        }
    }
    out
}

fn r_cadential_64(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let checks = tonal_chord_checks(ctx);
    let mut out = Vec::new();
    for w in checks.windows(2) {
        let (_t0, n0, _b0, _r0, _q0, inv0) = &w[0];
        let (_t1, _n1, _b1, _r1, _q1, inv1) = &w[1];
        if *inv0 == "second" && *inv1 == "second" {
            out.push(diag(
                "gen.cadence.cadential_64_resolves_65_43",
                Severity::Error,
                "cadential 6/4 should resolve away from second inversion",
                ctx.score,
                n0[0],
                None,
            ));
        }
    }
    out
}

fn r_nct_supported(_ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    // Non-chord-tone taxonomy is handled by harmony/NCT annotator and consumed by dedicated rules.
    Vec::new()
}

fn r_spacing_alias(_ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    Vec::new()
}

fn r_sp1_rhythm(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((a, b)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let min_len = a.len().min(b.len());
    let mut out = Vec::new();
    for i in 0..min_len {
        if a[i].duration_ticks != b[i].duration_ticks {
            out.push(diag(
                "sp1.rhythm.one_to_one_only",
                Severity::Error,
                "first species requires equal duration between paired notes",
                ctx.score,
                &a[i],
                Some(&b[i]),
            ));
        }
    }
    out
}

fn r_sp1_vertical(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (_tick, a, b) in sample_two_voice(ctx.score) {
        let pc = interval_pc(a.midi, b.midi);
        if !is_consonant(pc) {
            out.push(diag(
                "sp1.vertical.consonance_only",
                Severity::Error,
                "dissonant vertical interval in species 1",
                ctx.score,
                a,
                Some(b),
            ));
        }
    }
    out
}

fn r_sp1_opening(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((_, a, b)) = sample_two_voice(ctx.score).first().copied() else {
        return Vec::new();
    };
    if is_perfect(interval_pc(a.midi, b.midi)) {
        return Vec::new();
    }
    vec![diag(
        "sp1.opening.perfect_consonance_required",
        Severity::Error,
        "species 1 must begin with a perfect consonance",
        ctx.score,
        a,
        Some(b),
    )]
}

fn r_sp1_ending(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((_, a, b)) = sample_two_voice(ctx.score).last().copied() else {
        return Vec::new();
    };
    let pc = interval_pc(a.midi, b.midi);
    if pc == 0 {
        return Vec::new();
    }
    vec![diag(
        "sp1.ending.unison_or_octave_required",
        Severity::Error,
        "species 1 should end on unison/octave class",
        ctx.score,
        a,
        Some(b),
    )]
}

fn r_sp1_penultimate(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let samples = sample_two_voice(ctx.score);
    if samples.len() < 2 {
        return Vec::new();
    }
    let (_, a, b) = samples[samples.len() - 2];
    let pc = interval_pc(a.midi, b.midi);
    if matches!(pc, 3 | 4 | 8 | 9) {
        return Vec::new();
    }
    vec![diag(
        "sp1.cadence.penultimate_imperfect_consonance",
        Severity::Warning,
        "penultimate interval should be imperfect consonance",
        ctx.score,
        a,
        Some(b),
    )]
}

fn r_sp2_rhythm(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    if cp.len() == cf.len() * 2 {
        return Vec::new();
    }
    let Some(n) = cp.first() else { return Vec::new(); };
    vec![diag(
        "sp2.rhythm.two_to_one_only",
        Severity::Error,
        "species 2 expects 2:1 note ratio",
        ctx.score,
        n,
        None,
    )]
}

fn r_sp2_strong(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if is_downbeat(ctx.score, tick) && !is_consonant(interval_pc(a.midi, b.midi)) {
            out.push(diag(
                "sp2.strong_beat.consonance_required",
                Severity::Error,
                "species 2 downbeat must be consonant",
                ctx.score,
                a,
                Some(b),
            ));
        }
    }
    out
}

fn cp_note_neighbors(cp: &[NoteEvent], tick: u32) -> Option<(&NoteEvent, &NoteEvent, &NoteEvent)> {
    for i in 0..cp.len() {
        let n = &cp[i];
        if n.start_tick <= tick && tick < n.start_tick + n.duration_ticks {
            if i == 0 || i + 1 >= cp.len() {
                return None;
            }
            return Some((&cp[i - 1], &cp[i], &cp[i + 1]));
        }
    }
    None
}

fn r_sp2_weak_passing(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if is_downbeat(ctx.score, tick) {
            continue;
        }
        if is_consonant(interval_pc(a.midi, b.midi)) {
            continue;
        }
        if let Some((p, c, n)) = cp_note_neighbors(cp, tick) {
            let d1 = c.midi - p.midi;
            let d2 = n.midi - c.midi;
            if !(d1.abs() <= 2 && d2.abs() <= 2 && d1.signum() == d2.signum()) {
                out.push(diag(
                    "sp2.dissonance.weak_passing_stepwise",
                    Severity::Error,
                    "weak-beat dissonance must be passing and stepwise",
                    ctx.score,
                    c,
                    Some(cf.iter().find(|x| x.note_id == b.note_id).unwrap_or(b)),
                ));
            }
        }
    }
    out
}

fn r_sp2_downbeat_parallel(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let downs: Vec<(u32, &NoteEvent, &NoteEvent)> = sample_two_voice(ctx.score)
        .into_iter()
        .filter(|(tick, _, _)| is_downbeat(ctx.score, *tick))
        .collect();
    let mut out = Vec::new();
    for w in downs.windows(2) {
        let (_t0, a0, b0) = w[0];
        let (_t1, a1, b1) = w[1];
        let i0 = interval_pc(a0.midi, b0.midi);
        let i1 = interval_pc(a1.midi, b1.midi);
        if is_perfect(i0)
            && is_perfect(i1)
            && i0 == i1
            && a0.midi != a1.midi
            && b0.midi != b1.midi
        {
            out.push(diag(
                "sp2.structure.downbeat_skeleton_no_parallel_perfects",
                Severity::Error,
                "downbeat skeleton has parallel perfect intervals",
                ctx.score,
                a1,
                Some(b1),
            ));
        }
    }
    out
}

fn r_sp2_downbeat_unison(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if is_downbeat(ctx.score, tick) && a.midi == b.midi {
            out.push(diag(
                "sp2.downbeat_unison_discouraged",
                Severity::Warning,
                "species 2 downbeat unison is discouraged",
                ctx.score,
                a,
                Some(b),
            ));
        }
    }
    out
}

fn r_sp3_rhythm(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    if cp.len() == cf.len() * 4 {
        return Vec::new();
    }
    let Some(n) = cp.first() else { return Vec::new(); };
    vec![diag(
        "sp3.rhythm.four_to_one_only",
        Severity::Error,
        "species 3 expects 4:1 note ratio",
        ctx.score,
        n,
        None,
    )]
}

fn r_sp3_strong(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if is_downbeat(ctx.score, tick) && !is_consonant(interval_pc(a.midi, b.midi)) {
            out.push(diag(
                "sp3.strong_beat.consonance_required",
                Severity::Error,
                "species 3 beat 1 must be consonant",
                ctx.score,
                a,
                Some(b),
            ));
        }
    }
    out
}

fn r_sp3_patterns(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if is_downbeat(ctx.score, tick) || is_consonant(interval_pc(a.midi, b.midi)) {
            continue;
        }
        if let Some((p, c, n)) = cp_note_neighbors(cp, tick) {
            let d1 = c.midi - p.midi;
            let d2 = n.midi - c.midi;
            let passing_or_neighbor = d1.abs() <= 2 && d2.abs() <= 2;
            if !passing_or_neighbor {
                out.push(diag(
                    "sp3.dissonance.passing_neighbor_patterns_only",
                    Severity::Error,
                    "species 3 dissonance must be passing/neighbor pattern",
                    ctx.score,
                    c,
                    Some(b),
                ));
            }
        }
    }
    out
}

fn r_sp3_double_neighbor(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for w in cp.windows(4) {
        if w[0].midi == w[3].midi {
            continue;
        }
        if (w[1].midi - w[0].midi).abs() == 1 && (w[2].midi - w[0].midi).abs() == 1 {
            out.push(diag(
                "sp3.dissonance.double_neighbor_allowed_pattern",
                Severity::Warning,
                "possible malformed double-neighbor pattern",
                ctx.score,
                &w[1],
                Some(&w[2]),
            ));
        }
    }
    out
}

fn r_sp3_cambiata(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for w in cp.windows(5) {
        let d1 = w[1].midi - w[0].midi;
        let d2 = w[2].midi - w[1].midi;
        let d3 = w[3].midi - w[2].midi;
        let d4 = w[4].midi - w[3].midi;
        let looks_cambiata = d1 == -1 && d2.abs() >= 3 && d3 == 1 && d4 == 1;
        if d2.abs() >= 3 && !looks_cambiata {
            out.push(diag(
                "sp3.dissonance.cambiata_limited_exception",
                Severity::Warning,
                "leap-from-dissonance should follow cambiata schema",
                ctx.score,
                &w[2],
                Some(&w[3]),
            ));
        }
    }
    out
}

fn r_sp3_downbeat_unison(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if is_downbeat(ctx.score, tick) && a.midi == b.midi {
            out.push(diag(
                "sp3.downbeat_unison_forbidden",
                Severity::Warning,
                "species 3 downbeat unison is discouraged",
                ctx.score,
                a,
                Some(b),
            ));
        }
    }
    out
}

fn suspension_events<'a>(ctx: &'a RuleContext<'a>) -> Vec<(&'a NoteEvent, u32, &'a NoteEvent)> {
    let Some((cp, cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for n in cp {
        let end = n.start_tick + n.duration_ticks;
        if !n.tie_start || end % tpm(ctx.score) != 0 {
            continue;
        }
        if let Some(cf_note) = active_note_at(cf, end) {
            out.push((n, end, cf_note));
        }
    }
    out
}

fn r_sp4_syncopated(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    if cp.iter().any(|n| n.tie_start && (n.start_tick + n.duration_ticks) % tpm(ctx.score) == 0) {
        return Vec::new();
    }
    let Some(n) = cp.first() else { return Vec::new(); };
    vec![diag(
        "sp4.rhythm.syncopated_ligature_profile",
        Severity::Error,
        "species 4 expects syncopated ligatures across barlines",
        ctx.score,
        n,
        None,
    )]
}

fn r_sp4_prep(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (n, _end, cf) in suspension_events(ctx) {
        if !is_consonant(interval_pc(n.midi, cf.midi)) {
            out.push(diag(
                "sp4.suspension.preparation_required",
                Severity::Error,
                "suspension must be prepared by consonance",
                ctx.score,
                n,
                Some(cf),
            ));
        }
    }
    out
}

fn r_sp4_downbeat_dissonance(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    let events = suspension_events(ctx);
    let event_ticks: Vec<u32> = events.iter().map(|(_, end, _)| *end).collect();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if !is_downbeat(ctx.score, tick) {
            continue;
        }
        if is_consonant(interval_pc(a.midi, b.midi)) {
            continue;
        }
        if !event_ticks.contains(&tick) {
            out.push(diag(
                "sp4.suspension.downbeat_dissonance_allowed_only_if_suspension",
                Severity::Error,
                "downbeat dissonance must be a prepared suspension",
                ctx.score,
                a,
                Some(b),
            ));
        }
    }
    out
}

fn r_sp4_step_resolution(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (n, _end, cf) in suspension_events(ctx) {
        let Some(ix) = cp.iter().position(|x| x.note_id == n.note_id) else {
            continue;
        };
        if ix + 1 >= cp.len() {
            continue;
        }
        let d = cp[ix + 1].midi - cp[ix].midi;
        if !(d == -1 || d == -2) {
            out.push(diag(
                "sp4.suspension.step_resolution_required",
                Severity::Error,
                "suspension should resolve downward by step",
                ctx.score,
                &cp[ix],
                Some(cf),
            ));
        }
    }
    out
}

fn r_sp4_break_species(_ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    Vec::new()
}

fn r_sp4_allowed_classes(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (n, end, cf) in suspension_events(ctx) {
        let Some(cp_active) = pair_notes(ctx.score).and_then(|(cp, _)| active_note_at(cp, end)) else {
            continue;
        };
        let pc = interval_pc(cp_active.midi, cf.midi);
        if !matches!(pc, 2 | 5 | 10 | 11) {
            out.push(diag(
                "sp4.allowed_suspension_classes_enforced",
                Severity::Error,
                "suspension class is outside allowed set",
                ctx.score,
                n,
                Some(cf),
            ));
        }
    }
    out
}

fn r_sp4_afterbeat_parallel(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let weak: Vec<(u32, &NoteEvent, &NoteEvent)> = sample_two_voice(ctx.score)
        .into_iter()
        .filter(|(tick, _, _)| !is_downbeat(ctx.score, *tick))
        .collect();
    let mut out = Vec::new();
    for w in weak.windows(2) {
        let (_t0, a0, b0) = w[0];
        let (_t1, a1, b1) = w[1];
        let i0 = interval_pc(a0.midi, b0.midi);
        let i1 = interval_pc(a1.midi, b1.midi);
        if is_perfect(i0) && is_perfect(i1) && i0 == i1 {
            out.push(diag(
                "sp4.afterbeat_parallel_guard",
                Severity::Error,
                "after-beat parallel perfect intervals detected",
                ctx.score,
                a1,
                Some(b1),
            ));
        }
    }
    out
}

fn r_sp4_all_voices_syncopation(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let tpm = tpm(ctx.score);
    let all_sync = ctx.score.voices.iter().all(|v| {
        v.notes
            .iter()
            .any(|n| n.tie_start && (n.start_tick + n.duration_ticks) % tpm == 0)
    });
    if !all_sync || ctx.score.voices.is_empty() {
        return Vec::new();
    }
    let n = &ctx.score.voices[0].notes[0];
    vec![diag(
        "sp4.all_voices_syncopation_avoidance",
        Severity::Warning,
        "all voices are syncopated; keep at least one beat-articulating voice",
        ctx.score,
        n,
        None,
    )]
}

fn r_sp5_rhythm_mixed(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut lens: Vec<u32> = cp.iter().map(|n| n.duration_ticks).collect();
    lens.sort_unstable();
    lens.dedup();
    if lens.len() >= 2 {
        return Vec::new();
    }
    let Some(n) = cp.first() else { return Vec::new(); };
    vec![diag(
        "sp5.rhythm.mixed_species_profile",
        Severity::Error,
        "species 5 should use mixed rhythmic values",
        ctx.score,
        n,
        None,
    )]
}

fn r_sp5_strong(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if !is_downbeat(ctx.score, tick) {
            continue;
        }
        if is_consonant(interval_pc(a.midi, b.midi)) {
            continue;
        }
        let is_susp = active_note_at(cp, tick)
            .map(|n| n.tie_start || n.tie_end)
            .unwrap_or(false);
        if !is_susp {
            out.push(diag(
                "sp5.strong_beat.consonance_or_prepared_suspension_only",
                Severity::Error,
                "strong-beat dissonance must be suspension-licensed",
                ctx.score,
                a,
                Some(b),
            ));
        }
    }
    out
}

fn r_sp5_eighth_weak(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let eighth = tpq(ctx.score) / 2;
    let mut out = Vec::new();
    for i in 0..cp.len() {
        let n = &cp[i];
        if n.duration_ticks != eighth {
            continue;
        }
        if !is_weak_eighth_position(ctx.score, n.start_tick) {
            out.push(diag(
                "sp5.eighth_notes.weak_position_pairs_only",
                Severity::Warning,
                "eighth note should be on weak subdivision",
                ctx.score,
                n,
                None,
            ));
            continue;
        }
        let paired = (i > 0 && cp[i - 1].duration_ticks == eighth)
            || (i + 1 < cp.len() && cp[i + 1].duration_ticks == eighth);
        if !paired {
            out.push(diag(
                "sp5.eighth_notes.weak_position_pairs_only",
                Severity::Warning,
                "eighth note should appear in pair",
                ctx.score,
                n,
                None,
            ));
        }
    }
    out
}

fn r_sp5_eighth_grouping(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let eighth = tpq(ctx.score) / 2;
    let mut out = Vec::new();
    let mut run = 0usize;
    for n in cp {
        if n.duration_ticks == eighth {
            run += 1;
            if run > 2 {
                out.push(diag(
                    "sp5.eighth_grouping_no_triplet_like_clusters",
                    Severity::Warning,
                    "runs of more than two eighth notes are discouraged in strict species 5",
                    ctx.score,
                    n,
                    None,
                ));
            }
        } else {
            run = 0;
        }
    }
    out
}

fn r_sp5_dissonance_patterns(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        let pc = interval_pc(a.midi, b.midi);
        if is_consonant(pc) {
            continue;
        }
        if is_downbeat(ctx.score, tick) {
            let is_susp = active_note_at(cp, tick)
                .map(|n| n.tie_start || n.tie_end)
                .unwrap_or(false);
            if !is_susp {
                out.push(diag(
                    "sp5.dissonance.licensed_patterns_only",
                    Severity::Error,
                    "downbeat dissonance must be suspension-licensed",
                    ctx.score,
                    a,
                    Some(b),
                ));
            }
        } else if let Some((p, c, n)) = cp_note_neighbors(cp, tick) {
            let d1 = c.midi - p.midi;
            let d2 = n.midi - c.midi;
            if !(d1.abs() <= 2 && d2.abs() <= 2) {
                out.push(diag(
                    "sp5.dissonance.licensed_patterns_only",
                    Severity::Error,
                    "weak-beat dissonance must fit passing/neighbor-type motion",
                    ctx.score,
                    c,
                    Some(b),
                ));
            }
        }
    }
    out
}

fn r_sp5_cadence(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let samples = sample_two_voice(ctx.score);
    let Some((_, a, b)) = samples.last().copied() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if !is_perfect(interval_pc(a.midi, b.midi)) {
        out.push(diag(
            "sp5.cadence.strict_closure_required",
            Severity::Error,
            "species 5 cadence must end with perfect consonance",
            ctx.score,
            a,
            Some(b),
        ));
    }
    out
}

fn r_adv_noop(_ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    Vec::new()
}

fn rule(id: &'static str, eval: fn(&RuleContext<'_>) -> Vec<AnalysisDiagnostic>) -> Box<dyn Rule> {
    Box::new(IdRule { id, eval })
}

pub fn rule_registry() -> HashMap<RuleId, Box<dyn Rule>> {
    let rules: Vec<Box<dyn Rule>> = vec![
        rule("gen.input.single_exercise_per_file", r_gen_input_single_exercise),
        rule(
            "gen.input.key_signature_required_and_stable",
            r_gen_input_key_sig,
        ),
        rule("gen.input.timesig_supported", r_gen_input_timesig),
        rule("gen.input.species_supported", r_gen_input_species_supported),
        rule("gen.input.voice_count_supported", r_gen_input_voice_count),
        rule(
            "gen.input_min_note_length.eighth_or_longer",
            r_gen_input_min_note_len,
        ),
        rule("gen.harmony.supported_sonorities", r_gen_harmony_supported_sonorities),
        rule("gen.motion.parallel_perfects_forbidden", r_parallel_perfects),
        rule("gen.motion.direct_perfects_restricted", r_direct_perfects),
        rule("gen.spacing.upper_adjacent_max_octave", r_spacing_upper_octave),
        rule("gen.spacing.tenor_bass_max_twelfth", r_spacing_tenor_bass),
        rule("gen.spacing.*", r_spacing_alias),
        rule(
            "gen.voice_crossing_and_overlap.restricted",
            r_voice_crossing_overlap,
        ),
        rule("gen.unison.interior_restricted", r_unison_interior),
        rule("gen.melody.max_leap_octave", r_melodic_max_leap),
        rule(
            "gen.melody.dissonant_leaps_forbidden",
            r_melodic_dissonant_leaps,
        ),
        rule(
            "gen.melody.post_leap_compensation_required",
            r_post_leap_compensation,
        ),
        rule("gen.melody.single_climax_preferred", r_single_climax),
        rule(
            "gen.melody.consecutive_large_leaps_restricted",
            r_consecutive_large_leaps,
        ),
        rule(
            "gen.motion.contrary_and_oblique_preferred",
            r_contrary_oblique_preferred,
        ),
        rule(
            "gen.cadence.final_perfect_consonance_required",
            r_final_perfect_cadence,
        ),
        rule(
            "gen.interval.p4_dissonant_against_bass_in_two_voice",
            r_p4_against_bass,
        ),
        rule("gen.voice.leading_tone_not_doubled", r_leading_tone_not_doubled),
        rule(
            "gen.voice.chordal_seventh_resolves_down",
            r_chordal_seventh_resolves_down,
        ),
        rule("gen.voice.leading_tone_resolves_up", r_leading_tone_resolves_up),
        rule("gen.doubling.root_position_prefers_root", r_double_root_pref),
        rule(
            "gen.doubling.first_inversion_no_bass_double_default",
            r_first_inv_no_bass_double,
        ),
        rule(
            "gen.doubling.diminished_first_inversion_double_third",
            r_dim_first_inv_double_third,
        ),
        rule("gen.doubling.second_inversion_double_bass", r_second_inv_double_bass),
        rule(
            "gen.cadence.cadential_64_resolves_65_43",
            r_cadential_64,
        ),
        rule(
            "gen.nct.appoggiatura_escape_anticipation_pedal_retardation_supported",
            r_nct_supported,
        ),
        rule("sp1.rhythm.one_to_one_only", r_sp1_rhythm),
        rule("sp1.vertical.consonance_only", r_sp1_vertical),
        rule("sp1.opening.perfect_consonance_required", r_sp1_opening),
        rule("sp1.ending.unison_or_octave_required", r_sp1_ending),
        rule(
            "sp1.cadence.penultimate_imperfect_consonance",
            r_sp1_penultimate,
        ),
        rule("sp2.rhythm.two_to_one_only", r_sp2_rhythm),
        rule("sp2.strong_beat.consonance_required", r_sp2_strong),
        rule("sp2.dissonance.weak_passing_stepwise", r_sp2_weak_passing),
        rule(
            "sp2.structure.downbeat_skeleton_no_parallel_perfects",
            r_sp2_downbeat_parallel,
        ),
        rule("sp2.downbeat_unison_discouraged", r_sp2_downbeat_unison),
        rule("sp3.rhythm.four_to_one_only", r_sp3_rhythm),
        rule("sp3.strong_beat.consonance_required", r_sp3_strong),
        rule(
            "sp3.dissonance.passing_neighbor_patterns_only",
            r_sp3_patterns,
        ),
        rule(
            "sp3.dissonance.double_neighbor_allowed_pattern",
            r_sp3_double_neighbor,
        ),
        rule(
            "sp3.dissonance.cambiata_limited_exception",
            r_sp3_cambiata,
        ),
        rule("sp3.downbeat_unison_forbidden", r_sp3_downbeat_unison),
        rule("sp4.rhythm.syncopated_ligature_profile", r_sp4_syncopated),
        rule("sp4.suspension.preparation_required", r_sp4_prep),
        rule(
            "sp4.suspension.downbeat_dissonance_allowed_only_if_suspension",
            r_sp4_downbeat_dissonance,
        ),
        rule("sp4.suspension.step_resolution_required", r_sp4_step_resolution),
        rule(
            "sp4.break_species.allowed_when_no_ligature_possible",
            r_sp4_break_species,
        ),
        rule(
            "sp4.allowed_suspension_classes_enforced",
            r_sp4_allowed_classes,
        ),
        rule("sp4.afterbeat_parallel_guard", r_sp4_afterbeat_parallel),
        rule(
            "sp4.all_voices_syncopation_avoidance",
            r_sp4_all_voices_syncopation,
        ),
        rule("sp5.rhythm.mixed_species_profile", r_sp5_rhythm_mixed),
        rule(
            "sp5.strong_beat.consonance_or_prepared_suspension_only",
            r_sp5_strong,
        ),
        rule("sp5.eighth_notes.weak_position_pairs_only", r_sp5_eighth_weak),
        rule("sp5.dissonance.licensed_patterns_only", r_sp5_dissonance_patterns),
        rule("sp5.cadence.strict_closure_required", r_sp5_cadence),
        rule(
            "sp5.eighth_grouping_no_triplet_like_clusters",
            r_sp5_eighth_grouping,
        ),
        rule("adv.invertible.octave_treat_fifth_as_sensitive", r_adv_noop),
        rule(
            "adv.invertible.octave_suspension_pair_76_23_preferred",
            r_adv_noop,
        ),
        rule(
            "adv.invertible.tenth_avoid_parallel_3_6_sources",
            r_adv_noop,
        ),
        rule(
            "adv.invertible.twelfth_limit_structural_sixths",
            r_adv_noop,
        ),
    ];

    let mut m: HashMap<RuleId, Box<dyn Rule>> = HashMap::new();
    for r in rules {
        m.insert(r.id().to_string(), r);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use cp_core::{KeySignature, ScaleMode, ScoreMeta, TimeSignature, Voice};
    use serde_json::json;

    fn mk_score() -> NormalizedScore {
        NormalizedScore {
            meta: ScoreMeta {
                exercise_count: 1,
                key_signature: KeySignature {
                    tonic_pc: 0,
                    mode: ScaleMode::Major,
                },
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
                ticks_per_quarter: 480,
            },
            voices: vec![
                Voice {
                    voice_index: 0,
                    name: "v0".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "a0".to_string(),
                            voice_index: 0,
                            midi: 60,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "a1".to_string(),
                            voice_index: 0,
                            midi: 62,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
                Voice {
                    voice_index: 1,
                    name: "v1".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "b0".to_string(),
                            voice_index: 1,
                            midi: 53,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "b1".to_string(),
                            voice_index: 1,
                            midi: 55,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn registry_contains_all_canonical_ids() {
        let reg = rule_registry();
        assert!(reg.contains_key("gen.input.single_exercise_per_file"));
        assert!(reg.contains_key("sp5.cadence.strict_closure_required"));
        assert!(reg.contains_key("gen.voice.leading_tone_not_doubled"));
    }

    #[test]
    fn direct_perfect_rule_runs() {
        let score = mk_score();
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let d = r_direct_perfects(&ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn rule_param_validation_rejects_bad_threshold() {
        let active = vec!["gen.motion.contrary_and_oblique_preferred".to_string()];
        let mut params = BTreeMap::new();
        params.insert(
            "gen.motion.contrary_and_oblique_preferred".to_string(),
            json!({ "pair_mode": "all_pairs", "similar_motion_ratio_max": 1.2 }),
        );
        let err = validate_rule_params(active.iter(), &params, 3).expect_err("must fail");
        assert!(err
            .iter()
            .any(|e| e.field_path == "similar_motion_ratio_max" && e.reason == "out_of_range"));
    }

    #[test]
    fn contrary_oblique_rule_supports_all_pairs() {
        let score = NormalizedScore {
            meta: ScoreMeta {
                exercise_count: 1,
                key_signature: KeySignature {
                    tonic_pc: 0,
                    mode: ScaleMode::Major,
                },
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
                ticks_per_quarter: 480,
            },
            voices: vec![
                Voice {
                    voice_index: 0,
                    name: "s".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "s0".to_string(),
                            voice_index: 0,
                            midi: 60,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "s1".to_string(),
                            voice_index: 0,
                            midi: 62,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "s2".to_string(),
                            voice_index: 0,
                            midi: 64,
                            start_tick: 960,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
                Voice {
                    voice_index: 1,
                    name: "a".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "a0".to_string(),
                            voice_index: 1,
                            midi: 55,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "a1".to_string(),
                            voice_index: 1,
                            midi: 57,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "a2".to_string(),
                            voice_index: 1,
                            midi: 59,
                            start_tick: 960,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
                Voice {
                    voice_index: 2,
                    name: "b".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "b0".to_string(),
                            voice_index: 2,
                            midi: 48,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "b1".to_string(),
                            voice_index: 2,
                            midi: 50,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "b2".to_string(),
                            voice_index: 2,
                            midi: 52,
                            start_tick: 960,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
            ],
        };
        let mut params = BTreeMap::new();
        params.insert(
            "gen.motion.contrary_and_oblique_preferred".to_string(),
            json!({
                "pair_mode": "all_pairs",
                "similar_motion_ratio_max": 0.5,
                "min_observations": 1
            }),
        );
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::GeneralVoiceLeading,
            rule_params: &params,
        };
        let out = r_contrary_oblique_preferred(&ctx);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|d| d.context.contains_key("pair")));
    }

    fn mk_four_voice_motion_score() -> NormalizedScore {
        let mk_voice = |voice_index: u8, name: &str, midis: [i16; 3]| Voice {
            voice_index,
            name: name.to_string(),
            notes: vec![
                NoteEvent {
                    note_id: format!("{}0", name),
                    voice_index,
                    midi: midis[0],
                    start_tick: 0,
                    duration_ticks: 480,
                    tie_start: false,
                    tie_end: false,
                },
                NoteEvent {
                    note_id: format!("{}1", name),
                    voice_index,
                    midi: midis[1],
                    start_tick: 480,
                    duration_ticks: 480,
                    tie_start: false,
                    tie_end: false,
                },
                NoteEvent {
                    note_id: format!("{}2", name),
                    voice_index,
                    midi: midis[2],
                    start_tick: 960,
                    duration_ticks: 480,
                    tie_start: false,
                    tie_end: false,
                },
            ],
        };

        NormalizedScore {
            meta: ScoreMeta {
                exercise_count: 1,
                key_signature: KeySignature {
                    tonic_pc: 0,
                    mode: ScaleMode::Major,
                },
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
                ticks_per_quarter: 480,
            },
            voices: vec![
                mk_voice(0, "s", [60, 62, 64]), // up, up
                mk_voice(1, "a", [69, 68, 69]), // down, up
                mk_voice(2, "t", [55, 56, 55]), // up, down
                mk_voice(3, "b", [40, 42, 44]), // up, up
            ],
        }
    }

    #[test]
    fn contrary_oblique_rule_supports_outer_voices_mode() {
        let score = mk_four_voice_motion_score();
        let mut params = BTreeMap::new();
        params.insert(
            "gen.motion.contrary_and_oblique_preferred".to_string(),
            json!({
                "pair_mode": "outer_voices",
                "similar_motion_ratio_max": 0.5,
                "min_observations": 1
            }),
        );
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::GeneralVoiceLeading,
            rule_params: &params,
        };
        let out = r_contrary_oblique_preferred(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].context.get("pair").map(String::as_str), Some("0-3"));
    }

    #[test]
    fn contrary_oblique_rule_supports_selected_pairs_mode() {
        let score = mk_four_voice_motion_score();
        let mut params = BTreeMap::new();
        params.insert(
            "gen.motion.contrary_and_oblique_preferred".to_string(),
            json!({
                "pair_mode": "selected_pairs",
                "selected_pairs": [[3, 0], [1, 2]],
                "similar_motion_ratio_max": 0.6,
                "min_observations": 1
            }),
        );
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::GeneralVoiceLeading,
            rule_params: &params,
        };
        let out = r_contrary_oblique_preferred(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].context.get("pair").map(String::as_str), Some("0-3"));
    }
}
