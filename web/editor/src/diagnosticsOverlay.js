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

    const circleClass =
      diag.severity === "warning"
        ? "overlay-circle-warning"
        : diag.severity === "info"
          ? "overlay-circle-info"
          : "overlay-circle-error";
    const lineClass = diag.severity === "info" ? "overlay-line-info" : "overlay-line";
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
            class: `${lineClass}${isSelected ? " overlay-line-selected" : ""}`,
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
  for (const anchor of anchors) {
    if (!anchor || !Number.isFinite(anchor.x) || !Number.isFinite(anchor.sourceY) || !anchor.label) {
      continue;
    }
    const row = pickStaffRow(rows, anchor.x, anchor.sourceY);
    const y = row ? row.bottom + 14 : anchor.sourceY + 14;
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
      const leftTop = pointFromClient(svg, rect.left, rect.top);
      const rightTop = pointFromClient(svg, rect.right, rect.top);
      const leftBottom = pointFromClient(svg, rect.left, rect.bottom);
      if (!leftTop || !rightTop || !leftBottom) return null;
      return {
        left: Math.min(leftTop.x, rightTop.x),
        right: Math.max(leftTop.x, rightTop.x),
        top: Math.min(leftTop.y, leftBottom.y),
        bottom: Math.max(leftTop.y, leftBottom.y),
      };
    })
    .filter(Boolean);
  return groupStaffRowsBySystem(candidates);
}

function median(values) {
  if (!values || values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 0) {
    return (sorted[mid - 1] + sorted[mid]) / 2;
  }
  return sorted[mid];
}

function lineGap(a, b) {
  if (b.top > a.bottom) return b.top - a.bottom;
  if (a.top > b.bottom) return a.top - b.bottom;
  return 0;
}

function groupStaffRowsBySystem(candidates) {
  const rows = [];
  const ordered = [...candidates].sort((a, b) => a.top - b.top);
  for (const c of ordered) {
    const centerY = (c.top + c.bottom) / 2;
    let matched = null;
    let bestGap = Number.POSITIVE_INFINITY;
    for (const row of rows) {
      const horizontalMatch = Math.abs(row.left - c.left) < 10 && Math.abs(row.right - c.right) < 24;
      if (!horizontalMatch) continue;
      const gap = lineGap(row, c);
      if (gap <= 14 && gap < bestGap) {
        matched = row;
        bestGap = gap;
      }
    }
    if (matched) {
      matched.left = Math.min(matched.left, c.left);
      matched.right = Math.max(matched.right, c.right);
      matched.top = Math.min(matched.top, c.top);
      matched.bottom = Math.max(matched.bottom, c.bottom);
      matched.centerY = (matched.top + matched.bottom) / 2;
    } else {
      rows.push({
        left: c.left,
        right: c.right,
        top: c.top,
        bottom: c.bottom,
        centerY,
        systemIndex: 0,
      });
    }
  }
  rows.sort((a, b) => a.centerY - b.centerY);
  if (rows.length <= 1) return rows;

  const gaps = [];
  for (let i = 1; i < rows.length; i += 1) {
    gaps.push(rows[i].centerY - rows[i - 1].centerY);
  }
  const systemBreakGap = Math.min(90, Math.max(32, median(gaps) * 1.6));
  let systemIndex = 0;
  rows[0].systemIndex = systemIndex;
  for (let i = 1; i < rows.length; i += 1) {
    if (rows[i].centerY - rows[i - 1].centerY > systemBreakGap) {
      systemIndex += 1;
    }
    rows[i].systemIndex = systemIndex;
  }

  return rows;
}

function pickStaffRow(rows, x, sourceY) {
  if (!rows || rows.length === 0) return null;
  let nearest = null;
  let nearestScore = Number.POSITIVE_INFINITY;
  for (const row of rows) {
    const dy = Math.abs((row.centerY ?? row.bottom) - sourceY);
    // Prefer rows that also span the anchor X, but never require it.
    const inX = x >= row.left - 8 && x <= row.right + 8;
    const score = dy + (inX ? 0 : 24);
    if (score < nearestScore) {
      nearestScore = score;
      nearest = row;
    }
  }
  if (!nearest) return null;

  const systemRows = rows.filter((row) => row.systemIndex === nearest.systemIndex);
  let best = systemRows[0] ?? nearest;
  for (const row of systemRows) {
    if (row.bottom > best.bottom) {
      best = row;
    }
  }
  return best;
}
