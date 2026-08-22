// Shared data shapes. These mirror the Rust types in ui/src-tauri/src/lib.rs;
// hand-kept in sync until we generate them from the Rust types with ts-rs.

export interface AgentStatus {
    version: string;
    agent_connected: boolean;
    tracker_backend: string;
    filter_backend: string;
    tracking_available: boolean;
    pin_configured: boolean;
    strict_mode: boolean;
}

export interface UsageRow {
    id: number;
    label: string;
    seconds: number;
    color: string | null;
    limitSeconds: number | null;
    blocked: boolean;
}

export interface DaySummary {
    day: number;
    totalSeconds: number;
    apps: UsageRow[];
    categories: UsageRow[];
}

export type LimitTarget =
    | { kind: "app"; id: number }
    | { kind: "category"; id: number }
    | { kind: "total" };

export interface AppInfo {
    id: number;
    key: string;
    displayName: string;
    primaryCategory: number;
    tags: number[];
    userClassified: boolean;
}

export interface CategoryInfo {
    id: number;
    slug: string;
    name: string;
    kind: string;
    color: string;
    builtin: boolean;
}

export interface LimitInfo {
    id: number;
    target: LimitTarget;
    defaultMinutes: number;
    weekdayMinutes: (number | null)[];
    enabled: boolean;
}

export interface Catalog {
    apps: AppInfo[];
    categories: CategoryInfo[];
    limits: LimitInfo[];
}

export type ToastKind = "success" | "error" | "info";
export interface Toast {
    id: number;
    kind: ToastKind;
    message: string;
}

export function todayKey(): number {
    const now = new Date();
    return now.getFullYear() * 10000 + (now.getMonth() + 1) * 100 + now.getDate();
}

export function formatDuration(total: number): string {
    if (total >= 3600) {
        const h = Math.floor(total / 3600);
        const m = Math.round((total % 3600) / 60);
        return `${h}h ${m}m`;
    }
    if (total >= 60) return `${Math.round(total / 60)}m`;
    return `${total}s`;
}

export function percent(seconds: number, total: number): string {
    if (total <= 0) return "0";
    return Math.round((seconds / total) * 100).toString();
}

export function targetLabel(t: LimitTarget, catalog: Catalog | null): string {
    switch (t.kind) {
        case "total":
            return "Total screen time";
        case "app":
            return catalog?.apps.find((a) => a.id === t.id)?.displayName ?? `App #${t.id}`;
        case "category":
            return catalog?.categories.find((c) => c.id === t.id)?.name ?? `Category #${t.id}`;
    }
}