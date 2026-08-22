import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
// Refresh the dashboard this often. The agent writes usage in the background;
// without polling the window freezes at whatever it first fetched.
const REFRESH_MS = 3000;
const TOAST_MS = 4500;
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
function targetLabel(t, catalog) {
    switch (t.kind) {
        case "total":
            return "Total screen time";
        case "app":
            return catalog?.apps.find((a) => a.id === t.id)?.displayName ?? `App #${t.id}`;
        case "category":
            return catalog?.categories.find((c) => c.id === t.id)?.name ?? `Category #${t.id}`;
    }
}
let toastSeq = 0;
export function App() {
    const [status, setStatus] = useState(null);
    const [summary, setSummary] = useState(null);
    const [catalog, setCatalog] = useState(null);
    const [toasts, setToasts] = useState([]);
    // Limit editor state.
    const [editingTarget, setEditingTarget] = useState(null);
    const [editMinutes, setEditMinutes] = useState("60");
    const [editEnabled, setEditEnabled] = useState(true);
    // PIN.
    const [showPinSetup, setShowPinSetup] = useState(false);
    const [pinNew, setPinNew] = useState("");
    const [pinCurrent, setPinCurrent] = useState("");
    const [pinPromptFor, setPinPromptFor] = useState(null);
    const [pinInput, setPinInput] = useState("");
    const [pinError, setPinError] = useState(null);
    const pushToast = useCallback((kind, message) => {
        const id = ++toastSeq;
        setToasts((t) => [...t, { id, kind, message }]);
        window.setTimeout(() => {
            setToasts((t) => t.filter((x) => x.id !== id));
        }, TOAST_MS);
    }, []);
    const first = useRef(true);
    useEffect(() => {
        const refresh = () => {
            invoke("get_status")
                .then(setStatus)
                .catch((e) => {
                if (first.current) {
                    pushToast("error", `Agent unreachable: ${String(e)}`);
                    first.current = false;
                }
            });
            invoke("get_day_summary", { day: todayKey() })
                .then(setSummary)
                .catch((e) => {
                if (first.current) {
                    pushToast("error", `Dashboard failed: ${String(e)}`);
                    first.current = false;
                }
            });
            invoke("get_catalog")
                .then(setCatalog)
                .catch(() => { });
        };
        refresh();
        const timer = setInterval(refresh, REFRESH_MS);
        return () => clearInterval(timer);
    }, [pushToast]);
    const connected = status?.agent_connected ?? false;
    const total = summary?.totalSeconds ?? 0;
    const pinConfigured = status?.pin_configured ?? false;
    const blockedApps = summary?.apps.filter((a) => a.blocked) ?? [];
    // --- Actions ---
    async function handlePinSetup() {
        if (!pinNew) {
            setPinError("Enter a PIN.");
            return;
        }
        try {
            await invoke("set_pin", {
                newPin: pinNew,
                currentPin: pinConfigured ? pinCurrent : null,
            });
            setPinNew("");
            setPinCurrent("");
            setShowPinSetup(false);
            pushToast("success", "PIN saved.");
            setStatus((s) => (s ? { ...s, pin_configured: true } : s));
        }
        catch (e) {
            setPinError(String(e));
        }
    }
    function startSetLimit(target, existing) {
        setEditingTarget(target);
        setEditMinutes(existing ? String(existing.defaultMinutes) : "60");
        setEditEnabled(existing ? existing.enabled : true);
    }
    function confirmEdit() {
        if (!editingTarget)
            return;
        const minutes = Math.max(0, parseInt(editMinutes, 10) || 0);
        if (pinConfigured) {
            setPinPromptFor({ kind: "set_limit", target: editingTarget, minutes, enabled: editEnabled });
            setPinInput("");
        }
        else {
            doSetLimit(editingTarget, minutes, editEnabled, "");
        }
    }
    async function doSetLimit(target, minutes, enabled, pin) {
        try {
            const effective = await invoke("set_limit", {
                target,
                defaultMinutes: minutes,
                enabled,
                pin,
            });
            const when = effective ? ` Takes effect ${new Date(effective).toLocaleString()}.` : "";
            pushToast("success", `Limit saved.${when}`);
            setEditingTarget(null);
            setPinPromptFor(null);
        }
        catch (e) {
            setPinError(String(e));
        }
    }
    function startDelete(target) {
        if (pinConfigured) {
            setPinPromptFor({ kind: "delete_limit", target });
            setPinInput("");
        }
        else {
            doDelete(target, "");
        }
    }
    async function doDelete(target, pin) {
        try {
            const effective = await invoke("delete_limit", { target, pin });
            const when = effective ? ` Takes effect ${new Date(effective).toLocaleString()}.` : "";
            pushToast("info", `Limit removed.${when}`);
            setPinPromptFor(null);
        }
        catch (e) {
            setPinError(String(e));
        }
    }
    function startOverride(target) {
        if (pinConfigured) {
            setPinPromptFor({ kind: "override", target });
            setPinInput("");
        }
        else {
            doOverride(target, "");
        }
    }
    async function doOverride(target, pin) {
        try {
            await invoke("grant_override", { target, seconds: 15 * 60, pin });
            pushToast("success", "+15 minutes granted.");
            setPinPromptFor(null);
        }
        catch (e) {
            setPinError(String(e));
        }
    }
    function confirmPin() {
        if (!pinPromptFor)
            return;
        const pin = pinInput;
        switch (pinPromptFor.kind) {
            case "set_limit":
                if (pinPromptFor.target)
                    doSetLimit(pinPromptFor.target, pinPromptFor.minutes ?? 60, pinPromptFor.enabled ?? true, pin);
                break;
            case "delete_limit":
                if (pinPromptFor.target)
                    doDelete(pinPromptFor.target, pin);
                break;
            case "override":
                if (pinPromptFor.target)
                    doOverride(pinPromptFor.target, pin);
                break;
        }
    }
    return (_jsxs("main", { className: "app", children: [_jsx("div", { className: "toasts", "aria-live": "polite", children: toasts.map((t) => (_jsx("div", { className: `toast toast-${t.kind}`, children: t.message }, t.id))) }), _jsxs("header", { className: "masthead", children: [_jsxs("div", { children: [_jsx("h1", { children: "Screentime" }), _jsx("p", { className: "subtitle", children: connected && status
                                    ? `${status.tracker_backend} tracking · v${status.version}`
                                    : "Agent not running — start screentime-agent" })] }), _jsx("span", { className: `live-dot ${connected ? "on" : ""}`, title: connected ? "connected" : "disconnected" })] }), !pinConfigured && connected && (_jsxs("section", { className: "panel card", children: [_jsxs("div", { className: "card-head", children: [_jsx("h2", { className: "eyebrow", children: "Set a PIN" }), _jsx("button", { className: "ghost", onClick: () => setShowPinSetup((v) => !v), children: showPinSetup ? "Cancel" : "Set PIN" })] }), _jsx("p", { className: "hint", children: "A PIN gates limit changes and overrides. Optional until you set one." }), showPinSetup && (_jsxs("div", { className: "pin-setup", children: [pinConfigured && (_jsx("input", { type: "password", placeholder: "Current PIN", value: pinCurrent, onChange: (e) => setPinCurrent(e.target.value) })), _jsx("input", { type: "password", placeholder: "New PIN", value: pinNew, onChange: (e) => setPinNew(e.target.value) }), _jsx("button", { onClick: handlePinSetup, children: "Save PIN" }), pinError && _jsx("p", { className: "error-text", children: pinError })] }))] })), _jsxs("section", { className: "hero", "aria-label": "Today's screen time", children: [_jsxs("div", { className: "hero-meta", children: [_jsx("span", { className: "eyebrow", children: "Today" }), _jsx("span", { className: "hero-clock", children: summary ? new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "" })] }), _jsx("p", { className: "hero-total", children: formatDuration(total) }), _jsx("div", { className: "horizon", "aria-hidden": "true", children: _jsx("div", { className: "horizon-track", children: summary?.categories.length ? (summary.categories.map((c) => (_jsx("span", { className: "horizon-seg", style: {
                                    background: c.color ?? "#94a3b8",
                                    flexGrow: c.seconds,
                                } }, c.id)))) : (_jsx("span", { className: "horizon-seg horizon-empty" })) }) })] }), !connected && _jsx("p", { className: "hint", children: "Waiting for the agent\u2026" }), blockedApps.length > 0 && (_jsxs("section", { className: "panel card card-blocked", "aria-label": "Blocked apps", children: [_jsx("div", { className: "card-head", children: _jsx("h2", { className: "eyebrow", children: "Blocked right now" }) }), blockedApps.map((a) => (_jsxs("div", { className: "blocked-row", children: [_jsx("span", { className: "row-label", children: a.label }), _jsx("button", { onClick: () => startOverride({ kind: "app", id: a.id }), children: "+15 min" })] }, a.id)))] })), summary && summary.categories.length > 0 && (_jsxs("section", { className: "panel", children: [_jsx("h2", { className: "eyebrow", children: "By category" }), _jsx("ul", { className: "rows", children: summary.categories.map((c) => {
                            const limit = catalog?.limits.find((l) => l.target.kind === "category" && l.target.id === c.id);
                            return (_jsxs("li", { className: "row", children: [_jsx("span", { className: "dot", style: { background: c.color ?? "#94a3b8" } }), _jsx("span", { className: "row-label", children: c.label }), _jsx("span", { className: "row-bar", children: _jsx("span", { className: "row-bar-fill", style: {
                                                background: c.color ?? "#94a3b8",
                                                width: `${percent(c.seconds, total)}%`,
                                            } }) }), _jsx("span", { className: "row-time", children: formatDuration(c.seconds) }), _jsxs("span", { className: "row-pct", children: [percent(c.seconds, total), "%"] }), limit && (_jsxs("button", { className: "ghost badge-btn", onClick: () => startSetLimit({ kind: "category", id: c.id }, limit), children: [formatDuration(limit.defaultMinutes * 60), "/day"] }))] }, c.id));
                        }) })] })), summary && summary.apps.length > 0 && (_jsxs("section", { className: "panel", children: [_jsx("h2", { className: "eyebrow", children: "By app" }), _jsx("ul", { className: "rows", children: summary.apps.map((a) => {
                            const limit = catalog?.limits.find((l) => l.target.kind === "app" && l.target.id === a.id);
                            return (_jsxs("li", { className: "row", children: [_jsx("span", { className: "dot", style: { background: a.color ?? "#94a3b8" } }), _jsxs("span", { className: "row-label", children: [a.label, a.blocked && _jsx("span", { className: "blocked-tag", children: "blocked" })] }), _jsx("span", { className: "row-bar", children: _jsx("span", { className: "row-bar-fill", style: {
                                                background: a.color ?? "#94a3b8",
                                                width: `${percent(a.seconds, total)}%`,
                                            } }) }), _jsx("span", { className: "row-time", children: formatDuration(a.seconds) }), _jsxs("span", { className: "row-pct", children: [percent(a.seconds, total), "%"] }), limit && (_jsxs("button", { className: "ghost badge-btn", onClick: () => startSetLimit({ kind: "app", id: a.id }, limit), children: [formatDuration(limit.defaultMinutes * 60), "/day"] }))] }, a.id));
                        }) })] })), catalog && catalog.categories.length > 0 && (_jsxs("section", { className: "panel", children: [_jsx("h2", { className: "eyebrow", children: "Limits" }), _jsx("p", { className: "hint", children: "Set a daily budget for a category or an app. Loosening takes effect after the cooldown; tightening applies immediately." }), _jsxs("div", { className: "limit-editor", children: [_jsxs("select", { onChange: (e) => {
                                    const v = e.target.value;
                                    if (v === "total") {
                                        const existing = catalog.limits.find((l) => l.target.kind === "total");
                                        startSetLimit({ kind: "total" }, existing);
                                    }
                                    else {
                                        const id = parseInt(v, 10);
                                        const existing = catalog.limits.find((l) => l.target.kind === "category" && l.target.id === id);
                                        startSetLimit({ kind: "category", id }, existing);
                                    }
                                }, value: "", children: [_jsx("option", { value: "", disabled: true, children: "Add a category limit\u2026" }), catalog.categories
                                        .filter((c) => c.kind === "limitable")
                                        .map((c) => (_jsx("option", { value: String(c.id), children: c.name }, c.id))), _jsx("option", { value: "total", children: "Total screen time" })] }), _jsxs("select", { onChange: (e) => {
                                    const id = parseInt(e.target.value, 10);
                                    if (Number.isNaN(id))
                                        return;
                                    const existing = catalog.limits.find((l) => l.target.kind === "app" && l.target.id === id);
                                    startSetLimit({ kind: "app", id }, existing);
                                }, value: "", children: [_jsx("option", { value: "", disabled: true, children: "Add an app limit\u2026" }), catalog.apps.map((a) => (_jsx("option", { value: String(a.id), children: a.displayName }, a.id)))] })] }), catalog.limits.length > 0 && (_jsx("ul", { className: "rows", children: catalog.limits.map((l) => (_jsxs("li", { className: "row", children: [_jsx("span", { className: "row-label", children: targetLabel(l.target, catalog) }), _jsxs("span", { className: "row-time", children: [formatDuration(l.defaultMinutes * 60), "/day"] }), _jsx("span", { className: "row-pct", children: l.enabled ? "on" : "off" }), _jsx("button", { className: "ghost", onClick: () => startSetLimit(l.target, l), children: "edit" }), _jsx("button", { className: "ghost", onClick: () => startDelete(l.target), children: "remove" })] }, l.id))) }))] })), connected && summary && total === 0 && (_jsx("p", { className: "hint", children: "No screen time recorded yet today." })), editingTarget && (_jsx("div", { className: "modal-backdrop", onClick: () => setEditingTarget(null), children: _jsxs("div", { className: "modal", onClick: (e) => e.stopPropagation(), children: [_jsx("h3", { children: "Edit limit" }), _jsx("p", { className: "hint", children: targetLabel(editingTarget, catalog) }), _jsxs("label", { children: ["Minutes per day", _jsx("input", { type: "number", min: 0, value: editMinutes, onChange: (e) => setEditMinutes(e.target.value) })] }), _jsxs("label", { className: "checkbox", children: [_jsx("input", { type: "checkbox", checked: editEnabled, onChange: (e) => setEditEnabled(e.target.checked) }), "Enabled"] }), _jsxs("div", { className: "modal-actions", children: [_jsx("button", { onClick: confirmEdit, children: "Save" }), _jsx("button", { className: "ghost", onClick: () => setEditingTarget(null), children: "Cancel" })] })] }) })), pinPromptFor && (_jsx("div", { className: "modal-backdrop", children: _jsxs("div", { className: "modal", children: [_jsx("h3", { children: "Enter PIN" }), _jsx("p", { className: "hint", children: pinPromptFor.kind === "set_limit"
                                ? "PIN required to change this limit."
                                : pinPromptFor.kind === "delete_limit"
                                    ? "PIN required to remove this limit."
                                    : "PIN required to grant more time." }), _jsx("input", { type: "password", placeholder: "PIN", value: pinInput, autoFocus: true, onChange: (e) => {
                                setPinInput(e.target.value);
                                setPinError(null);
                            }, onKeyDown: (e) => {
                                if (e.key === "Enter")
                                    confirmPin();
                            } }), pinError && _jsx("p", { className: "error-text", children: pinError }), _jsxs("div", { className: "modal-actions", children: [_jsx("button", { onClick: confirmPin, children: "Confirm" }), _jsx("button", { className: "ghost", onClick: () => setPinPromptFor(null), children: "Cancel" })] })] }) }))] }));
}
