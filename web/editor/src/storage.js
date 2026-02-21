const PRESET_KEY = "oh.cp.custom_presets.v1";
const SETTINGS_KEY = "oh.cp.editor_settings.v1";

function safeParse(jsonText, fallback) {
  try {
    return JSON.parse(jsonText);
  } catch {
    return fallback;
  }
}

export function loadCustomProfiles() {
  const raw = localStorage.getItem(PRESET_KEY);
  if (!raw) {
    return [];
  }
  const parsed = safeParse(raw, []);
  if (!Array.isArray(parsed)) {
    return [];
  }
  return parsed.filter((entry) => typeof entry === "object" && entry && typeof entry.name === "string");
}

export function saveCustomProfiles(profiles) {
  localStorage.setItem(PRESET_KEY, JSON.stringify(profiles));
}

export function loadEditorSettings() {
  const raw = localStorage.getItem(SETTINGS_KEY);
  if (!raw) {
    return null;
  }
  const parsed = safeParse(raw, null);
  if (!parsed || typeof parsed !== "object") {
    return null;
  }
  return parsed;
}

export function saveEditorSettings(settings) {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
}
