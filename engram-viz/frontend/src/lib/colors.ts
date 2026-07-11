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
