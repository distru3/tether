import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
/** The signature: the whole day as one frontier line.
 *
 * Each category occupies its proportional share in its own colour. A bright
 * marker sits at the current fraction of the day elapsed, so "spent" visibly
 * approaches it. The spent portion is shaded, the remaining budget is dim. */
export function Horizon({ summary, nowMarker = true }) {
    const total = summary.totalSeconds;
    const cats = summary.categories;
    const now = new Date();
    const dayFraction = (now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds()) / 86400;
    const markerPct = Math.min(100, dayFraction * 100);
    return (_jsxs("div", { className: "horizon", "aria-hidden": "true", children: [_jsx("div", { className: "horizon-track", children: cats.length > 0
                    ? cats.map((c) => (_jsx("span", { className: "horizon-seg", style: {
                            background: c.color ?? "#94a3b8",
                            flexGrow: c.seconds,
                        } }, c.id)))
                    : _jsx("span", { className: "horizon-seg horizon-empty" }) }), nowMarker && (_jsx("span", { className: "horizon-marker", style: { left: `${markerPct}%` }, title: "now" })), _jsxs("div", { className: "horizon-scale", "aria-hidden": "true", children: [_jsx("span", { children: "00:00" }), _jsx("span", { children: "12:00" }), _jsx("span", { children: "now" })] })] }));
}
/** A compact category chip legend under the hero. */
export function Legend({ categories }) {
    return (_jsx("ul", { className: "legend", children: categories.map((c) => (_jsxs("li", { className: "legend-item", children: [_jsx("span", { className: "legend-swatch", style: { background: c.color ?? "#94a3b8" } }), _jsx("span", { className: "legend-name", children: c.label })] }, c.id))) }));
}
