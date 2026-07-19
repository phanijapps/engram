//! Community color helper: hash a community label into a stable HSL color.

export function communityColor(label: number | undefined): string {
  if (label === undefined || label === null) {
    return "#6e7681"; // muted gray for unclassified nodes
  }
  // Spread hues across the spectrum deterministically.
  const hue = (label * 47) % 360;
  const saturation = 62;
  const lightness = 60;
  return `hsl(${hue}, ${saturation}%, ${lightness}%)`;
}

//! Entity-kind color map — consistent palette for the legend + grouping view.
//! Lowercased keys so lookups are case-insensitive.
export const kindColors: Record<string, string> = {
  function: "#39c5cf",
  method: "#39c5cf",
  closure: "#39c5cf",
  struct: "#bc8cff",
  class: "#bc8cff",
  trait: "#d2a8ff",
  interface: "#d2a8ff",
  enum: "#bc8cff",
  module: "#d29922",
  variable: "#3fb950",
  constant: "#3fb950",
  macro: "#f85149",
  typealias: "#7d8590",
  impl: "#58a6ff",
  unknown: "#6e7681",
};

/** Returns a stable color for an EntityKind string (case-insensitive). */
export function kindColor(kind: string): string {
  return kindColors[kind.toLowerCase()] ?? "#6e7681";
}
