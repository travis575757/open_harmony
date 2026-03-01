import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { exportMusicXml, importMusicXml } from "../src/musicxml.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

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

test("import honors backup/forward timing for delayed inner voices", () => {
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
  <score-partwise version="3.1">
    <part-list><score-part id="P1"><part-name>One</part-name></score-part></part-list>
    <part id="P1">
      <measure number="1">
        <attributes><divisions>8</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
        <note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice><staff>1</staff></note>
        <backup><duration>8</duration></backup>
        <forward><duration>8</duration></forward>
        <note><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration><voice>2</voice><staff>1</staff></note>
      </measure>
    </part>
  </score-partwise>`;
  const imported = importMusicXml(xml, { maxVoices: 4, presetId: "species1" });
  assert.equal(imported.voices.length, 2);
  assert.equal(imported.voices[0].notes[0].start_eighths, 0);
  assert.equal(imported.voices[1].notes[0].start_eighths, 2);
});

test("import chooses top 4 substantive voices when sparse extra voices exist", () => {
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
  <score-partwise version="3.1">
    <part-list><score-part id="P1"><part-name>Piano</part-name></score-part></part-list>
    <part id="P1">
      <measure number="1">
        <attributes><divisions>8</divisions><staves>2</staves><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
        <note><pitch><step>C</step><octave>5</octave></pitch><duration>8</duration><voice>1</voice><staff>1</staff></note>
        <note><pitch><step>D</step><octave>5</octave></pitch><duration>8</duration><voice>1</voice><staff>1</staff></note>
        <backup><duration>16</duration></backup>
        <note><pitch><step>A</step><octave>4</octave></pitch><duration>8</duration><voice>2</voice><staff>1</staff></note>
        <note><pitch><step>B</step><octave>4</octave></pitch><duration>8</duration><voice>2</voice><staff>1</staff></note>
        <backup><duration>16</duration></backup>
        <forward><duration>8</duration></forward>
        <note><pitch><step>F</step><octave>5</octave></pitch><duration>8</duration><voice>3</voice><staff>1</staff></note>
        <backup><duration>16</duration></backup>
        <note><pitch><step>C</step><octave>3</octave></pitch><duration>8</duration><voice>5</voice><staff>2</staff></note>
        <note><pitch><step>D</step><octave>3</octave></pitch><duration>8</duration><voice>5</voice><staff>2</staff></note>
        <backup><duration>16</duration></backup>
        <note><pitch><step>A</step><octave>2</octave></pitch><duration>8</duration><voice>6</voice><staff>2</staff></note>
        <note><pitch><step>B</step><octave>2</octave></pitch><duration>8</duration><voice>6</voice><staff>2</staff></note>
      </measure>
    </part>
  </score-partwise>`;
  const imported = importMusicXml(xml, { maxVoices: 4, presetId: "species1" });
  assert.equal(imported.voices.length, 4);
  const allMidi = imported.voices.flatMap((voice) => voice.notes).map((note) => note.midi);
  // Sparse voice=3 note (F5=77) should be excluded; dense voice=6 notes retained.
  assert.ok(!allMidi.includes(77));
  assert.ok(allMidi.includes(45));
  assert.ok(allMidi.includes(47));
});

test("import detects pickup measure length", () => {
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
  <score-partwise version="3.1">
    <part-list><score-part id="P1"><part-name>One</part-name></score-part></part-list>
    <part id="P1">
      <measure number="0" implicit="yes">
        <attributes><divisions>8</divisions><time><beats>3</beats><beat-type>4</beat-type></time></attributes>
        <note><pitch><step>C</step><octave>5</octave></pitch><duration>4</duration><voice>1</voice><staff>1</staff></note>
      </measure>
      <measure number="1">
        <note><pitch><step>D</step><octave>5</octave></pitch><duration>8</duration><voice>1</voice><staff>1</staff></note>
      </measure>
    </part>
  </score-partwise>`;
  const imported = importMusicXml(xml, { maxVoices: 4, presetId: "species1" });
  assert.equal(imported.pickup_eighths, 1);
});

test("import ignores grace notes for timeline and voice alignment", () => {
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
  <score-partwise version="3.1">
    <part-list><score-part id="P1"><part-name>One</part-name></score-part></part-list>
    <part id="P1">
      <measure number="1">
        <attributes><divisions>16</divisions><time><beats>3</beats><beat-type>4</beat-type></time></attributes>
        <note><pitch><step>C</step><octave>5</octave></pitch><duration>12</duration><voice>1</voice><staff>1</staff></note>
        <note><grace/><pitch><step>D</step><octave>5</octave></pitch><type>16th</type><voice>1</voice><staff>1</staff></note>
        <note><grace/><pitch><step>C</step><octave>5</octave></pitch><type>16th</type><voice>1</voice><staff>1</staff></note>
        <note><pitch><step>E</step><octave>5</octave></pitch><duration>4</duration><voice>1</voice><staff>1</staff></note>
        <note><pitch><step>F</step><octave>5</octave></pitch><duration>8</duration><voice>1</voice><staff>1</staff></note>
        <note><pitch><step>G</step><octave>5</octave></pitch><duration>8</duration><voice>1</voice><staff>1</staff></note>
        <backup><duration>48</duration></backup>
        <note><pitch><step>F</step><octave>3</octave></pitch><duration>32</duration><voice>5</voice><staff>2</staff></note>
      </measure>
    </part>
  </score-partwise>`;
  const imported = importMusicXml(xml, { maxVoices: 4, presetId: "species1" });
  assert.equal(imported.voices.length, 2);
  const upper = imported.voices[0].notes.filter((n) => !n.is_rest);
  const lower = imported.voices[1].notes.filter((n) => !n.is_rest);
  // Grace notes should not appear as timed events.
  assert.equal(upper.length, 4);
  // Lower staff onset should align at measure start, not shifted by grace-note faux duration.
  assert.equal(lower[0].start_eighths, 0);
});

test("mozart K330/2 import matches music21-derived oracle for pickup and onsets", async () => {
  const musicxmlPath = path.resolve(__dirname, "../mozart_k330_2_when_in_rome_score.musicxml");
  const fixturePath = path.resolve(__dirname, "fixtures/mozart_k330_2_when_in_rome.music21.json");
  const xml = await readFile(musicxmlPath, "utf8");
  const fixture = JSON.parse(await readFile(fixturePath, "utf8"));

  const imported = importMusicXml(xml, { maxVoices: 4, presetId: "general_voice_leading" });
  assert.equal(imported.pickup_eighths, fixture.pickup_eighths);

  const selectedPairs = imported.voices.map((voice) => [voice.source_staff_num, voice.source_voice_num]);
  assert.deepEqual(selectedPairs, [
    [1, 1],
    [1, 2],
    [2, 5],
    [2, 6],
  ]);

  const fixtureStaff2 = fixture.staffs.find((staff) => staff.staff_num === 2);
  assert.ok(fixtureStaff2);
  const oracleStarts = new Set(fixtureStaff2.sounding_start_eighths.map((v) => Number(v.toFixed(6))));
  const importedStaff2Starts = new Set(
    imported.voices
      .filter((voice) => voice.source_staff_num === 2)
      .flatMap((voice) => voice.notes.filter((n) => !n.is_rest).map((n) => Number(n.start_eighths.toFixed(6)))),
  );

  for (const start of importedStaff2Starts) {
    assert.ok(oracleStarts.has(start), `imported lower-staff onset ${start} is not in music21 oracle`);
  }

  for (const expectedStart of [3, 7, 9, 12, 15]) {
    assert.ok(
      importedStaff2Starts.has(expectedStart),
      `expected lower-staff onset ${expectedStart} missing after import`,
    );
  }
});
