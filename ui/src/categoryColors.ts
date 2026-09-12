const FALLBACK_PALETTE = [
  "#DCA06D",
  "#A55B4B",
  "#C27D60",
  "#E8B88A",
  "#9D4F6A",
  "#B56B55",
  "#C8E66E",
  "#8E3E63",
  "#d4b8af",
];

const LABEL_COLORS: Record<string, string> = {
  "ai assistants": "#DCA06D",
  "social media": "#A55B4B",
  "short-form video": "#E8B88A",
  "video & streaming": "#9D4F6A",
  "music & audio": "#C27D60",
  news: "#d4b8af",
  shopping: "#B56B55",
  communication: "#DCA06D",
  "productivity & office": "#f5ede8",
  "creativity & design": "#C8E66E",
  "education & reading": "#E8B88A",
  finance: "#9b7e7a",
  uncategorized: "#703B52",
  "development & tools": "#9b7e7a",
  "utilities & system": "#5a4350",
  "adult content": "#e07070",
  "gambling & betting": "#cf5353",
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
  return LABEL_COLORS[normalized] ?? FALLBACK_PALETTE[normalized ? stableIndex(normalized) : fallbackIndex % FALLBACK_PALETTE.length] ?? "#A55B4B";
}
