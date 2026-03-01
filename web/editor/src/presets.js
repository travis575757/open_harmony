export const BUILTIN_PRESET_IDS = [
  "species1",
  "species2",
  "species3",
  "species4",
  "species5",
  "general_voice_leading",
  "moderate_classical",
  "relaxed",
  "custom",
];

export function presetLabel(presetId) {
  switch (presetId) {
    case "species1":
      return "Species 1 (1:1)";
    case "species2":
      return "Species 2 (2:1)";
    case "species3":
      return "Species 3 (4:1)";
    case "species4":
      return "Species 4 (Syncopation)";
    case "species5":
      return "Species 5 (Florid)";
    case "general_voice_leading":
      return "General Voice Leading";
    case "moderate_classical":
      return "Moderate / Classical";
    case "relaxed":
      return "Relaxed";
    case "custom":
      return "Custom";
    default:
      return presetId;
  }
}

function cloneObj(obj) {
  return JSON.parse(JSON.stringify(obj));
}

export function resolvePreset(schema, presetId, overrides = {}, basePresetIdForCustom = "species1") {
  const effectiveId = presetId === "custom" ? basePresetIdForCustom : presetId;
  const preset = schema.presets.find((entry) => entry.preset_id === effectiveId);
  if (!preset) {
    throw new Error(`Unknown preset '${effectiveId}'`);
  }

  const active = new Set();
  for (const group of preset.include_groups ?? []) {
    for (const rid of schema.groups[group] ?? []) {
      active.add(rid);
    }
  }
  for (const rid of preset.include_rules ?? []) {
    active.add(rid);
  }
  for (const group of preset.exclude_groups ?? []) {
    for (const rid of schema.groups[group] ?? []) {
      active.delete(rid);
    }
  }
  for (const rid of preset.exclude_rules ?? []) {
    active.delete(rid);
  }

  for (const rid of overrides.enabled_rule_ids ?? []) {
    active.add(rid);
  }
  for (const rid of overrides.disabled_rule_ids ?? []) {
    active.delete(rid);
  }

  const severity = {
    ...(preset.severity_overrides ?? {}),
    ...(overrides.severity_overrides ?? {}),
  };

  const ruleParams = {
    ...(preset.rule_param_defaults ?? {}),
    ...(overrides.rule_params ?? {}),
  };

  return {
    effective_preset_id: effectiveId,
    active_rule_ids: [...active].sort(),
    severity_overrides: severity,
    rule_params: ruleParams,
    deferred_rule_ids: cloneObj(preset.deferred_rules ?? []),
  };
}

export function buildRuleCatalog(schema) {
  const groupByRule = {};
  for (const [group, ruleIds] of Object.entries(schema.groups ?? {})) {
    for (const rid of ruleIds) {
      groupByRule[rid] = group;
    }
  }
  const allRuleIds = [...new Set(Object.values(schema.groups ?? {}).flat())].sort();
  return allRuleIds.map((ruleId) => ({
    rule_id: ruleId,
    group: groupByRule[ruleId] ?? "unknown",
  }));
}

export function getBasePresetIds(schema) {
  return (schema.preset_ids ?? []).filter((id) => id !== "custom");
}
