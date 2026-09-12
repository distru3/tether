const FALLBACK_PALETTE = [
  "#8B5CF6", // Purple
  "#3B82F6", // Blue
  "#EC4899", // Pink
  "#10B981", // Emerald
  "#F97316", // Orange
  "#06B6D4", // Cyan
  "#F59E0B", // Amber
  "#EF4444", // Crimson
  "#14B8A6", // Teal
  "#84CC16", // Lime
  "#6366F1", // Indigo
  "#A855F7", // Violet
];

const LABEL_COLORS: Record<string, string> = {
  "games": "#8B5CF6",
  "social media": "#3B82F6",
  "short-form video": "#EC4899",
  "video & streaming": "#EF4444",
  "music & audio": "#10B981",
  "news": "#F97316",
  "shopping": "#F59E0B",
  "communication": "#06B6D4",
  "productivity & office": "#0EA5E9",
  "creativity & design": "#A855F7",
  "education & reading": "#14B8A6",
  "finance": "#84CC16",
  "ai assistants": "#6366F1",
  "ai chatbots": "#6366F1",
  "development & tools": "#64748B",
  "utilities & system": "#475569",
  "adult content": "#BE123C",
  "gambling & betting": "#991B1B",
  "gambling": "#991B1B",
  "uncategorized": "#94A3B8",
};

function normalize(value: string): string {
  return value.trim().toLowerCase().replace(/\s+/g, " ");
}

function stableIndex(value: string): number {
  let hash = 0;
  for (const character of value) hash = (hash * 31 + character.charCodeAt(0)) | 0;
  return Math.abs(hash) % FALLBACK_PALETTE.length;
}

export function colorForCategory(
  value: string | null | undefined,
  explicitColor?: string | null,
  fallbackIndex = 0,
): string {
  const normalized = normalize(value ?? "");
  if (LABEL_COLORS[normalized]) return LABEL_COLORS[normalized];

  if (explicitColor && explicitColor.trim() && explicitColor !== "#DCA06D" && explicitColor !== "#A55B4B") {
    return explicitColor;
  }

  return (normalized ? FALLBACK_PALETTE[stableIndex(normalized)] : FALLBACK_PALETTE[fallbackIndex % FALLBACK_PALETTE.length]) ?? "#A855F7";
}

/**
 * Returns an rgba color string from a 6-digit hex and opacity (0 to 1).
 */
export function categoryBgColor(hexColor: string, opacity = 0.14): string {
  const clean = hexColor.replace("#", "");
  if (clean.length === 6) {
    const r = parseInt(clean.substring(0, 2), 16);
    const g = parseInt(clean.substring(2, 4), 16);
    const b = parseInt(clean.substring(4, 6), 16);
    return `rgba(${r}, ${g}, ${b}, ${opacity})`;
  }
  return `rgba(99, 102, 241, ${opacity})`;
}

/**
 * Returns an rgba border color string with slightly higher opacity.
 */
export function categoryBorderColor(hexColor: string, opacity = 0.28): string {
  return categoryBgColor(hexColor, opacity);
}
