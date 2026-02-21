function createSvgElement(tag, attrs) {
  const el = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [k, v] of Object.entries(attrs)) {
    el.setAttribute(k, String(v));
  }
  return el;
}

function pointFromRectCenter(svg, rect) {
  const ctm = svg.getScreenCTM();
  if (!ctm) {
    return null;
  }
  const inv = ctm.inverse();
  const p = svg.createSVGPoint();
  p.x = rect.left + rect.width / 2;
  p.y = rect.top + rect.height / 2;
  return p.matrixTransform(inv);
}

function pointFromClient(svg, clientX, clientY) {
  const ctm = svg.getScreenCTM();
  if (!ctm) {
    return null;
  }
  const inv = ctm.inverse();
  const p = svg.createSVGPoint();
  p.x = clientX;
  p.y = clientY;
  return p.matrixTransform(inv);
}

function classString(el) {
  const c = el.getAttribute?.("class");
  return typeof c === "string" ? c : "";
}

function isExcludedGlyphClass(cls) {
  return (
    cls.includes("accidental") ||
    cls.includes("stem") ||
    cls.includes("ledger") ||
    cls.includes("flag") ||
    cls.includes("beam")
  );
}

function pickNoteheadElement(group) {
  const groupCls = classString(group).toLowerCase();
  if (groupCls.includes("abcjs-notehead")) {
    return group;
  }

  const direct = group.querySelector(".abcjs-notehead");
  if (direct) {
    return direct;
  }

  const allShapes = Array.from(group.querySelectorAll("path,ellipse,circle"))
    .map((el) => ({ el, rect: el.getBoundingClientRect(), cls: classString(el).toLowerCase() }))
    .filter(({ rect }) => rect.width > 0.2 && rect.height > 0.2);

  const noteheadLike = allShapes
    .filter(({ cls }) => !isExcludedGlyphClass(cls))
    .filter(({ cls }) => {
      // Prefer explicit notehead tags when present.
      return cls.includes("notehead") || cls.includes("abcjs-notehead");
    })
    .filter(({ rect }) => rect.width >= 2.5 && rect.height >= 2.5 && rect.width <= 30 && rect.height <= 30);

  if (noteheadLike.length > 0) {
    noteheadLike.sort((a, b) => {
      const ax = a.rect.left + a.rect.width * 0.5;
      const bx = b.rect.left + b.rect.width * 0.5;
      return bx - ax;
    });
    return noteheadLike[0].el;
  }

  const candidates = allShapes
    .filter(({ cls }) => !isExcludedGlyphClass(cls))
    .filter(({ rect }) => rect.width >= 2.5 && rect.height >= 2.5 && rect.width <= 30 && rect.height <= 30)
    .filter(({ rect }) => {
      const ratio = rect.width / rect.height;
      // Accidentals are often tall+thin; ledger lines are wide+flat.
      return ratio >= 0.25 && ratio <= 3.5;
    });

  if (candidates.length === 0) {
    return group;
  }

  // Accidentals sit to the left; notehead center is typically right-most compact glyph.
  candidates.sort((a, b) => {
    const ax = a.rect.left + a.rect.width * 0.5;
    const bx = b.rect.left + b.rect.width * 0.5;
    return bx - ax;
  });
  return candidates[0].el;
}

export function computeNoteCenters(svg, voices) {
  const out = new Map();
  for (const voice of voices) {
    const groups = Array.from(svg.querySelectorAll(`.abcjs-note.abcjs-v${voice.voice_index}`));
    const soundedNotes = voice.notes.filter((n) => !n.is_rest);

    for (let i = 0; i < groups.length && i < soundedNotes.length; i += 1) {
      const note = soundedNotes[i];
      const group = groups[i];
      const target = pickNoteheadElement(group);
      const rect = target.getBoundingClientRect();
      const pt = pointFromRectCenter(svg, rect);
      if (!pt) continue;
      out.set(note.note_id, pt);
      group.dataset.noteId = note.note_id;
      group.dataset.voiceIndex = String(voice.voice_index);
      group.dataset.noteIndex = String(i);
    }
  }
  return out;
}

export function clearOverlay(svg) {
  const old = svg.querySelector(".oh-overlay-layer");
  if (old) {
    old.remove();
  }
}

function ensureOverlayLayer(svg) {
  let layer = svg.querySelector(".oh-overlay-layer");
  if (!layer) {
    layer = createSvgElement("g", { class: "oh-overlay-layer" });
    svg.appendChild(layer);
  }
  return layer;
}

export function drawDiagnosticsOverlay(svg, diagnostics, centers, opts = {}) {
  const selectedDiagnosticIndex = Number.isInteger(opts.selectedDiagnosticIndex)
    ? opts.selectedDiagnosticIndex
    : -1;
  clearOverlay(svg);
  const layer = ensureOverlayLayer(svg);

  for (let diagIndex = 0; diagIndex < diagnostics.length; diagIndex += 1) {
    const diag = diagnostics[diagIndex];
    const a = centers.get(diag.primary.note_id);
    if (!a) continue;

    const circleClass = diag.severity === "warning" ? "overlay-circle-warning" : "overlay-circle-error";
    const isSelected = diagIndex === selectedDiagnosticIndex;
    layer.appendChild(
      createSvgElement("circle", {
        cx: a.x,
        cy: a.y,
        r: 12,
        class: `${circleClass}${isSelected ? " overlay-diagnostic-selected" : ""}`,
      }),
    );
    if (isSelected) {
      layer.appendChild(
        createSvgElement("circle", {
          cx: a.x,
          cy: a.y,
          r: 16,
          class: "overlay-diagnostic-focus-ring",
        }),
      );
    }

    if (diag.related?.note_id) {
      const b = centers.get(diag.related.note_id);
      if (b) {
        layer.appendChild(
          createSvgElement("line", {
            x1: a.x,
            y1: a.y,
            x2: b.x,
            y2: b.y,
            class: `overlay-line${isSelected ? " overlay-line-selected" : ""}`,
          }),
        );
      }
    }
  }

}

export function drawSelectedOverlay(svg, noteId, centers) {
  if (!noteId) return;
  const pt = centers.get(noteId);
  if (!pt) return;
  const layer = ensureOverlayLayer(svg);
  layer.appendChild(
    createSvgElement("circle", {
      cx: pt.x,
      cy: pt.y,
      r: 10,
      class: "overlay-selected",
    }),
  );
}

export function drawRomanOverlay(svg, anchors) {
  if (!Array.isArray(anchors) || anchors.length === 0) return;
  const layer = ensureOverlayLayer(svg);
  const rows = detectStaffRows(svg);
  let yMax = Number.POSITIVE_INFINITY;
  const vb = svg.viewBox?.baseVal;
  if (vb && Number.isFinite(vb.y) && Number.isFinite(vb.height) && vb.height > 0) {
    yMax = vb.y + vb.height - 6;
  } else {
    const bb = svg.getBBox();
    yMax = bb.y + bb.height - 6;
  }
  for (const anchor of anchors) {
    if (!anchor || !Number.isFinite(anchor.x) || !Number.isFinite(anchor.sourceY) || !anchor.label) {
      continue;
    }
    const row = pickStaffRow(rows, anchor.x, anchor.sourceY);
    const y = Math.min(row ? row.bottom + 14 : anchor.sourceY + 14, yMax);
    const text = createSvgElement("text", {
      x: anchor.x,
      y,
      class: "overlay-roman",
      "text-anchor": "middle",
      "dominant-baseline": "hanging",
    });
    text.textContent = anchor.label;
    if (anchor.figure) {
      const fig = createSvgElement("tspan", {
        class: "overlay-roman-figure",
        "baseline-shift": "super",
      });
      fig.textContent = anchor.figure;
      text.appendChild(fig);
    }
    layer.appendChild(text);
  }
}

function detectStaffRows(svg) {
  const candidates = Array.from(svg.querySelectorAll('[class*="staff"]'))
    .map((el) => {
      const rect = el.getBoundingClientRect();
      if (rect.width < 40 || rect.height <= 0.2) return null;
      const left = pointFromClient(svg, rect.left, rect.top);
      const right = pointFromClient(svg, rect.right, rect.top);
      const bottom = pointFromClient(svg, rect.left, rect.bottom);
      if (!left || !right || !bottom) return null;
      return {
        left: Math.min(left.x, right.x),
        right: Math.max(left.x, right.x),
        bottom: bottom.y,
      };
    })
    .filter(Boolean);

  const rows = [];
  for (const c of candidates) {
    let matched = null;
    for (const row of rows) {
      if (Math.abs(row.left - c.left) < 10 && Math.abs(row.right - c.right) < 24) {
        matched = row;
        break;
      }
    }
    if (matched) {
      matched.left = Math.min(matched.left, c.left);
      matched.right = Math.max(matched.right, c.right);
      matched.bottom = Math.max(matched.bottom, c.bottom);
    } else {
      rows.push({ ...c });
    }
  }
  return rows;
}

function pickStaffRow(rows, x, sourceY) {
  if (!rows || rows.length === 0) return null;
  let best = null;
  let bestScore = Number.POSITIVE_INFINITY;
  for (const row of rows) {
    const inX = x >= row.left - 8 && x <= row.right + 8;
    if (!inX) continue;
    const dy = Math.abs(row.bottom - sourceY);
    if (dy < bestScore) {
      bestScore = dy;
      best = row;
    }
  }
  return best;
}
