use cp_core::{
    interval_pc, is_consonant, is_perfect, note_location, ticks_per_measure, AnalysisDiagnostic,
    NormalizedScore, NoteEvent, PresetId, RuleId, Severity,
};
use cp_harmony::identify_chord;
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

fn rule_params_or_default<T: DeserializeOwned + Default>(
    ctx: &RuleContext<'_>,
    rule_id: &str,
) -> T {
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
    let a = score.voices[0].notes.as_slice();
    let b = score.voices[1].notes.as_slice();
    // Species checks that need CP/CF roles assume CP has denser rhythm than CF.
    // If both are equal density, preserve declared voice order.
    if b.len() > a.len() {
        Some((b, a))
    } else {
        Some((a, b))
    }
}

fn cp_starts_per_cf_window(cp: &[NoteEvent], cf: &[NoteEvent]) -> Option<Vec<usize>> {
    if cf.is_empty() {
        return None;
    }
    let mut counts = vec![0usize; cf.len()];
    for n in cp {
        let mut placed = false;
        for (i, c) in cf.iter().enumerate() {
            let start = c.start_tick;
            let end = start + c.duration_ticks;
            if n.start_tick >= start && n.start_tick < end {
                counts[i] += 1;
                placed = true;
                break;
            }
        }
        if !placed {
            return None;
        }
    }
    Some(counts)
}

fn species_ratio_with_terminal_cadence_ok(
    cp: &[NoteEvent],
    cf: &[NoteEvent],
    expected_per_cf: usize,
) -> bool {
    let Some(counts) = cp_starts_per_cf_window(cp, cf) else {
        return false;
    };
    if counts.is_empty() {
        return true;
    }
    for c in counts.iter().take(counts.len().saturating_sub(1)) {
        if *c != expected_per_cf {
            return false;
        }
    }
    let last = counts[counts.len() - 1];
    (1..=expected_per_cf).contains(&last)
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
    sample_voice_pair(score, 0, 1)
}

fn sample_voice_pair(
    score: &NormalizedScore,
    upper_voice_index: usize,
    lower_voice_index: usize,
) -> Vec<(u32, &NoteEvent, &NoteEvent)> {
    let mut out = Vec::new();
    let Some(a) = score
        .voices
        .get(upper_voice_index)
        .map(|v| v.notes.as_slice())
    else {
        return out;
    };
    let Some(b) = score
        .voices
        .get(lower_voice_index)
        .map(|v| v.notes.as_slice())
    else {
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
    let allowed = [(2, 4), (3, 4), (4, 4), (2, 2), (5, 4), (6, 4), (3, 2)];
    let mut out = Vec::new();
    if !allowed.contains(&(ts.numerator, ts.denominator)) {
        if let Some(note) = ctx.score.voices.iter().flat_map(|v| v.notes.iter()).next() {
            out.push(diag(
                "gen.input.timesig_supported",
                Severity::Error,
                format!(
                    "unsupported time signature {}/{}",
                    ts.numerator, ts.denominator
                ),
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
            let samples: Vec<(u32, &NoteEvent, &NoteEvent)> = sample_voice_pair(ctx.score, i, j)
                .into_iter()
                .filter(|(tick, a, b)| a.start_tick == *tick && b.start_tick == *tick)
                .collect();
            if samples.len() < 2 {
                continue;
            }
            for w in samples.windows(2) {
                let (_prev_tick, prev_a, prev_b) = w[0];
                let (_tick, now_a, now_b) = w[1];
                let prev = interval_pc(prev_a.midi, prev_b.midi);
                let now = interval_pc(now_a.midi, now_b.midi);
                let da = now_a.midi - prev_a.midi;
                let db = now_b.midi - prev_b.midi;
                let similar = (da > 0 && db > 0) || (da < 0 && db < 0);
                if is_perfect(prev)
                    && is_perfect(now)
                    && prev == now
                    && da != 0
                    && db != 0
                    && similar
                {
                    out.push(diag(
                        "gen.motion.parallel_perfects_forbidden",
                        Severity::Error,
                        "parallel perfect interval detected",
                        ctx.score,
                        now_a,
                        Some(now_b),
                    ));
                }
            }
        }
    }
    out
}

fn r_direct_perfects(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let samples = sample_two_voice(ctx.score);
    if samples.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for w in samples.windows(2) {
        let (_prev_tick, prev_upper, prev_lower) = w[0];
        let (_tick, upper, lower) = w[1];
        let now = interval_pc(upper.midi, lower.midi);
        if !is_perfect(now) {
            continue;
        }
        let du = upper.midi - prev_upper.midi;
        let dl = lower.midi - prev_lower.midi;
        let similar = (du > 0 && dl > 0) || (du < 0 && dl < 0);
        if similar && du != 0 && dl != 0 && du.abs() > 2 {
            out.push(diag(
                "gen.motion.direct_perfects_restricted",
                Severity::Warning,
                "direct perfect interval approach in similar motion",
                ctx.score,
                upper,
                Some(lower),
            ));
        }
    }
    out
}

fn imperfect_generic_class(pc: u8) -> Option<u8> {
    match pc {
        3 | 4 => Some(3),
        8 | 9 => Some(6),
        _ => None,
    }
}

fn r_consecutive_parallel_imperfects(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    const MAX_CONSECUTIVE: usize = 3;

    let mut out = Vec::new();
    for i in 0..ctx.score.voices.len() {
        for j in (i + 1)..ctx.score.voices.len() {
            let samples = sample_voice_pair(ctx.score, i, j);
            if samples.len() < 2 {
                continue;
            }

            let mut run_class: Option<u8> = None;
            let mut run_len: usize = 0;
            let mut run_flagged = false;

            for w in samples.windows(2) {
                let (_prev_tick, prev_a, prev_b) = w[0];
                let (_tick, now_a, now_b) = w[1];

                let prev_class = imperfect_generic_class(interval_pc(prev_a.midi, prev_b.midi));
                let now_class = imperfect_generic_class(interval_pc(now_a.midi, now_b.midi));

                let da = now_a.midi - prev_a.midi;
                let db = now_b.midi - prev_b.midi;
                let similar = (da > 0 && db > 0) || (da < 0 && db < 0);

                let continues_parallel_run = similar
                    && da != 0
                    && db != 0
                    && prev_class.is_some()
                    && prev_class == now_class;

                if !continues_parallel_run {
                    run_class = None;
                    run_len = 0;
                    run_flagged = false;
                    continue;
                }

                let cls = now_class.unwrap_or(0);
                if run_class == Some(cls) {
                    run_len += 1;
                } else {
                    run_class = Some(cls);
                    run_len = 2; // previous + current sonority
                    run_flagged = false;
                }

                if run_len > MAX_CONSECUTIVE && !run_flagged {
                    let mut d = diag(
                        "gen.motion.consecutive_parallel_imperfects_limited",
                        Severity::Error,
                        "more than 3 consecutive parallel thirds/sixths",
                        ctx.score,
                        now_a,
                        Some(now_b),
                    );
                    d.context.insert("pair".to_string(), format!("{}-{}", i, j));
                    d.context.insert("run_length".to_string(), run_len.to_string());
                    d.context.insert("max".to_string(), MAX_CONSECUTIVE.to_string());
                    d.context
                        .insert("generic_interval".to_string(), cls.to_string());
                    out.push(d);
                    run_flagged = true;
                }
            }
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
        let peak_ix: Vec<usize> = v
            .notes
            .iter()
            .enumerate()
            .filter_map(|(ix, n)| if n.midi == max { Some(ix) } else { None })
            .collect();
        let mut independent_peaks: Vec<usize> = Vec::new();
        for ix in peak_ix {
            if let Some(prev_ix) = independent_peaks.last().copied() {
                // Treat tied continuation of the same pitch as one sustained climax.
                if ix == prev_ix + 1 && is_tied_repeat(&v.notes[prev_ix], &v.notes[ix]) {
                    continue;
                }
            }
            independent_peaks.push(ix);
        }
        if independent_peaks.len() > 1 {
            out.push(diag(
                "gen.melody.single_climax_preferred",
                Severity::Warning,
                "multiple highest-note climaxes detected",
                ctx.score,
                &v.notes[independent_peaks[1]],
                Some(&v.notes[independent_peaks[0]]),
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
            let both_large = d1.abs() >= params.large_leap_min_semitones
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

fn is_tied_repeat(prev: &NoteEvent, next: &NoteEvent) -> bool {
    prev.midi == next.midi
        && prev.start_tick + prev.duration_ticks == next.start_tick
        && (prev.tie_start || next.tie_end)
}

fn r_melodic_repeated_pitch_species_profiled(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some(sp) = species_number(ctx.preset_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for v in &ctx.score.voices {
        for w in v.notes.windows(2) {
            if w[0].midi != w[1].midi {
                continue;
            }
            let tied = is_tied_repeat(&w[0], &w[1]);
            let (should_emit, severity, msg) = match sp {
                1 => (
                    true,
                    Severity::Error,
                    "species 1 forbids repeated melodic notes",
                ),
                2 | 3 => (
                    true,
                    Severity::Warning,
                    "repeated melodic notes are discouraged in this species",
                ),
                4 => (
                    !tied,
                    Severity::Error,
                    "species 4 repeated notes should be tie-linked",
                ),
                5 => (
                    !tied,
                    Severity::Warning,
                    "repeated melodic notes are discouraged unless tie-linked",
                ),
                _ => (false, Severity::Warning, ""),
            };
            if !should_emit {
                continue;
            }
            let mut d = diag(
                "gen.melody.repeated_pitch_species_profiled",
                severity,
                msg,
                ctx.score,
                &w[1],
                Some(&w[0]),
            );
            d.context.insert("species".to_string(), sp.to_string());
            d.context
                .insert("voice_index".to_string(), v.voice_index.to_string());
            d.context.insert("tie_linked".to_string(), tied.to_string());
            out.push(d);
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
        d.context
            .insert("total_count".to_string(), total.to_string());
        d.context
            .insert("ratio".to_string(), format!("{:.4}", ratio));
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
    // In species 2-5, dissonance treatment is governed by dedicated species rules
    // (weak-beat passing/neighbor tones, suspensions, etc.). A blanket P4 error
    // would over-report valid licensed dissonances.
    if matches!(
        ctx.preset_id,
        PresetId::Species2 | PresetId::Species3 | PresetId::Species4 | PresetId::Species5
    ) {
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
    Option<String>,
    String,
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
        let bass_pc = notes
            .iter()
            .map(|n| n.midi)
            .min()
            .unwrap_or(60)
            .rem_euclid(12) as u8;
        let chord = identify_chord(ctx.score, &notes);
        let inv = chord.inversion.unwrap_or_else(|| "other".to_string());
        out.push((tick, notes, bass_pc, chord.root_pc, chord.quality, inv));
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
        if cnt > 1 && quality.as_deref() != Some("diminished") {
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
        if inv == "first" && quality.as_deref() == Some("diminished") {
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
        if inv0 == "second" && inv1 == "second" {
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

fn active_pair_at_tick(score: &NormalizedScore, tick: u32) -> Option<(&NoteEvent, &NoteEvent)> {
    if score.voices.len() < 2 {
        return None;
    }
    let a = active_note_at(&score.voices[0].notes, tick)?;
    let b = active_note_at(&score.voices[1].notes, tick)?;
    Some((a, b))
}

fn r_opening_interval_by_position(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if species_number(ctx.preset_id).is_none() {
        return Vec::new();
    }
    let Some((cp, cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let (Some(cp0), Some(cf0)) = (cp.first(), cf.first()) else {
        return Vec::new();
    };
    let tonic = ctx.score.meta.key_signature.tonic_pc;
    let dom = (tonic + 7) % 12;
    let cp_pc = cp0.midi.rem_euclid(12) as u8;
    let cp_below = cp0.midi < cf0.midi;

    let ok = if cp_below {
        cp_pc == tonic
    } else {
        cp_pc == tonic || cp_pc == dom
    };
    if ok {
        return Vec::new();
    }
    vec![diag(
        "gen.opening.interval_by_position_species_profiled",
        Severity::Error,
        if cp_below {
            "when counterpoint starts below the cantus, opening pitch should be tonic (do)"
        } else {
            "opening counterpoint pitch should be tonic (do) or dominant (sol)"
        },
        ctx.score,
        cp0,
        Some(cf0),
    )]
}

fn r_clausula_vera(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if species_number(ctx.preset_id).is_none() {
        return Vec::new();
    }
    let Some((cp, cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    if cp.len() < 2 || cf.len() < 2 {
        return Vec::new();
    }
    let cp_prev = &cp[cp.len() - 2];
    let cp_last = &cp[cp.len() - 1];
    let cf_prev = &cf[cf.len() - 2];
    let cf_last = &cf[cf.len() - 1];

    let tonic = ctx.score.meta.key_signature.tonic_pc;
    let cp_last_pc = cp_last.midi.rem_euclid(12) as u8;
    let cp_prev_pc = cp_prev.midi.rem_euclid(12) as u8;
    let cf_prev_pc = cf_prev.midi.rem_euclid(12) as u8;

    let final_is_perfect = is_perfect(interval_pc(cp_last.midi, cf_last.midi));
    let cp_final_is_tonic = cp_last_pc == tonic;

    let dcp = cp_last.midi - cp_prev.midi;
    let dcf = cf_last.midi - cf_prev.midi;
    let contrary_stepwise = dcp != 0
        && dcf != 0
        && dcp.abs() <= 2
        && dcf.abs() <= 2
        && dcp.signum() != dcf.signum();

    let expected_formula = if cf_prev_pc == (tonic + 2) % 12 {
        cp_prev_pc == (tonic + 11) % 12
    } else if cf_prev_pc == (tonic + 11) % 12 {
        cp_prev_pc == (tonic + 2) % 12
    } else {
        true
    };

    if final_is_perfect && cp_final_is_tonic && contrary_stepwise && expected_formula {
        return Vec::new();
    }

    let mut d = diag(
        "gen.cadence.clausula_vera_required",
        Severity::Error,
        "ending should follow clausula vera profile (tonic final, contrary stepwise approach, penultimate formula)",
        ctx.score,
        cp_last,
        Some(cf_last),
    );
    d.context
        .insert("final_is_perfect".to_string(), final_is_perfect.to_string());
    d.context
        .insert("cp_final_is_tonic".to_string(), cp_final_is_tonic.to_string());
    d.context.insert(
        "contrary_stepwise_approach".to_string(),
        contrary_stepwise.to_string(),
    );
    d.context.insert(
        "formula_match".to_string(),
        expected_formula.to_string(),
    );
    vec![d]
}

fn r_two_voice_max_distance(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if ctx.score.voices.len() != 2 {
        return Vec::new();
    }
    // Uses semitone approximations for compound intervals with MIDI-only input:
    // <= M10 (16 semitones) allowed, > M10 warning, > P12 (19 semitones) error.
    let mut out = Vec::new();
    for (_tick, a, b) in sample_two_voice(ctx.score) {
        let dist = (a.midi - b.midi).abs();
        if dist > 19 {
            out.push(diag(
                "gen.spacing.two_voice_max_distance",
                Severity::Error,
                "two-voice spacing exceeds a twelfth",
                ctx.score,
                a,
                Some(b),
            ));
        } else if dist > 16 {
            out.push(diag(
                "gen.spacing.two_voice_max_distance",
                Severity::Warning,
                "two-voice spacing exceeds a tenth",
                ctx.score,
                a,
                Some(b),
            ));
        }
    }
    out
}

fn r_climax_non_coincident(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    if ctx.score.voices.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..ctx.score.voices.len() {
        for j in (i + 1)..ctx.score.voices.len() {
            let vi = &ctx.score.voices[i];
            let vj = &ctx.score.voices[j];
            let Some(max_i) = vi.notes.iter().map(|n| n.midi).max() else {
                continue;
            };
            let Some(max_j) = vj.notes.iter().map(|n| n.midi).max() else {
                continue;
            };
            let highs_i: Vec<&NoteEvent> = vi.notes.iter().filter(|n| n.midi == max_i).collect();
            let highs_j: Vec<&NoteEvent> = vj.notes.iter().filter(|n| n.midi == max_j).collect();
            let mut hit: Option<(&NoteEvent, &NoteEvent)> = None;
            for ni in &highs_i {
                if let Some(nj) = highs_j.iter().copied().find(|nj| nj.start_tick == ni.start_tick) {
                    hit = Some((ni, nj));
                    break;
                }
            }
            if let Some((ni, nj)) = hit {
                let mut d = diag(
                    "gen.melody.climax_non_coincident_between_voices",
                    Severity::Warning,
                    "voice climaxes should not coincide",
                    ctx.score,
                    ni,
                    Some(nj),
                );
                d.context.insert("pair".to_string(), format!("{}-{}", i, j));
                out.push(d);
            }
        }
    }
    out
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
    if species_ratio_with_terminal_cadence_ok(cp, cf, 2) {
        return Vec::new();
    }
    let Some(n) = cp.first() else {
        return Vec::new();
    };
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

fn cp_index_at_tick(cp: &[NoteEvent], tick: u32) -> Option<usize> {
    cp.iter()
        .position(|n| n.start_tick <= tick && tick < n.start_tick + n.duration_ticks)
}

fn is_step_interval(d: i16) -> bool {
    d != 0 && d.abs() <= 2
}

fn is_passing_motion(cp: &[NoteEvent], idx: usize) -> bool {
    if idx == 0 || idx + 1 >= cp.len() {
        return false;
    }
    let d1 = cp[idx].midi - cp[idx - 1].midi;
    let d2 = cp[idx + 1].midi - cp[idx].midi;
    is_step_interval(d1) && is_step_interval(d2) && d1.signum() == d2.signum()
}

fn is_neighbor_motion(cp: &[NoteEvent], idx: usize) -> bool {
    if idx == 0 || idx + 1 >= cp.len() {
        return false;
    }
    let d1 = cp[idx].midi - cp[idx - 1].midi;
    let d2 = cp[idx + 1].midi - cp[idx].midi;
    is_step_interval(d1)
        && is_step_interval(d2)
        && d1.signum() == -d2.signum()
        && cp[idx - 1].midi == cp[idx + 1].midi
}

fn is_double_neighbor_window(w: &[NoteEvent]) -> bool {
    if w.len() != 4 {
        return false;
    }
    if w[0].midi != w[3].midi {
        return false;
    }
    let d_up = w[1].midi - w[0].midi;
    let d_down = w[2].midi - w[3].midi;
    if !is_step_interval(d_up) || !is_step_interval(d_down) {
        return false;
    }
    let rel1 = (w[1].midi - w[0].midi).signum();
    let rel2 = (w[2].midi - w[0].midi).signum();
    rel1 != 0 && rel2 != 0 && rel1 == -rel2
}

fn is_double_neighbor_member(cp: &[NoteEvent], idx: usize) -> bool {
    if cp.len() < 4 {
        return false;
    }
    for s in 0..=(cp.len() - 4) {
        if idx != s + 1 && idx != s + 2 {
            continue;
        }
        if is_double_neighbor_window(&cp[s..s + 4]) {
            return true;
        }
    }
    false
}

fn is_cambiata_at(cp: &[NoteEvent], idx: usize) -> bool {
    if idx == 0 || idx + 3 >= cp.len() {
        return false;
    }
    let d1 = cp[idx].midi - cp[idx - 1].midi;
    let d2 = cp[idx + 1].midi - cp[idx].midi;
    let d3 = cp[idx + 2].midi - cp[idx + 1].midi;
    let d4 = cp[idx + 3].midi - cp[idx + 2].midi;
    // Classical cambiata contour: step down to dissonance, leap down, then step up twice.
    (d1 == -1 || d1 == -2)
        && d2 <= -3
        && is_step_interval(d3)
        && is_step_interval(d4)
        && d3 > 0
        && d4 > 0
}

fn is_escape_motion(cp: &[NoteEvent], idx: usize) -> bool {
    if idx == 0 || idx + 1 >= cp.len() {
        return false;
    }
    let d1 = cp[idx].midi - cp[idx - 1].midi;
    let d2 = cp[idx + 1].midi - cp[idx].midi;
    is_step_interval(d1) && d2.abs() >= 3 && d1.signum() == -d2.signum()
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
            let passing_stepwise =
                is_step_interval(d1) && is_step_interval(d2) && d1.signum() == d2.signum();
            let bounded_by_consonance = active_note_at(cf, p.start_tick)
                .map(|x| is_consonant(interval_pc(p.midi, x.midi)))
                .unwrap_or(false)
                && active_note_at(cf, n.start_tick)
                    .map(|x| is_consonant(interval_pc(n.midi, x.midi)))
                    .unwrap_or(false);
            if !(passing_stepwise && bounded_by_consonance) {
                out.push(diag(
                    "sp2.dissonance.weak_passing_stepwise",
                    Severity::Error,
                    "weak-beat dissonance must be passing and stepwise",
                    ctx.score,
                    c,
                    Some(cf.iter().find(|x| x.note_id == b.note_id).unwrap_or(b)),
                ));
            }
        } else {
            out.push(diag(
                "sp2.dissonance.weak_passing_stepwise",
                Severity::Error,
                "weak-beat dissonance must be passing and stepwise",
                ctx.score,
                a,
                Some(b),
            ));
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
        if is_perfect(i0) && is_perfect(i1) && i0 == i1 && a0.midi != a1.midi && b0.midi != b1.midi
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
    let downbeat_ticks: Vec<u32> = sample_two_voice(ctx.score)
        .into_iter()
        .filter_map(|(tick, _, _)| is_downbeat(ctx.score, tick).then_some(tick))
        .collect();
    let first = downbeat_ticks.first().copied();
    let last = downbeat_ticks.last().copied();
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if Some(tick) == first || Some(tick) == last {
            continue;
        }
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

fn interval_class_downbeat(score: &NormalizedScore) -> Vec<(u32, &NoteEvent, &NoteEvent, u8)> {
    sample_two_voice(score)
        .into_iter()
        .filter(|(tick, _, _)| is_downbeat(score, *tick))
        .map(|(tick, a, b)| (tick, a, b, interval_pc(a.midi, b.midi)))
        .collect()
}

fn r_sp2_downbeat_interval_repetition(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let downs = interval_class_downbeat(ctx.score);
    if downs.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut imperfect_run_class: Option<u8> = None;
    let mut imperfect_run_len: usize = 0;
    for w in downs.windows(2) {
        let (_t0, _a0, _b0, i0) = w[0];
        let (_t1, a1, b1, i1) = w[1];
        if is_perfect(i0) && i0 == i1 {
            out.push(diag(
                "sp2.structure.downbeat_interval_repetition_limits",
                Severity::Error,
                "species 2 should not repeat the same perfect interval on consecutive downbeats",
                ctx.score,
                a1,
                Some(b1),
            ));
        }

        let cls0 = imperfect_generic_class(i0);
        let cls1 = imperfect_generic_class(i1);
        if cls0.is_some() && cls0 == cls1 {
            let cls = cls1.unwrap_or(0);
            if imperfect_run_class == Some(cls) {
                imperfect_run_len += 1;
            } else {
                imperfect_run_class = Some(cls);
                imperfect_run_len = 2;
            }
            if imperfect_run_len > 3 {
                out.push(diag(
                    "sp2.structure.downbeat_interval_repetition_limits",
                    Severity::Warning,
                    "species 2 should avoid more than 3 downbeats in one imperfect interval class",
                    ctx.score,
                    a1,
                    Some(b1),
                ));
                imperfect_run_class = None;
                imperfect_run_len = 0;
            }
        } else {
            imperfect_run_class = None;
            imperfect_run_len = 0;
        }
    }
    out
}

fn is_step(d: i16) -> bool {
    d.abs() == 1 || d.abs() == 2
}

fn same_dir(a: i16, b: i16) -> bool {
    (a > 0 && b > 0) || (a < 0 && b < 0)
}

fn r_sp2_weak_consonant_pattern_catalog(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    if cp.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for idx in 1..(cp.len() - 1) {
        let n0 = &cp[idx - 1];
        let n1 = &cp[idx];
        let n2 = &cp[idx + 1];
        if n1.start_tick % tpm(ctx.score) != tpm(ctx.score) / 2 {
            continue;
        }
        let Some(cf_note) = active_note_at(cf, n1.start_tick) else {
            continue;
        };
        if !is_consonant(interval_pc(n1.midi, cf_note.midi)) {
            continue;
        }

        let d1 = n1.midi - n0.midi;
        let d2 = n2.midi - n1.midi;
        let ds = n2.midi - n0.midi;

        let consonant_passing = is_step(d1) && is_step(d2) && same_dir(d1, d2) && ds.abs() <= 4;
        let consonant_neighbor =
            is_step(d1) && is_step(d2) && d1.signum() == -d2.signum() && n2.midi == n0.midi;
        let substitution = d1.abs() == 5 && is_step(d2) && d1.signum() == -d2.signum();
        let skipped_passing =
            matches!(d1.abs(), 3 | 4) && is_step(d2) && same_dir(d1, d2) && ds.abs() <= 5;
        let interval_subdivision = matches!(d1.abs(), 3 | 4 | 5)
            && matches!(d2.abs(), 3 | 4 | 5)
            && same_dir(d1, d2)
            && matches!(ds.abs(), 7..=10);
        let change_register =
            matches!(d1.abs(), 7 | 8 | 9 | 12) && is_step(d2) && d1.signum() == -d2.signum();
        let delay_progression =
            matches!(d1.abs(), 3 | 4) && is_step(d2) && d1.signum() == -d2.signum() && is_step(ds);

        if consonant_passing
            || consonant_neighbor
            || substitution
            || skipped_passing
            || interval_subdivision
            || change_register
            || delay_progression
        {
            continue;
        }

        out.push(diag(
            "sp2.weak_beat.consonant_pattern_catalog",
            Severity::Warning,
            "weak-beat consonant is outside the standard species-2 pattern catalog",
            ctx.score,
            n1,
            Some(n0),
        ));
    }
    out
}

fn r_sp3_rhythm(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    if species_ratio_with_terminal_cadence_ok(cp, cf, 4) {
        return Vec::new();
    }
    let Some(n) = cp.first() else {
        return Vec::new();
    };
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
        let Some(idx) = cp_index_at_tick(cp, tick) else {
            continue;
        };
        let licensed = is_passing_motion(cp, idx)
            || is_neighbor_motion(cp, idx)
            || is_double_neighbor_member(cp, idx)
            || is_cambiata_at(cp, idx);
        if !licensed {
            out.push(diag(
                "sp3.dissonance.passing_neighbor_patterns_only",
                Severity::Error,
                "species 3 dissonance must be passing/neighbor pattern",
                ctx.score,
                a,
                Some(b),
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
    let samples = sample_two_voice(ctx.score);
    for (tick, cp_note, cf_note) in samples {
        if is_downbeat(ctx.score, tick) {
            continue;
        }
        if is_consonant(interval_pc(cp_note.midi, cf_note.midi)) {
            continue;
        }
        let Some(idx) = cp_index_at_tick(cp, tick) else {
            continue;
        };
        if idx + 1 >= cp.len() {
            continue;
        }
        let leave = cp[idx + 1].midi - cp[idx].midi;
        if leave.abs() >= 3 && !is_cambiata_at(cp, idx) {
            out.push(diag(
                "sp3.dissonance.cambiata_limited_exception",
                Severity::Warning,
                "leap-from-dissonance should follow cambiata schema",
                ctx.score,
                cp_note,
                Some(cf_note),
            ));
        }
    }
    out
}

fn r_sp3_downbeat_unison(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let downbeat_ticks: Vec<u32> = sample_two_voice(ctx.score)
        .into_iter()
        .filter_map(|(tick, _, _)| is_downbeat(ctx.score, tick).then_some(tick))
        .collect();
    let first = downbeat_ticks.first().copied();
    let last = downbeat_ticks.last().copied();
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if Some(tick) == first || Some(tick) == last {
            continue;
        }
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

fn r_sp3_downbeat_interval_repetition(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let downs = interval_class_downbeat(ctx.score);
    if downs.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();

    let mut perfect_run_pc: Option<u8> = None;
    let mut perfect_run_len: usize = 0;
    let mut imperfect_run_class: Option<u8> = None;
    let mut imperfect_run_len: usize = 0;

    for (_tick, a, b, i) in downs {
        if is_perfect(i) {
            if perfect_run_pc == Some(i) {
                perfect_run_len += 1;
            } else {
                perfect_run_pc = Some(i);
                perfect_run_len = 1;
            }
            if perfect_run_len > 2 {
                out.push(diag(
                    "sp3.structure.downbeat_interval_repetition_limits",
                    Severity::Error,
                    "species 3 allows at most two consecutive downbeats in one perfect interval",
                    ctx.score,
                    a,
                    Some(b),
                ));
                perfect_run_pc = Some(i);
                perfect_run_len = 1;
            }
        } else {
            perfect_run_pc = None;
            perfect_run_len = 0;
        }

        if let Some(cls) = imperfect_generic_class(i) {
            if imperfect_run_class == Some(cls) {
                imperfect_run_len += 1;
            } else {
                imperfect_run_class = Some(cls);
                imperfect_run_len = 1;
            }
            if imperfect_run_len > 3 {
                out.push(diag(
                    "sp3.structure.downbeat_interval_repetition_limits",
                    Severity::Warning,
                    "species 3 should avoid more than 3 downbeats in one imperfect interval class",
                    ctx.score,
                    a,
                    Some(b),
                ));
                imperfect_run_class = Some(cls);
                imperfect_run_len = 1;
            }
        } else {
            imperfect_run_class = None;
            imperfect_run_len = 0;
        }
    }
    out
}

fn r_sp3_perfect_interval_proximity_guard(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let bt = beat_ticks(ctx.score);
    let mut out = Vec::new();
    for (tick, a, b) in sample_two_voice(ctx.score) {
        if !is_downbeat(ctx.score, tick) {
            continue;
        }
        let now = interval_pc(a.midi, b.midi);
        let checks: &[u32] = if now == 7 {
            &[1, 2] // previous beats 4 and 3
        } else if now == 0 {
            &[1, 2, 3] // previous beats 4,3,2
        } else {
            continue;
        };

        for off in checks {
            let Some(prev_tick) = tick.checked_sub(bt * *off) else {
                continue;
            };
            let Some((pa, pb)) = active_pair_at_tick(ctx.score, prev_tick) else {
                continue;
            };
            if interval_pc(pa.midi, pb.midi) == now {
                out.push(diag(
                    "sp3.perfect_interval_proximity_guard",
                    Severity::Error,
                    "perfect interval appears too close before a structural downbeat perfect interval",
                    ctx.score,
                    a,
                    Some(pa),
                ));
                break;
            }
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
    if cp
        .iter()
        .any(|n| n.tie_start && (n.start_tick + n.duration_ticks) % tpm(ctx.score) == 0)
    {
        return Vec::new();
    }
    let Some(n) = cp.first() else {
        return Vec::new();
    };
    vec![diag(
        "sp4.rhythm.syncopated_ligature_profile",
        Severity::Error,
        "species 4 expects syncopated ligatures across barlines",
        ctx.score,
        n,
        None,
    )]
}

fn r_sp4_strict_entry_exit_profile(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    if cp.is_empty() || cf.len() < 2 {
        return Vec::new();
    }

    let tonic = ctx.score.meta.key_signature.tonic_pc;
    let mut reasons: Vec<&str> = Vec::new();

    let cp0 = &cp[0];
    if cp0.start_tick != tpq(ctx.score) * 2 {
        reasons.push("entry should start after a half rest");
    }

    let cp_last = &cp[cp.len() - 1];
    if cp_last.duration_ticks < tpm(ctx.score) {
        reasons.push("final counterpoint note should cover the full last bar");
    }
    if cp_last.midi.rem_euclid(12) as u8 != tonic {
        reasons.push("counterpoint final should be tonic");
    }
    if cp.len() >= 2 {
        let cp_prev = &cp[cp.len() - 2];
        if !is_step(cp_last.midi - cp_prev.midi) {
            reasons.push("counterpoint cadence should approach final by step");
        }
    }

    let cf_prev = &cf[cf.len() - 2];
    let cf_last = &cf[cf.len() - 1];
    if cf_last.midi.rem_euclid(12) as u8 != tonic {
        reasons.push("cantus final should be tonic");
    }
    if cf_prev.midi.rem_euclid(12) as u8 != (tonic + 2) % 12 {
        reasons.push("cantus penultimate should be re");
    }
    let dcf = cf_last.midi - cf_prev.midi;
    if !(dcf == -1 || dcf == -2) {
        reasons.push("cantus should end by descending step");
    }

    if reasons.is_empty() {
        return Vec::new();
    }
    let mut d = diag(
        "sp4.form.strict_entry_exit_profile",
        Severity::Error,
        "species 4 entry/exit profile violation",
        ctx.score,
        cp_last,
        Some(cf_last),
    );
    d.context
        .insert("reasons".to_string(), reasons.join("; "));
    vec![d]
}

fn r_sp4_prep(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((_, cf)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (n, _end, cf_downbeat) in suspension_events(ctx) {
        let Some(cf_prep) = active_note_at(cf, n.start_tick) else {
            continue;
        };
        if !is_consonant(interval_pc(n.midi, cf_prep.midi)) {
            out.push(diag(
                "sp4.suspension.preparation_required",
                Severity::Error,
                "suspension must be prepared by consonance",
                ctx.score,
                n,
                Some(cf_downbeat),
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
        let Some(held_ix) = cp_index_at_tick(cp, n.start_tick + n.duration_ticks) else {
            continue;
        };
        let held = &cp[held_ix];
        let downbeat_pc = interval_pc(held.midi, cf.midi);
        // Consonant ties (e.g., 6-5, 5-6) are permitted in strict species 4.
        if is_consonant(downbeat_pc) {
            continue;
        }
        if held_ix + 1 >= cp.len() {
            continue;
        }
        let d = cp[held_ix + 1].midi - held.midi;
        if !(d == -1 || d == -2) {
            out.push(diag(
                "sp4.suspension.step_resolution_required",
                Severity::Error,
                "suspension should resolve downward by step",
                ctx.score,
                held,
                Some(cf),
            ));
        }
    }
    out
}

fn r_sp4_break_species(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    if cp.is_empty() {
        return Vec::new();
    }
    let tpm = tpm(ctx.score);
    let end_tick = cp
        .iter()
        .map(|n| n.start_tick + n.duration_ticks)
        .max()
        .unwrap_or(0);
    if end_tick <= tpm * 2 {
        return Vec::new();
    }
    let measures = (end_tick / tpm) as usize;
    if measures < 3 {
        return Vec::new();
    }

    let mut breaks: Vec<usize> = Vec::new();
    for m in 1..(measures - 1) {
        let m_start = m as u32 * tpm;
        let m_end = m_start + tpm;
        let has_ligature = cp.iter().any(|n| {
            n.start_tick >= m_start
                && n.start_tick < m_end
                && n.tie_start
                && (n.start_tick + n.duration_ticks) % tpm == 0
        });
        if !has_ligature {
            breaks.push(m);
        }
    }
    if breaks.is_empty() {
        return Vec::new();
    }
    let mut segments = 1usize;
    for w in breaks.windows(2) {
        if w[1] != w[0] + 1 {
            segments += 1;
        }
    }
    if breaks.len() <= 2 && segments <= 1 {
        return Vec::new();
    }

    let m0 = breaks[0] as u32 * tpm;
    let note = cp.iter().find(|n| n.start_tick >= m0).unwrap_or(&cp[0]);
    let mut d = diag(
        "sp4.form.break_species_budget",
        Severity::Warning,
        "species-4 break-species usage exceeds recommended budget",
        ctx.score,
        note,
        None,
    );
    d.context
        .insert("break_measures".to_string(), breaks.len().to_string());
    d.context
        .insert("break_segments".to_string(), segments.to_string());
    vec![d]
}

fn r_sp4_suspension_density_minimum(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let Some((cp, _)) = pair_notes(ctx.score) else {
        return Vec::new();
    };
    let events = suspension_events(ctx);
    if events.is_empty() {
        let Some(n) = cp.first() else {
            return Vec::new();
        };
        return vec![diag(
            "sp4.suspension_density_minimum",
            Severity::Warning,
            "species 4 should include dissonant suspensions regularly",
            ctx.score,
            n,
            None,
        )];
    }
    let mut dissonant = 0usize;
    for (_n, end, cf_note) in &events {
        let Some(ix) = cp_index_at_tick(cp, *end) else {
            continue;
        };
        let held = &cp[ix];
        if !is_consonant(interval_pc(held.midi, cf_note.midi)) {
            dissonant += 1;
        }
    }
    let density = dissonant as f32 / events.len() as f32;
    if density >= 0.5 {
        return Vec::new();
    }
    let note = events
        .first()
        .map(|(n, _, _)| *n)
        .or_else(|| cp.first())
        .unwrap_or(&cp[0]);
    let mut d = diag(
        "sp4.suspension_density_minimum",
        Severity::Warning,
        "species 4 should use more dissonant suspensions",
        ctx.score,
        note,
        None,
    );
    d.context.insert("density".to_string(), format!("{:.3}", density));
    d.context
        .insert("dissonant".to_string(), dissonant.to_string());
    d.context
        .insert("total_events".to_string(), events.len().to_string());
    vec![d]
}

fn r_sp4_allowed_classes(ctx: &RuleContext<'_>) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for (n, end, cf) in suspension_events(ctx) {
        let Some(cp_active) = pair_notes(ctx.score).and_then(|(cp, _)| active_note_at(cp, end))
        else {
            continue;
        };
        let pc = interval_pc(cp_active.midi, cf.midi);
        if is_consonant(pc) {
            continue;
        }
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
    let Some(n) = cp.first() else {
        return Vec::new();
    };
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
        } else {
            let Some(idx) = cp_index_at_tick(cp, tick) else {
                continue;
            };
            let licensed = is_passing_motion(cp, idx)
                || is_neighbor_motion(cp, idx)
                || is_double_neighbor_member(cp, idx)
                || is_cambiata_at(cp, idx)
                || is_escape_motion(cp, idx);
            if !licensed {
                out.push(diag(
                    "sp5.dissonance.licensed_patterns_only",
                    Severity::Error,
                    "weak-beat dissonance must fit licensed florid pattern",
                    ctx.score,
                    a,
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
        rule(
            "gen.input.single_exercise_per_file",
            r_gen_input_single_exercise,
        ),
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
        rule(
            "gen.harmony.supported_sonorities",
            r_gen_harmony_supported_sonorities,
        ),
        rule(
            "gen.motion.parallel_perfects_forbidden",
            r_parallel_perfects,
        ),
        rule("gen.motion.direct_perfects_restricted", r_direct_perfects),
        rule(
            "gen.motion.consecutive_parallel_imperfects_limited",
            r_consecutive_parallel_imperfects,
        ),
        rule(
            "gen.spacing.two_voice_max_distance",
            r_two_voice_max_distance,
        ),
        rule(
            "gen.spacing.upper_adjacent_max_octave",
            r_spacing_upper_octave,
        ),
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
            "gen.melody.repeated_pitch_species_profiled",
            r_melodic_repeated_pitch_species_profiled,
        ),
        rule(
            "gen.melody.climax_non_coincident_between_voices",
            r_climax_non_coincident,
        ),
        rule(
            "gen.motion.contrary_and_oblique_preferred",
            r_contrary_oblique_preferred,
        ),
        rule(
            "gen.opening.interval_by_position_species_profiled",
            r_opening_interval_by_position,
        ),
        rule("gen.cadence.clausula_vera_required", r_clausula_vera),
        rule(
            "gen.cadence.final_perfect_consonance_required",
            r_final_perfect_cadence,
        ),
        rule(
            "gen.interval.p4_dissonant_against_bass_in_two_voice",
            r_p4_against_bass,
        ),
        rule(
            "gen.voice.leading_tone_not_doubled",
            r_leading_tone_not_doubled,
        ),
        rule(
            "gen.voice.chordal_seventh_resolves_down",
            r_chordal_seventh_resolves_down,
        ),
        rule(
            "gen.voice.leading_tone_resolves_up",
            r_leading_tone_resolves_up,
        ),
        rule(
            "gen.doubling.root_position_prefers_root",
            r_double_root_pref,
        ),
        rule(
            "gen.doubling.first_inversion_no_bass_double_default",
            r_first_inv_no_bass_double,
        ),
        rule(
            "gen.doubling.diminished_first_inversion_double_third",
            r_dim_first_inv_double_third,
        ),
        rule(
            "gen.doubling.second_inversion_double_bass",
            r_second_inv_double_bass,
        ),
        rule("gen.cadence.cadential_64_resolves_65_43", r_cadential_64),
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
        rule(
            "sp2.structure.downbeat_interval_repetition_limits",
            r_sp2_downbeat_interval_repetition,
        ),
        rule(
            "sp2.weak_beat.consonant_pattern_catalog",
            r_sp2_weak_consonant_pattern_catalog,
        ),
        rule("sp2.downbeat_unison_discouraged", r_sp2_downbeat_unison),
        rule("sp3.rhythm.four_to_one_only", r_sp3_rhythm),
        rule("sp3.strong_beat.consonance_required", r_sp3_strong),
        rule(
            "sp3.dissonance.passing_neighbor_patterns_only",
            r_sp3_patterns,
        ),
        rule("sp3.dissonance.cambiata_limited_exception", r_sp3_cambiata),
        rule(
            "sp3.structure.downbeat_interval_repetition_limits",
            r_sp3_downbeat_interval_repetition,
        ),
        rule(
            "sp3.perfect_interval_proximity_guard",
            r_sp3_perfect_interval_proximity_guard,
        ),
        rule("sp3.downbeat_unison_forbidden", r_sp3_downbeat_unison),
        rule("sp4.rhythm.syncopated_ligature_profile", r_sp4_syncopated),
        rule(
            "sp4.form.strict_entry_exit_profile",
            r_sp4_strict_entry_exit_profile,
        ),
        rule("sp4.suspension.preparation_required", r_sp4_prep),
        rule(
            "sp4.suspension.downbeat_dissonance_allowed_only_if_suspension",
            r_sp4_downbeat_dissonance,
        ),
        rule(
            "sp4.suspension.step_resolution_required",
            r_sp4_step_resolution,
        ),
        rule(
            "sp4.break_species.allowed_when_no_ligature_possible",
            r_adv_noop,
        ),
        rule("sp4.form.break_species_budget", r_sp4_break_species),
        rule(
            "sp4.suspension_density_minimum",
            r_sp4_suspension_density_minimum,
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
        rule(
            "sp5.eighth_notes.weak_position_pairs_only",
            r_sp5_eighth_weak,
        ),
        rule(
            "sp5.dissonance.licensed_patterns_only",
            r_sp5_dissonance_patterns,
        ),
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
        rule("adv.invertible.twelfth_limit_structural_sixths", r_adv_noop),
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

    fn mk_two_voice_score(
        cp: &[(i16, u32, bool, bool)],
        cf: &[(i16, u32, bool, bool)],
        numerator: u8,
        denominator: u8,
    ) -> NormalizedScore {
        fn mk_voice(voice_index: u8, name: &str, spec: &[(i16, u32, bool, bool)]) -> Voice {
            let mut tick = 0u32;
            let mut notes = Vec::new();
            for (i, (midi, dur, tie_start, tie_end)) in spec.iter().copied().enumerate() {
                notes.push(NoteEvent {
                    note_id: format!("{}_{}", name, i),
                    voice_index,
                    midi,
                    start_tick: tick,
                    duration_ticks: dur,
                    tie_start,
                    tie_end,
                });
                tick += dur;
            }
            Voice {
                voice_index,
                name: name.to_string(),
                notes,
            }
        }

        NormalizedScore {
            meta: ScoreMeta {
                exercise_count: 1,
                key_signature: KeySignature {
                    tonic_pc: 0,
                    mode: ScaleMode::Major,
                },
                time_signature: TimeSignature {
                    numerator,
                    denominator,
                },
                ticks_per_quarter: 480,
            },
            voices: vec![mk_voice(0, "cp", cp), mk_voice(1, "cf", cf)],
        }
    }

    #[test]
    fn registry_contains_all_canonical_ids() {
        let reg = rule_registry();
        assert!(reg.contains_key("gen.input.single_exercise_per_file"));
        assert!(reg.contains_key("sp5.cadence.strict_closure_required"));
        assert!(reg.contains_key("gen.voice.leading_tone_not_doubled"));
        assert!(reg.contains_key("gen.motion.consecutive_parallel_imperfects_limited"));
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
    fn p4_rule_is_suppressed_for_species2_profile() {
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
                    name: "cp".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "cp0".to_string(),
                            voice_index: 0,
                            midi: 60,
                            start_tick: 0,
                            duration_ticks: 960,
                            tie_start: false,
                            tie_end: false,
                        },
                        // weak beat note forms P4 (F over C) and is valid passing context for species 2
                        NoteEvent {
                            note_id: "cp1".to_string(),
                            voice_index: 0,
                            midi: 65,
                            start_tick: 960,
                            duration_ticks: 960,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
                Voice {
                    voice_index: 1,
                    name: "cf".to_string(),
                    notes: vec![NoteEvent {
                        note_id: "cf0".to_string(),
                        voice_index: 1,
                        midi: 53,
                        start_tick: 0,
                        duration_ticks: 1920,
                        tie_start: false,
                        tie_end: false,
                    }],
                },
            ],
        };
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let d = r_p4_against_bass(&ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn sp2_rhythm_allows_single_long_final_cadence_note() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (62, 960, false, false),
                (64, 1920, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let d = r_sp2_rhythm(&ctx);
        assert!(d.is_empty(), "expected cadential long-note exception, got: {:?}", d);
    }

    #[test]
    fn sp2_rhythm_rejects_non_terminal_ratio_break() {
        let score = mk_two_voice_score(
            &[
                (60, 1920, false, false),
                (62, 960, false, false),
                (64, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let d = r_sp2_rhythm(&ctx);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "sp2.rhythm.two_to_one_only");
    }

    #[test]
    fn sp3_rhythm_allows_single_long_final_cadence_note() {
        let score = mk_two_voice_score(
            &[
                (60, 480, false, false),
                (62, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
                (67, 1920, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        let d = r_sp3_rhythm(&ctx);
        assert!(d.is_empty(), "expected cadential long-note exception, got: {:?}", d);
    }

    #[test]
    fn sp3_rhythm_allows_terminal_note_count_variation() {
        let score = mk_two_voice_score(
            &[
                (60, 480, false, false),
                (62, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
                (67, 960, false, false),
                (69, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        let d = r_sp3_rhythm(&ctx);
        assert!(d.is_empty(), "expected terminal rhythm variation allowance, got: {:?}", d);
    }

    #[test]
    fn parallel_perfects_are_time_aligned_not_index_aligned() {
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
                    name: "upper".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "u0".to_string(),
                            voice_index: 0,
                            midi: 60,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "u1".to_string(),
                            voice_index: 0,
                            midi: 62,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "u2".to_string(),
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
                    name: "lower".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "l0".to_string(),
                            voice_index: 1,
                            midi: 53,
                            start_tick: 0,
                            duration_ticks: 960,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "l1".to_string(),
                            voice_index: 1,
                            midi: 55,
                            start_tick: 960,
                            duration_ticks: 960,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
            ],
        };
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let d = r_parallel_perfects(&ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn direct_perfects_are_time_aligned_not_index_aligned() {
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
                    name: "upper".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "u0".to_string(),
                            voice_index: 0,
                            midi: 60,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "u1".to_string(),
                            voice_index: 0,
                            midi: 64,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "u2".to_string(),
                            voice_index: 0,
                            midi: 65,
                            start_tick: 960,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
                Voice {
                    voice_index: 1,
                    name: "lower".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "l0".to_string(),
                            voice_index: 1,
                            midi: 53,
                            start_tick: 0,
                            duration_ticks: 960,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "l1".to_string(),
                            voice_index: 1,
                            midi: 55,
                            start_tick: 960,
                            duration_ticks: 960,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
            ],
        };
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let d = r_direct_perfects(&ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn parallel_perfects_detected_on_aligned_similar_motion() {
        let score = mk_score();
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let d = r_parallel_perfects(&ctx);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn consecutive_parallel_imperfects_limit_enforced() {
        let score = mk_two_voice_score(
            &[
                (64, 480, false, false),
                (65, 480, false, false),
                (67, 480, false, false),
                (69, 480, false, false),
                (71, 480, false, false),
            ],
            &[
                (60, 480, false, false),
                (62, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
                (67, 480, false, false),
            ],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let d = r_consecutive_parallel_imperfects(&ctx);
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].rule_id,
            "gen.motion.consecutive_parallel_imperfects_limited"
        );
    }

    #[test]
    fn consecutive_parallel_imperfects_allows_three() {
        let score = mk_two_voice_score(
            &[
                (64, 480, false, false),
                (65, 480, false, false),
                (67, 480, false, false),
            ],
            &[
                (60, 480, false, false),
                (62, 480, false, false),
                (64, 480, false, false),
            ],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let d = r_consecutive_parallel_imperfects(&ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn consecutive_parallel_imperfects_reset_by_species2_weak_beat_events() {
        let score = mk_two_voice_score(
            &[
                (64, 960, false, false),
                (65, 960, false, false),
                (65, 960, false, false),
                (67, 960, false, false),
                (67, 960, false, false),
                (69, 960, false, false),
                (69, 960, false, false),
                (71, 960, false, false),
                (71, 960, false, false),
                (72, 960, false, false),
            ],
            &[
                (60, 1920, false, false),
                (62, 1920, false, false),
                (64, 1920, false, false),
                (65, 1920, false, false),
                (67, 1920, false, false),
            ],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let d = r_consecutive_parallel_imperfects(&ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn sp2_allows_consonant_weak_beat_leaps() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (65, 960, false, false),
                (62, 960, false, false),
                (64, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        assert!(r_sp2_weak_passing(&ctx).is_empty());
    }

    #[test]
    fn sp2_rejects_dissonant_weak_beat_leap() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (66, 960, false, false),
                (64, 960, false, false),
            ],
            &[(53, 1920, false, false), (53, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let out = r_sp2_weak_passing(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp2.dissonance.weak_passing_stepwise");
    }

    #[test]
    fn sp2_requires_dissonant_passing_between_consonances() {
        let score = mk_two_voice_score(
            &[
                (58, 960, false, false),
                (59, 960, false, false),
                (60, 960, false, false),
            ],
            &[(53, 1920, false, false), (53, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let out = r_sp2_weak_passing(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp2.dissonance.weak_passing_stepwise");
    }

    #[test]
    fn sp2_downbeat_unison_ignores_terminal_downbeat_unison() {
        let score = mk_two_voice_score(
            &[
                (62, 960, false, false),
                (64, 960, false, false),
                (69, 1920, false, false),
            ],
            &[(53, 1920, false, false), (69, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let out = r_sp2_downbeat_unison(&ctx);
        assert!(out.is_empty());
    }

    #[test]
    fn sp2_downbeat_unison_flags_interior_downbeat() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (62, 960, false, false),
                (62, 960, false, false),
                (64, 960, false, false),
                (64, 960, false, false),
                (65, 960, false, false),
            ],
            &[
                (60, 1920, false, false),
                (62, 1920, false, false),
                (64, 1920, false, false),
            ],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let out = r_sp2_downbeat_unison(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp2.downbeat_unison_discouraged");
        assert_eq!(out[0].primary.tick, 1920);
    }

    #[test]
    fn species1_repeated_pitch_is_forbidden() {
        let score = mk_two_voice_score(
            &[(60, 1920, false, false), (60, 1920, false, false)],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let out = r_melodic_repeated_pitch_species_profiled(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "gen.melody.repeated_pitch_species_profiled");
        assert_eq!(out[0].severity, Severity::Error);
    }

    #[test]
    fn species2_repeated_pitch_is_discouraged() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (60, 960, false, false),
                (62, 960, false, false),
                (64, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let out = r_melodic_repeated_pitch_species_profiled(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "gen.melody.repeated_pitch_species_profiled");
        assert_eq!(out[0].severity, Severity::Warning);
    }

    #[test]
    fn species4_allows_tie_linked_repeat() {
        let score = mk_two_voice_score(
            &[
                (62, 960, false, false),
                (60, 960, true, false),
                (60, 960, false, true),
                (59, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        let out = r_melodic_repeated_pitch_species_profiled(&ctx);
        assert!(out.is_empty());
    }

    #[test]
    fn species4_flags_non_tied_repeat() {
        let score = mk_two_voice_score(
            &[
                (62, 960, false, false),
                (60, 960, false, false),
                (60, 960, false, false),
                (59, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        let out = r_melodic_repeated_pitch_species_profiled(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "gen.melody.repeated_pitch_species_profiled");
        assert_eq!(out[0].severity, Severity::Error);
    }

    #[test]
    fn species5_non_tied_repeat_is_discouraged() {
        let score = mk_two_voice_score(
            &[
                (62, 960, false, false),
                (60, 960, false, false),
                (60, 960, false, false),
                (59, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species5,
            rule_params: &params,
        };
        let out = r_melodic_repeated_pitch_species_profiled(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "gen.melody.repeated_pitch_species_profiled");
        assert_eq!(out[0].severity, Severity::Warning);
    }

    #[test]
    fn opening_by_position_requires_tonic_when_cp_below() {
        let score = mk_two_voice_score(
            &[(55, 1920, false, false), (57, 1920, false, false)],
            &[(60, 1920, false, false), (62, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let out = r_opening_interval_by_position(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].rule_id,
            "gen.opening.interval_by_position_species_profiled"
        );
    }

    #[test]
    fn opening_by_position_allows_dominant_when_cp_above() {
        let score = mk_two_voice_score(
            &[(67, 1920, false, false), (69, 1920, false, false)],
            &[(60, 1920, false, false), (62, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let out = r_opening_interval_by_position(&ctx);
        assert!(out.is_empty());
    }

    #[test]
    fn clausula_vera_valid_formula_passes() {
        let score = mk_two_voice_score(
            &[(71, 1920, false, false), (72, 1920, false, false)],
            &[(62, 1920, false, false), (60, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        assert!(r_clausula_vera(&ctx).is_empty());
    }

    #[test]
    fn clausula_vera_invalid_formula_fails() {
        let score = mk_two_voice_score(
            &[(69, 1920, false, false), (72, 1920, false, false)],
            &[(62, 1920, false, false), (60, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let out = r_clausula_vera(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "gen.cadence.clausula_vera_required");
    }

    #[test]
    fn two_voice_max_distance_flags_twelth_excess() {
        let score = mk_two_voice_score(
            &[(80, 1920, false, false)],
            &[(60, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let out = r_two_voice_max_distance(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "gen.spacing.two_voice_max_distance");
        assert_eq!(out[0].severity, Severity::Error);
    }

    #[test]
    fn two_voice_max_distance_allows_octave() {
        let score = mk_two_voice_score(
            &[(72, 1920, false, false)],
            &[(60, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let out = r_two_voice_max_distance(&ctx);
        assert!(out.is_empty());
    }

    #[test]
    fn two_voice_max_distance_warns_when_exceeding_tenth() {
        let score = mk_two_voice_score(
            &[(77, 1920, false, false)],
            &[(60, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let out = r_two_voice_max_distance(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "gen.spacing.two_voice_max_distance");
        assert_eq!(out[0].severity, Severity::Warning);
    }

    #[test]
    fn single_climax_ignores_tied_peak_continuation() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (67, 960, true, false),
                (67, 960, false, true),
                (65, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        assert!(r_single_climax(&ctx).is_empty());
    }

    #[test]
    fn climax_non_coincident_warns_on_shared_peak_tick() {
        let score = mk_two_voice_score(
            &[(60, 1920, false, false), (72, 1920, false, false)],
            &[(53, 1920, false, false), (65, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species1,
            rule_params: &params,
        };
        let out = r_climax_non_coincident(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].rule_id,
            "gen.melody.climax_non_coincident_between_voices"
        );
    }

    #[test]
    fn sp2_downbeat_interval_repetition_flags_consecutive_perfects() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (62, 960, false, false),
                (62, 960, false, false),
                (64, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let out = r_sp2_downbeat_interval_repetition(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].rule_id,
            "sp2.structure.downbeat_interval_repetition_limits"
        );
    }

    #[test]
    fn sp2_consonant_pattern_catalog_flags_unknown_shape() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (68, 960, false, false),
                (63, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species2,
            rule_params: &params,
        };
        let out = r_sp2_weak_consonant_pattern_catalog(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp2.weak_beat.consonant_pattern_catalog");
    }

    #[test]
    fn sp3_downbeat_interval_repetition_flags_three_perfects() {
        let score = mk_two_voice_score(
            &[
                (60, 480, false, false),
                (61, 480, false, false),
                (62, 480, false, false),
                (63, 480, false, false),
                (62, 480, false, false),
                (63, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
                (66, 480, false, false),
                (67, 480, false, false),
            ],
            &[
                (53, 1920, false, false),
                (55, 1920, false, false),
                (57, 1920, false, false),
            ],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        let out = r_sp3_downbeat_interval_repetition(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].rule_id,
            "sp3.structure.downbeat_interval_repetition_limits"
        );
    }

    #[test]
    fn sp3_perfect_interval_proximity_guard_flags_prior_same_perfect() {
        let score = mk_two_voice_score(
            &[
                (60, 480, false, false),
                (61, 480, false, false),
                (62, 480, false, false),
                (60, 480, false, false),
                (62, 480, false, false),
                (63, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        let out = r_sp3_perfect_interval_proximity_guard(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp3.perfect_interval_proximity_guard");
    }

    #[test]
    fn sp4_strict_entry_exit_profile_flags_missing_half_rest() {
        let score = mk_two_voice_score(
            &[
                (62, 960, false, false),
                (60, 960, true, false),
                (60, 960, false, true),
                (59, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        let out = r_sp4_strict_entry_exit_profile(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp4.form.strict_entry_exit_profile");
    }

    #[test]
    fn sp4_break_species_budget_warns_when_overused() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (62, 960, false, false),
                (64, 960, false, false),
                (65, 960, false, false),
                (67, 960, false, false),
                (69, 960, false, false),
                (67, 960, false, false),
                (65, 960, false, false),
                (64, 960, false, false),
                (62, 960, false, false),
            ],
            &[
                (53, 1920, false, false),
                (55, 1920, false, false),
                (57, 1920, false, false),
                (55, 1920, false, false),
                (53, 1920, false, false),
            ],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        let out = r_sp4_break_species(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp4.form.break_species_budget");
    }

    #[test]
    fn sp4_suspension_density_warns_when_too_low() {
        let score = mk_two_voice_score(
            &[
                (64, 960, false, false),
                (62, 960, true, false),
                (62, 960, false, true),
                (60, 960, true, false),
                (60, 960, false, true),
                (59, 960, false, false),
            ],
            &[
                (53, 1920, false, false),
                (55, 1920, false, false),
                (57, 1920, false, false),
            ],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        let out = r_sp4_suspension_density_minimum(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp4.suspension_density_minimum");
    }

    #[test]
    fn sp3_allows_dissonant_passing() {
        let score = mk_two_voice_score(
            &[
                (62, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
                (69, 480, false, false),
            ],
            &[(53, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        assert!(r_sp3_patterns(&ctx).is_empty());
    }

    #[test]
    fn sp3_allows_dissonant_neighbor() {
        let score = mk_two_voice_score(
            &[
                (62, 480, false, false),
                (64, 480, false, false),
                (62, 480, false, false),
                (60, 480, false, false),
            ],
            &[(53, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        assert!(r_sp3_patterns(&ctx).is_empty());
    }

    #[test]
    fn sp3_allows_canonical_double_neighbor() {
        let score = mk_two_voice_score(
            &[
                (62, 480, false, false),
                (64, 480, false, false),
                (61, 480, false, false),
                (62, 480, false, false),
            ],
            &[(53, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        assert!(r_sp3_patterns(&ctx).is_empty());
    }

    #[test]
    fn sp3_allows_cambiata_exception() {
        let score = mk_two_voice_score(
            &[
                (65, 480, false, false),
                (64, 480, false, false),
                (60, 480, false, false),
                (62, 480, false, false),
                (64, 480, false, false),
            ],
            &[(53, 1920, false, false), (53, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        assert!(r_sp3_cambiata(&ctx).is_empty());
    }

    #[test]
    fn sp3_downbeat_unison_allows_first_and_last_only() {
        let score = mk_two_voice_score(
            &[
                (60, 480, false, false),
                (62, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
                (62, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
                (67, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
                (67, 480, false, false),
                (69, 480, false, false),
            ],
            &[
                (60, 1920, false, false),
                (62, 1920, false, false),
                (64, 1920, false, false),
            ],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        let out = r_sp3_downbeat_unison(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp3.downbeat_unison_forbidden");
        assert_eq!(out[0].primary.tick, 1920);
    }

    #[test]
    fn sp3_downbeat_unison_ignores_terminal_downbeat_unison() {
        let score = mk_two_voice_score(
            &[
                (62, 480, false, false),
                (64, 480, false, false),
                (65, 480, false, false),
                (67, 480, false, false),
                (69, 1920, false, false),
            ],
            &[(53, 1920, false, false), (69, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        let out = r_sp3_downbeat_unison(&ctx);
        assert!(out.is_empty());
    }

    #[test]
    fn sp3_warns_when_leap_from_dissonance_is_not_cambiata() {
        let score = mk_two_voice_score(
            &[
                (65, 480, false, false),
                (64, 480, false, false),
                (61, 480, false, false),
                (60, 480, false, false),
                (59, 480, false, false),
            ],
            &[(53, 1920, false, false), (53, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species3,
            rule_params: &params,
        };
        let out = r_sp3_cambiata(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp3.dissonance.cambiata_limited_exception");
    }

    #[test]
    fn sp5_allows_escape_like_weak_dissonance() {
        let score = mk_two_voice_score(
            &[
                (62, 480, false, false),
                (64, 480, false, false),
                (60, 480, false, false),
                (62, 480, false, false),
            ],
            &[(53, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species5,
            rule_params: &params,
        };
        assert!(r_sp5_dissonance_patterns(&ctx).is_empty());
    }

    #[test]
    fn sp5_rejects_unlicensed_weak_dissonance_motion() {
        let score = mk_two_voice_score(
            &[
                (62, 480, false, false),
                (64, 480, false, false),
                (67, 480, false, false),
                (69, 480, false, false),
            ],
            &[(53, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species5,
            rule_params: &params,
        };
        let out = r_sp5_dissonance_patterns(&ctx);
        assert!(!out.is_empty());
        assert!(out
            .iter()
            .all(|d| d.rule_id == "sp5.dissonance.licensed_patterns_only"));
    }

    #[test]
    fn sp5_rejects_downbeat_dissonance_without_suspension() {
        let score = mk_two_voice_score(
            &[(64, 1920, false, false), (65, 1920, false, false)],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species5,
            rule_params: &params,
        };
        let out = r_sp5_strong(&ctx);
        assert!(!out.is_empty());
        assert_eq!(
            out[0].rule_id,
            "sp5.strong_beat.consonance_or_prepared_suspension_only"
        );
    }

    #[test]
    fn sp4_allows_consonant_tie_without_false_class_or_resolution_error() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (62, 960, true, false),
                (62, 960, false, true),
                (64, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        assert!(r_sp4_allowed_classes(&ctx).is_empty());
        assert!(r_sp4_step_resolution(&ctx).is_empty());
    }

    #[test]
    fn sp4_rejects_disallowed_suspension_class() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (61, 960, true, false),
                (61, 960, false, true),
                (60, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        let out = r_sp4_allowed_classes(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp4.allowed_suspension_classes_enforced");
    }

    #[test]
    fn sp4_accepts_43_suspension_with_step_resolution() {
        let score = mk_two_voice_score(
            &[
                (62, 960, false, false),
                (60, 960, true, false),
                (60, 960, false, true),
                (59, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        assert!(r_sp4_allowed_classes(&ctx).is_empty());
        assert!(r_sp4_step_resolution(&ctx).is_empty());
    }

    #[test]
    fn sp4_accepts_76_suspension_with_step_resolution() {
        let score = mk_two_voice_score(
            &[
                (67, 960, false, false),
                (65, 960, true, false),
                (65, 960, false, true),
                (64, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        assert!(r_sp4_allowed_classes(&ctx).is_empty());
        assert!(r_sp4_step_resolution(&ctx).is_empty());
    }

    #[test]
    fn sp4_accepts_98_suspension_with_step_resolution() {
        let score = mk_two_voice_score(
            &[
                (59, 960, false, false),
                (57, 960, true, false),
                (57, 960, false, true),
                (55, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        assert!(r_sp4_allowed_classes(&ctx).is_empty());
        assert!(r_sp4_step_resolution(&ctx).is_empty());
    }

    #[test]
    fn sp4_prep_uses_preparation_beat_consonance() {
        let score = mk_two_voice_score(
            &[
                (64, 960, false, false),
                (67, 960, true, false),
                (67, 960, false, true),
                (65, 960, false, false),
            ],
            &[(60, 1920, false, false), (62, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        assert!(r_sp4_prep(&ctx).is_empty());
    }

    #[test]
    fn sp4_accepts_23_suspension_below_cf() {
        let score = mk_two_voice_score(
            &[
                (60, 960, false, false),
                (58, 960, true, false),
                (58, 960, false, true),
                (57, 960, false, false),
            ],
            &[(62, 1920, false, false), (60, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        assert!(r_sp4_allowed_classes(&ctx).is_empty());
        assert!(r_sp4_step_resolution(&ctx).is_empty());
    }

    #[test]
    fn sp4_rejects_non_downward_resolution_for_dissonant_suspension() {
        let score = mk_two_voice_score(
            &[
                (62, 960, false, false),
                (60, 960, true, false),
                (60, 960, false, true),
                (62, 960, false, false),
            ],
            &[(53, 1920, false, false), (55, 1920, false, false)],
            4,
            4,
        );
        let params = BTreeMap::new();
        let ctx = RuleContext {
            score: &score,
            preset_id: &PresetId::Species4,
            rule_params: &params,
        };
        let out = r_sp4_step_resolution(&ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "sp4.suspension.step_resolution_required");
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
