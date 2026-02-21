export const FUX_CANTUS = [
  {
    id: "dorian_below",
    label: "Dorian (Cantus below)",
    mode: "dorian",
    position: "below",
    tokens: "D, F, E, D, G, F, A, G, F, E, D,",
  },
  {
    id: "phrygian_below",
    label: "Phrygian (Cantus below)",
    mode: "phrygian",
    position: "below",
    tokens: "E, F, G, A, G, F, E, D, E,",
  },
  {
    id: "lydian_below",
    label: "Lydian (Cantus below)",
    mode: "lydian",
    position: "below",
    tokens: "F, G, A, G, F, E, D, C, D, E, F,",
  },
  {
    id: "mixolydian_below",
    label: "Mixolydian (Cantus below)",
    mode: "mixolydian",
    position: "below",
    tokens: "G, A, B, C D C B, A, G, F, G,",
  },
  {
    id: "aeolian_below",
    label: "Aeolian (Cantus below)",
    mode: "aeolian",
    position: "below",
    tokens: "A, B, C D E F E D C B, A,",
  },
  {
    id: "ionian_below",
    label: "Ionian (Cantus below)",
    mode: "ionian",
    position: "below",
    tokens: "C, D, E, C, F, E, A, G, F, E, D, C,",
  },
  {
    id: "dorian_above",
    label: "Dorian (Cantus above)",
    mode: "dorian",
    position: "above",
    tokens: "D F E D G F A G F E D",
  },
  {
    id: "phrygian_above",
    label: "Phrygian (Cantus above)",
    mode: "phrygian",
    position: "above",
    tokens: "E F G A G F E D E",
  },
  {
    id: "lydian_above",
    label: "Lydian (Cantus above)",
    mode: "lydian",
    position: "above",
    tokens: "F G A G F E D C D E F",
  },
  {
    id: "mixolydian_above",
    label: "Mixolydian (Cantus above)",
    mode: "mixolydian",
    position: "above",
    tokens: "G A B c d c B A G F G",
  },
  {
    id: "aeolian_above",
    label: "Aeolian (Cantus above)",
    mode: "aeolian",
    position: "above",
    tokens: "A B c d e f e d c B A",
  },
  {
    id: "ionian_above",
    label: "Ionian (Cantus above)",
    mode: "ionian",
    position: "above",
    tokens: "c d e c f e a g f e d c",
  },
];

export function getCantusById(id) {
  return FUX_CANTUS.find((entry) => entry.id === id) || null;
}
