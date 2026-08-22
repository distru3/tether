// Shared data shapes. These mirror the Rust types in ui/src-tauri/src/lib.rs;
// hand-kept in sync until we generate them from the Rust types with ts-rs.
export function todayKey() {
    const now = new Date();
    return now.getFullYear() * 10000 + (now.getMonth() + 1) * 100 + now.getDate();
}
export function formatDuration(total) {
    if (total >= 3600) {
        const h = Math.floor(total / 3600);
        const m = Math.round((total % 3600) / 60);
        return `${h}h ${m}m`;
    }
    if (total >= 60)
        return `${Math.round(total / 60)}m`;
    return `${total}s`;
}
export function percent(seconds, total) {
    if (total <= 0)
        return "0";
    return Math.round((seconds / total) * 100).toString();
}
export function targetLabel(t, catalog) {
    switch (t.kind) {
        case "total":
            return "Total screen time";
        case "app":
            return catalog?.apps.find((a) => a.id === t.id)?.displayName ?? `App #${t.id}`;
        case "category":
            return catalog?.categories.find((c) => c.id === t.id)?.name ?? `Category #${t.id}`;
    }
}
