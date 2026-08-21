import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
// Refresh the dashboard this often. The agent writes usage in the background;
// without polling the window freezes at whatever it first fetched.
const REFRESH_MS = 3000;
function todayKey() {
    const now = new Date();
    return now.getFullYear() * 10000 + (now.getMonth() + 1) * 100 + now.getDate();
}
function formatDuration(total) {
    if (total >= 3600) {
        const h = Math.floor(total / 3600);
        const m = Math.round((total % 3600) / 60);
        return `${h}h ${m}m`;
    }
    if (total >= 60)
        return `${Math.round(total / 60)}m`;
    return `${total}s`;
}
function percent(seconds, total) {
    if (total <= 0)
        return "0";
    return Math.round((seconds / total) * 100).toString();
}
export function App() {
    const [status, setStatus] = useState(null);
    const [summary, setSummary] = useState(null);
    const [error, setError] = useState(null);
    useEffect(() => {
        // Refresh on an interval so the dashboard tracks live usage. A failed
        // fetch keeps the last good data on screen; only a failed *first* load
        // surfaces an error, so the agent starting late isn't treated as fatal.
        let first = true;
        const refresh = () => {
            invoke("get_status")
                .then(setStatus)
                .catch((e) => {
                if (first)
                    setError(String(e));
            });
            invoke("get_day_summary", { day: todayKey() })
                .then(setSummary)
                .catch((e) => {
                if (first)
                    setError(String(e));
            });
            first = false;
        };
        refresh();
        const timer = setInterval(refresh, REFRESH_MS);
        return () => clearInterval(timer);
    }, []);
    const connected = status?.agent_connected ?? false;
    const total = summary?.totalSeconds ?? 0;
    const updated = summary ? new Date().toLocaleTimeString() : null;
    return (_jsxs("main", { className: "app", children: [_jsxs("header", { children: [_jsx("h1", { children: "Screentime" }), _jsx("p", { className: "subtitle", children: connected && status
                            ? `Connected · ${status.tracker_backend} · v${status.version}`
                            : "Agent not running — start screentime-agent to see live data" })] }), error && _jsxs("p", { className: "error", children: ["Command failed: ", error] }), _jsxs("section", { className: "hero", "aria-label": "Today's screen time", children: [_jsxs("p", { className: "eyebrow", children: ["Today", updated && _jsxs("span", { className: "updated", children: ["updated ", updated] })] }), _jsx("p", { className: "hero-total", children: formatDuration(total) }), _jsx("div", { className: "day-bar", "aria-hidden": "true", children: summary?.categories.length ? (summary.categories.map((c) => (_jsx("span", { className: "day-bar-seg", style: {
                                background: c.color ?? "#94a3b8",
                                flexGrow: c.seconds,
                            } }, c.id)))) : (_jsx("span", { className: "day-bar-seg day-bar-empty" })) })] }), !connected && !error && _jsx("p", { className: "hint", children: "Waiting for the agent\u2026" }), summary && summary.categories.length > 0 && (_jsxs("section", { className: "panel", "aria-label": "By category", children: [_jsx("h2", { className: "eyebrow", children: "By category" }), _jsx("ul", { className: "rows", children: summary.categories.map((c) => (_jsxs("li", { className: "row", children: [_jsx("span", { className: "dot", style: { background: c.color ?? "#94a3b8" } }), _jsx("span", { className: "row-label", children: c.label }), _jsx("span", { className: "row-time", children: formatDuration(c.seconds) }), _jsxs("span", { className: "row-pct", children: [percent(c.seconds, total), "%"] })] }, c.id))) })] })), summary && summary.apps.length > 0 && (_jsxs("section", { className: "panel", "aria-label": "By app", children: [_jsx("h2", { className: "eyebrow", children: "By app" }), _jsx("ul", { className: "rows", children: summary.apps.map((a) => (_jsxs("li", { className: "row", children: [_jsx("span", { className: "dot", style: { background: a.color ?? "#94a3b8" } }), _jsx("span", { className: "row-label", children: a.label }), _jsx("span", { className: "row-time", children: formatDuration(a.seconds) }), _jsxs("span", { className: "row-pct", children: [percent(a.seconds, total), "%"] })] }, a.id))) })] })), connected && summary && total === 0 && (_jsx("p", { className: "hint", children: "No screen time recorded yet today." }))] }));
}
