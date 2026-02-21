import { resolvePreset } from "./presets.js";

function sortedUnique(values) {
  return [...new Set(values)].sort();
}

export function resolveUiRuleState(schema, state) {
  const basePresetId = state.preset_id === "custom" ? state.custom_base_preset_id : state.preset_id;
  const base = resolvePreset(schema, basePresetId, {}, basePresetId);
  const baseActive = new Set(base.active_rule_ids);

  const enabled = [];
  const disabled = [];

  for (const rid of state.rule_overrides.enabled_rule_ids) {
    if (!baseActive.has(rid)) {
      enabled.push(rid);
    }
  }
  for (const rid of state.rule_overrides.disabled_rule_ids) {
    if (baseActive.has(rid)) {
      disabled.push(rid);
    }
  }

  const resolvedActive = new Set(base.active_rule_ids);
  for (const rid of enabled) resolvedActive.add(rid);
  for (const rid of disabled) resolvedActive.delete(rid);

  if (state.preset_id === "custom") {
    return {
      active_rule_ids: [...resolvedActive].sort(),
      enabled_rule_ids: [...resolvedActive].sort(),
      disabled_rule_ids: [],
      severity_overrides: { ...state.rule_overrides.severity_overrides },
      rule_params: {},
      base_active_rule_ids: sortedUnique(base.active_rule_ids),
    };
  }

  return {
    active_rule_ids: [...resolvedActive].sort(),
    enabled_rule_ids: sortedUnique(enabled),
    disabled_rule_ids: sortedUnique(disabled),
    severity_overrides: { ...state.rule_overrides.severity_overrides },
    rule_params: {},
    base_active_rule_ids: sortedUnique(base.active_rule_ids),
  };
}

export function toggleRuleOverride(state, ruleId, checked, isBaseActive) {
  const enabled = new Set(state.rule_overrides.enabled_rule_ids);
  const disabled = new Set(state.rule_overrides.disabled_rule_ids);

  if (checked) {
    disabled.delete(ruleId);
    if (!isBaseActive) {
      enabled.add(ruleId);
    } else {
      enabled.delete(ruleId);
    }
  } else {
    enabled.delete(ruleId);
    if (isBaseActive) {
      disabled.add(ruleId);
    } else {
      disabled.delete(ruleId);
    }
  }

  state.rule_overrides.enabled_rule_ids = [...enabled].sort();
  state.rule_overrides.disabled_rule_ids = [...disabled].sort();
}
