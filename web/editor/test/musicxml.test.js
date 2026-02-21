import test from "node:test";
import assert from "node:assert/strict";
import { exportMusicXml, importMusicXml } from "../src/musicxml.js";

function sampleState() {
  return {
    mode: "major",
    time_signature: { numerator: 4, denominator: 4 },
    voices: [
      {
        voice_index: 0,
        name: "Soprano",
        notes: [
          { note_id: "v0_n0", midi: 72, duration_eighths: 2, tie_start: false, tie_end: false },
          { note_id: "v0_n1", midi: 74, duration_eighths: 2, tie_start: false, tie_end: false },
          { note_id: "v0_n2", midi: 76, duration_eighths: 4, tie_start: false, tie_end: false },
        ],
      },
      {
        voice_index: 1,
        name: "Bass",
        notes: [
          { note_id: "v1_n0", midi: 48, duration_eighths: 4, tie_start: false, tie_end: false },
          { note_id: "v1_n1", midi: 50, duration_eighths: 4, tie_start: false, tie_end: false },
        ],
      },
    ],
  };
}

test("export + import roundtrip preserves voice count and core note data", () => {
  const xml = exportMusicXml(sampleState());
  const imported = importMusicXml(xml, { maxVoices: 4, presetId: "species1" });

  assert.equal(imported.voices.length, 2);
  assert.equal(imported.time_signature.numerator, 4);
  assert.equal(imported.time_signature.denominator, 4);

  assert.equal(imported.voices[0].notes[0].midi, 72);
  assert.equal(imported.voices[1].notes[0].midi, 48);
  assert.ok(imported.voices[0].notes.length >= 2);
});

test("imports basic external musicxml structure", () => {
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
  <score-partwise version="3.1">
    <part-list>
      <score-part id="P1"><part-name>One</part-name></score-part>
    </part-list>
    <part id="P1">
      <measure number="1">
        <attributes>
          <divisions>8</divisions>
          <key><fifths>0</fifths><mode>major</mode></key>
          <time><beats>3</beats><beat-type>4</beat-type></time>
        </attributes>
        <note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice></note>
        <note><pitch><step>D</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice></note>
      </measure>
    </part>
  </score-partwise>`;

  const imported = importMusicXml(xml, { maxVoices: 4, presetId: "species2" });
  assert.equal(imported.mode, "major");
  assert.equal(imported.time_signature.numerator, 3);
  assert.equal(imported.voices.length, 1);
  assert.equal(imported.voices[0].notes[0].midi, 60);
  assert.equal(imported.voices[0].notes[1].midi, 62);
});

test("export writes semantic note types and dots instead of fixed eighth", () => {
  const xml = exportMusicXml({
    mode: "major",
    time_signature: { numerator: 4, denominator: 4 },
    voices: [
      {
        voice_index: 0,
        name: "One",
        notes: [
          { note_id: "n0", midi: 60, duration_eighths: 8, tie_start: false, tie_end: false },
          { note_id: "n1", midi: 62, duration_eighths: 4, tie_start: false, tie_end: false },
          { note_id: "n2", midi: 64, duration_eighths: 2, tie_start: false, tie_end: false },
          { note_id: "n3", midi: 65, duration_eighths: 1, tie_start: false, tie_end: false },
          { note_id: "n4", midi: 67, duration_eighths: 3, tie_start: false, tie_end: false },
        ],
      },
    ],
  });

  assert.match(xml, /<type>whole<\/type>/);
  assert.match(xml, /<type>half<\/type>/);
  assert.match(xml, /<type>quarter<\/type>/);
  assert.match(xml, /<type>eighth<\/type>/);
  assert.match(xml, /<type>quarter<\/type><dot\/>/);
});

test("import handles missing divisions as quarter-based default", () => {
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
  <score-partwise version="3.1">
    <part-list><score-part id="P1"><part-name>One</part-name></score-part></part-list>
    <part id="P1">
      <measure number="1">
        <attributes><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
        <note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice></note>
      </measure>
    </part>
  </score-partwise>`;
  const imported = importMusicXml(xml, { maxVoices: 4, presetId: "species1" });
  assert.equal(imported.voices[0].notes[0].duration_eighths, 8);
});

test("import falls back to <type> when duration is missing", () => {
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
  <score-partwise version="3.1">
    <part-list><score-part id="P1"><part-name>One</part-name></score-part></part-list>
    <part id="P1">
      <measure number="1">
        <attributes><divisions>8</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
        <note><pitch><step>C</step><octave>4</octave></pitch><type>whole</type><voice>1</voice></note>
      </measure>
    </part>
  </score-partwise>`;
  const imported = importMusicXml(xml, { maxVoices: 4, presetId: "species1" });
  assert.equal(imported.voices[0].notes[0].duration_eighths, 8);
});
