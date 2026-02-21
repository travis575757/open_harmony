# Rule Presets Mapping

## Purpose
This file defines how presets map to the canonical rule IDs in `docs/planning/rules-canonical.md`.

Machine-readable source of truth: `docs/planning/rules-presets.json`.

## Preset IDs
- `species1`
- `species2`
- `species3`
- `species4`
- `species5`
- `general_voice_leading`
- `custom`

## Mapping Policy
- Species presets are strict-species presets.
- Each species preset includes:
  - shared `core_general_rules`
  - its species-specific rule group (`speciesN_rules`)
- `general_voice_leading` includes:
  - `core_general_rules`
  - `tonal_general_rules`
  - excludes `gen.interval.p4_dissonant_against_bass_in_two_voice`
- `custom` is user-defined and derives from any base preset.
- `advanced_deferred_rules` are tracked but disabled by default in all presets.

## Rule Groups
- `core_general_rules`: shared rule baseline used by all shipped non-custom presets.
- `species1_rules`: first-species rules.
- `species2_rules`: second-species rules.
- `species3_rules`: third-species rules.
- `species4_rules`: fourth-species rules.
- `species5_rules`: fifth-species rules.
- `tonal_general_rules`: common-practice voice-leading rules.
- `advanced_deferred_rules`: invertible/advanced rules not active by default.

## Preset -> Group Crosswalk
- `species1`: `core_general_rules` + `species1_rules`
- `species2`: `core_general_rules` + `species2_rules`
- `species3`: `core_general_rules` + `species3_rules`
- `species4`: `core_general_rules` + `species4_rules`
- `species5`: `core_general_rules` + `species5_rules`
- `general_voice_leading`: `core_general_rules` + `tonal_general_rules` (with explicit exclusions/overrides)
- `custom`: no fixed groups; runtime selected rules

## Severity Defaults by Preset
- `species1..species5`: `gen.motion.direct_perfects_restricted = error`
- `general_voice_leading`: `gen.motion.direct_perfects_restricted = warning`
- `custom`: user override values

## Validation Requirements
- Every included/excluded/deferred rule in `rules-presets.json` must exist in `rules-canonical.md`.
- Every active `sp1.*..sp5.*` rule must be present in the matching species preset.
- No preset may contain the same rule in both include and exclude lists.

## Notes
- This file documents mapping intent; `rules-presets.json` is the integration artifact for engine/UI.
- Preset membership is explicit and no longer implicit via naming convention alone.
