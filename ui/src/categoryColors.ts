const FALLBACK_PALETTE = [
  "#5b23ff",
  "#008bff",
  "#e4ff30",
  "#8d6bff",
  "#4db8ff",
  "#b6c6ff",
  "#858196",
  "#e4e5eb",
];

const LABEL_COLORS: Record<string, string> = {
  "ai assistants": "#5b23ff",
  "social media": "#008bff",
  "short-form video": "#e4ff30",
  "video & streaming": "#8d6bff",
  "music & audio": "#4db8ff",
  news: "#b6c6ff",
  shopping: "#7f9dff",
  communication: "#52c3ff",
  "productivity & office": "#e4e5eb",
  "creativity & design": "#9d7dff",
  "education & reading": "#91baff",
  finance: "#858196",
  uncategorized: "#a8a7b5",
  "development & tools": "#858196",
  "utilities & system": "#615d76",
  "adult content": "#ff6b8a",
  "gambling & betting": "#d94f7c",
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
  if (explicitColor && explicitColor.trim()) return explicitColor;

  const normalized = normalize(value ?? "");
  return LABEL_COLORS[normalized] ?? FALLBACK_PALETTE[normalized ? stableIndex(normalized) : fallbackIndex % FALLBACK_PALETTE.length] ?? "#5b23ff";
}
