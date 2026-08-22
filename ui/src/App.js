import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { targetLabel, todayKey } from "./types";
import { ToastStack, useToasts } from "./toast";
import { Modal } from "./modal";
import { Horizon, Legend } from "./horizon";
import { LimitRowItem, UsageRowItem } from "./rows";
const REFRESH_MS = 3000;
function formatHero(total) {
    if (total >= 3600) {
        const h = Math.floor(total / 3600);
        const m = Math.round((total % 3600) / 60);
        return `${h}<span class="hero-unit">h</span> ${m}<span class="hero-unit">m</span>`;
    }
    if (total >= 60)
        return `${Math.round(total / 60)}<span class="hero-unit">m</span>`;
    return `${total}<span class="hero-unit">s</span>`;
}
export function App() {
    const [status, setStatus] = useState(null);
    const [summary, setSummary] = useState(null);
    const [catalog, setCatalog] = useState(null);
    const { toasts, push } = useToasts();
    const [editingTarget, setEditingTarget] = useState(null);
    const [editMinutes, setEditMinutes] = useState("60");
    const [editEnabled, setEditEnabled] = useState(true);
    const [showPinSetup, setShowPinSetup] = useState(false);
    const [pinNew, setPinNew] = useState("");
    const [pinCurrent, setPinCurrent] = useState("");
    const [pinPromptFor, setPinPromptFor] = useState(null);
    const [pinInput, setPinInput] = useState("");
    const [pinError, setPinError] = useState(null);
    const first = useRef(true);
    useEffect(() => {
        const refresh = () => {
            invoke("get_status")
                .then(setStatus)
                .catch((e) => {
                if (first.current) {
                    push("error", `Agent unreachable: ${String(e)}`);
                    first.current = false;
                }
            });
            invoke("get_day_summary", { day: todayKey() })
                .then(setSummary)
                .catch((e) => {
                if (first.current) {
                    push("error", `Dashboard failed: ${String(e)}`);
                    first.current = false;
                }
            });
            invoke("get_catalog").then(setCatalog).catch(() => { });
        };
        refresh();
        const timer = window.setInterval(refresh, REFRESH_MS);
        return () => window.clearInterval(timer);
    }, [push]);
    const connected = status?.agent_connected ?? false;
    const total = summary?.totalSeconds ?? 0;
    const pinConfigured = status?.pin_configured ?? false;
    const blockedApps = summary?.apps.filter((a) => a.blocked) ?? [];
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
            push("success", "PIN saved.");
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
            push("success", `Limit saved.${when}`);
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
            push("info", `Limit removed.${when}`);
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
            push("success", "+15 minutes granted.");
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
    function limitForTarget(id) {
        return catalog?.limits.find((l) => (l.target.kind === "app" || l.target.kind === "category") && l.target.id === id);
    }
    function limitFor(t) {
        return catalog?.limits.find((l) => {
            if (t.kind === "total")
                return l.target.kind === "total";
            if (t.kind === "app")
                return l.target.kind === "app" && l.target.id === t.id;
            return l.target.kind === "category" && l.target.id === t.id;
        });
    }
    return (_jsxs("main", { className: "app", children: [_jsx(ToastStack, { toasts: toasts }), _jsxs("header", { className: "masthead", children: [_jsxs("div", { className: "brand", children: [_jsx("h1", { children: "Screentime" }), _jsx("p", { className: "subtitle", children: connected && status
                                    ? `${status.tracker_backend} tracking � v${status.version}`
                                    : "Agent not running" })] }), _jsxs("div", { className: "conn", children: [_jsx("span", { className: `live-dot ${connected ? "on" : ""}` }), _jsx("span", { className: "conn-label", children: connected ? "live" : "offline" })] })] }), !pinConfigured && connected && (_jsxs("section", { className: "panel card", children: [_jsxs("div", { className: "card-head", children: [_jsx("h2", { className: "eyebrow", children: "Set a PIN" }), _jsx("button", { className: "ghost", onClick: () => setShowPinSetup((v) => !v), children: showPinSetup ? "Cancel" : "Set PIN" })] }), _jsx("p", { className: "hint", children: "A PIN gates limit changes and overrides. Optional until you set one." }), showPinSetup && (_jsxs("div", { className: "pin-setup", children: [pinConfigured && (_jsx("input", { type: "password", placeholder: "Current PIN", value: pinCurrent, onChange: (e) => setPinCurrent(e.target.value) })), _jsx("input", { type: "password", placeholder: "New PIN", value: pinNew, onChange: (e) => setPinNew(e.target.value) }), _jsx("button", { onClick: handlePinSetup, children: "Save PIN" }), pinError && _jsx("p", { className: "error-text", children: pinError })] }))] })), _jsxs("section", { className: "hero", "aria-label": "Today's screen time", children: [_jsxs("div", { className: "hero-meta", children: [_jsx("span", { className: "eyebrow", children: "Spent today" }), _jsx("span", { className: "hero-clock", children: summary
                                    ? new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
                                    : "" })] }), _jsx("p", { className: "hero-total", dangerouslySetInnerHTML: { __html: formatHero(total) } }), summary ? (_jsx(Horizon, { summary: summary })) : (_jsx("div", { className: "horizon", "aria-hidden": "true", children: _jsx("div", { className: "horizon-track", children: _jsx("span", { className: "horizon-seg horizon-empty" }) }) })), summary && summary.categories.length > 0 && _jsx(Legend, { categories: summary.categories })] }), !connected && _jsx("p", { className: "hint", children: "Waiting for the agent\uFFFD" }), connected && summary && total === 0 && (_jsxs("div", { className: "empty", children: [_jsx("p", { className: "empty-title", children: "A clean slate" }), _jsx("p", { className: "hint", children: "No screen time recorded yet today. Go make some." })] })), blockedApps.length > 0 && (_jsxs("section", { className: "alert-blocked", "aria-label": "Blocked apps", children: [_jsx("div", { className: "alert-blocked-icon", children: "?" }), _jsxs("div", { className: "alert-blocked-body", children: [_jsx("h2", { className: "alert-blocked-title", children: "Blocked right now" }), blockedApps.map((a) => (_jsxs("div", { className: "blocked-row", children: [_jsx("span", { className: "row-label", children: a.label }), _jsx("button", { className: "btn-primary", onClick: () => startOverride({ kind: "app", id: a.id }), children: "+15 min" })] }, a.id)))] })] })), summary && summary.categories.length > 0 && (_jsxs("section", { className: "panel", children: [_jsx("h2", { className: "eyebrow", children: "By category" }), _jsx("ul", { className: "rows", children: summary.categories.map((c) => (_jsx(UsageRowItem, { row: c, total: total, limit: limitForTarget(c.id), onEditLimit: (t) => startSetLimit(t, limitForTarget(c.id)) }, c.id))) })] })), summary && summary.apps.length > 0 && (_jsxs("section", { className: "panel", children: [_jsx("h2", { className: "eyebrow", children: "By app" }), _jsx("ul", { className: "rows", children: summary.apps.map((a) => (_jsx(UsageRowItem, { row: a, total: total, limit: limitForTarget(a.id), onEditLimit: (t) => startSetLimit(t, limitForTarget(a.id)) }, a.id))) })] })), catalog && catalog.categories.length > 0 && (_jsxs("section", { className: "panel", children: [_jsx("h2", { className: "eyebrow", children: "Limits" }), _jsx("p", { className: "hint", children: "Set a daily budget for a category or an app. Loosening takes effect after the cooldown; tightening applies immediately." }), _jsxs("div", { className: "limit-add", children: [_jsxs("select", { className: "limit-select", onChange: (e) => {
                                    const v = e.target.value;
                                    if (v === "total") {
                                        const existing = limitFor({ kind: "total" });
                                        startSetLimit({ kind: "total" }, existing);
                                    }
                                    else {
                                        const id = parseInt(v, 10);
                                        const existing = limitForTarget(id);
                                        startSetLimit({ kind: "category", id }, existing);
                                    }
                                }, value: "", children: [_jsx("option", { value: "", disabled: true, children: "Add a category limit\uFFFD" }), catalog.categories
                                        .filter((c) => c.kind === "limitable")
                                        .map((c) => (_jsx("option", { value: String(c.id), children: c.name }, c.id))), _jsx("option", { value: "total", children: "Total screen time" })] }), _jsxs("select", { className: "limit-select", onChange: (e) => {
                                    const id = parseInt(e.target.value, 10);
                                    if (Number.isNaN(id))
                                        return;
                                    const existing = limitForTarget(id);
                                    startSetLimit({ kind: "app", id }, existing);
                                }, value: "", children: [_jsx("option", { value: "", disabled: true, children: "Add an app limit\uFFFD" }), catalog.apps.map((a) => (_jsx("option", { value: String(a.id), children: a.displayName }, a.id)))] })] }), catalog.limits.length > 0 && (_jsx("ul", { className: "rows", children: catalog.limits.map((l) => (_jsx(LimitRowItem, { limit: l, catalog: catalog, onEdit: (t) => startSetLimit(t, l), onRemove: (t) => startDelete(t) }, l.id))) }))] })), editingTarget && (_jsxs(Modal, { title: "Edit limit", eyebrow: targetLabel(editingTarget, catalog), onClose: () => setEditingTarget(null), children: [_jsxs("label", { children: ["Minutes per day", _jsx("input", { type: "number", min: 0, value: editMinutes, onChange: (e) => setEditMinutes(e.target.value) })] }), _jsxs("label", { className: "checkbox", children: [_jsx("input", { type: "checkbox", checked: editEnabled, onChange: (e) => setEditEnabled(e.target.checked) }), "Enabled"] }), _jsxs("div", { className: "modal-actions", children: [_jsx("button", { className: "btn-primary", onClick: confirmEdit, children: "Save" }), _jsx("button", { className: "ghost", onClick: () => setEditingTarget(null), children: "Cancel" })] })] })), pinPromptFor && (_jsxs(Modal, { title: "Enter PIN", eyebrow: pinPromptFor.kind === "set_limit" ? "Change limit" : pinPromptFor.kind === "delete_limit" ? "Remove limit" : "Grant time", onClose: () => setPinPromptFor(null), children: [_jsx("p", { className: "hint", children: pinPromptFor.kind === "set_limit"
                            ? "PIN required to change this limit."
                            : pinPromptFor.kind === "delete_limit"
                                ? "PIN required to remove this limit."
                                : "PIN required to grant more time." }), _jsx("input", { type: "password", placeholder: "PIN", value: pinInput, autoFocus: true, onChange: (e) => {
                            setPinInput(e.target.value);
                            setPinError(null);
                        }, onKeyDown: (e) => {
                            if (e.key === "Enter")
                                confirmPin();
                        } }), pinError && _jsx("p", { className: "error-text", children: pinError }), _jsxs("div", { className: "modal-actions", children: [_jsx("button", { className: "btn-primary", onClick: confirmPin, children: "Confirm" }), _jsx("button", { className: "ghost", onClick: () => setPinPromptFor(null), children: "Cancel" })] })] }))] }));
}
