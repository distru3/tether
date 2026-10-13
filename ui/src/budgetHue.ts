import i18n from "./i18n";
import type { CatalogDto } from "./types/generated/CatalogDto";
import type { LimitTargetDto } from "./types/generated/LimitTargetDto";

/** The category hues of the design system (docs/DESIGN_SYSTEM.md §3). */
export type BudgetHue = "games" | "social" | "video" | "music" | "chat" | "work" | "total" | "other";

const BY_SLUG: Record<string, BudgetHue> = {
    games: "games",
    "social-media": "social",
    "short-form-video": "social",
    "video-streaming": "video",
    "music-audio": "music",
    communication: "chat",
    productivity: "work",
    creativity: "work",
    education: "work",
    finance: "work",
    "ai-chatbots": "work",
    development: "work",
};

export function hueForCategory(categoryId: number | null | undefined, catalog: CatalogDto | null): BudgetHue {
    const slug = catalog?.categories.find((c) => c.id === categoryId)?.slug;
    return (slug && BY_SLUG[slug]) || "other";
}

/** A limit's colour: its category's hue, or the app's primary category's. */
export function hueFor(target: LimitTargetDto, catalog: CatalogDto | null): BudgetHue {
    switch (target.kind) {
        case "total":
            return "total";
        case "category":
            return hueForCategory(target.id, catalog);
        case "app":
            return hueForCategory(catalog?.apps.find((a) => a.id === target.id)?.primary_category, catalog);
    }
}

/** What a hue stands for in chart legends: its kind of app, not a budget. */
export function hueLabel(hue: BudgetHue): string {
    switch (hue) {
        case "total":
            return i18n.t("limitEditor.totalScreenTime");
        case "other":
            return i18n.t("today.everythingElse");
        default:
            return i18n.t(`hue.${hue}`);
    }
}
