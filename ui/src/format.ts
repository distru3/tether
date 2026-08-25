import type { CatalogDto } from "./types/generated/CatalogDto";
import type { LimitTargetDto } from "./types/generated/LimitTargetDto";

export function todayKey(now = new Date()): number {
    return now.getFullYear() * 10000 + (now.getMonth() + 1) * 100 + now.getDate();
}

export function formatDuration(totalSeconds: number): string {
    const s = Math.max(0, Math.round(totalSeconds));
    if (s >= 3600) {
        const h = Math.floor(s / 3600);
        const m = Math.round((s % 3600) / 60);
        return m >= 60 ? `${h + 1}h 00m` : `${h}h ${m}m`;
    }
    if (s >= 60) return `${Math.round(s / 60)}m`;
    return `${s}s`;
}

export function heroParts(totalSeconds: number): Array<[string, string]> {
    const s = Math.max(0, Math.round(totalSeconds));
    if (s >= 3600) {
        let h = Math.floor(s / 3600);
        let m = Math.round((s % 3600) / 60);
        if (m >= 60) {
            h += 1;
            m = 0;
        }
        return [
            [String(h), "h"],
            [pad2(m), "m"],
        ];
    }
    if (s >= 60) return [[String(Math.round(s / 60)), "m"]];
    return [[String(s), "s"]];
}

function pad2(n: number): string {
    return n < 10 ? `0${n}` : String(n);
}

export function sharePercent(seconds: number, total: number): number {
    if (total <= 0) return 0;
    return Math.min(100, Math.round((seconds / total) * 100));
}

const WEEKDAYS = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"] as const;
const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"] as const;

export function formatDateline(d: Date): string {
    return `${WEEKDAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]}`;
}

export function formatClock(d: Date): string {
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function effectClause(effectiveUtc: string | null | undefined): string {
    if (!effectiveUtc) return " Applied.";
    const t = new Date(effectiveUtc);
    if (Number.isNaN(t.getTime())) return " Applied.";
    if (t.getTime() <= Date.now()) return " Applied.";
    return ` Takes effect ${formatClock(t)}.`;
}

export function targetLabel(target: LimitTargetDto, catalog: CatalogDto | null): string {
    switch (target.kind) {
        case "total":
            return "Total screen time";
        case "app":
            return catalog?.apps.find((a) => a.id === target.id)?.display_name ?? `App #${target.id}`;
        case "category":
            return catalog?.categories.find((c) => c.id === target.id)?.name ?? `Category #${target.id}`;
    }
}
