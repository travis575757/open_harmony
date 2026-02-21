import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { buildRuleCatalog, resolvePreset } from "../src/presets.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const schemaPath = path.resolve(__dirname, "../../../docs/planning/rules-presets.json");
const schema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));

test("species1 preset resolves expected rule groups", () => {
  const out = resolvePreset(schema, "species1");
  assert.ok(out.active_rule_ids.includes("sp1.rhythm.one_to_one_only"));
  assert.ok(out.active_rule_ids.includes("gen.motion.parallel_perfects_forbidden"));
  assert.ok(!out.active_rule_ids.includes("gen.voice.leading_tone_not_doubled"));
});

test("custom preset applies overrides", () => {
  const out = resolvePreset(
    schema,
    "custom",
    {
      enabled_rule_ids: ["gen.voice.leading_tone_not_doubled"],
      disabled_rule_ids: ["gen.motion.parallel_perfects_forbidden"],
      severity_overrides: { "gen.motion.direct_perfects_restricted": "warning" },
    },
    "species1",
  );

  assert.ok(out.active_rule_ids.includes("gen.voice.leading_tone_not_doubled"));
  assert.ok(!out.active_rule_ids.includes("gen.motion.parallel_perfects_forbidden"));
  assert.equal(out.severity_overrides["gen.motion.direct_perfects_restricted"], "warning");
});

test("rule catalog includes grouped rows", () => {
  const rows = buildRuleCatalog(schema);
  assert.ok(rows.length > 10);
  const firstSpecies = rows.find((r) => r.rule_id === "sp1.rhythm.one_to_one_only");
  assert.equal(firstSpecies.group, "species1_rules");
});
