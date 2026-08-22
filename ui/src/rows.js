import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { formatDuration, percent } from "./types";
/** One row in a breakdown list: swatch + name, thin proportional bar, duration,
 * share, and (if a limit exists) a clickable daily-budget chip. */
export function UsageRowItem({ row, total, limit, onEditLimit }) {
    return (_jsxs("li", { className: "row", children: [_jsx("span", { className: "dot", style: { background: row.color ?? "#94a3b8" } }), _jsxs("span", { className: "row-label", children: [row.label, row.blocked && _jsx("span", { className: "blocked-tag", children: "blocked" })] }), _jsx("span", { className: "row-bar", children: _jsx("span", { className: "row-bar-fill", style: {
                        background: row.color ?? "#94a3b8",
                        width: `${percent(row.seconds, total)}%`,
                    } }) }), _jsx("span", { className: "row-time", children: formatDuration(row.seconds) }), _jsxs("span", { className: "row-pct", children: [percent(row.seconds, total), "%"] }), limit && (_jsxs("button", { className: "chip", onClick: () => onEditLimit(limit.target), title: `${formatDuration(limit.defaultMinutes * 60)}/day limit`, children: [formatDuration(limit.defaultMinutes * 60), "/day"] }))] }));
}
/** A row in the limits list, with edit/remove actions. */
export function LimitRowItem({ limit, catalog, onEdit, onRemove, }) {
    const label = targetLabelLocal(limit.target, catalog);
    return (_jsxs("li", { className: "row limit-row", children: [_jsx("span", { className: "row-label", children: label }), _jsxs("span", { className: "row-time", children: [formatDuration(limit.defaultMinutes * 60), "/day"] }), _jsx("span", { className: `toggle ${limit.enabled ? "on" : ""}`, children: _jsx("span", { className: "toggle-knob" }) }), _jsx("button", { className: "ghost", onClick: () => onEdit(limit.target), children: "edit" }), _jsx("button", { className: "ghost", onClick: () => onRemove(limit.target), children: "remove" })] }));
}
function targetLabelLocal(t, catalog) {
    switch (t.kind) {
        case "total":
            return "Total screen time";
        case "app":
            return catalog.apps.find((a) => a.id === t.id)?.displayName ?? `App #${t.id}`;
        case "category":
            return catalog.categories.find((c) => c.id === t.id)?.name ?? `Category #${t.id}`;
    }
}
