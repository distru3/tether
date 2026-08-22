import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Refresh the dashboard this often. The agent writes usage in the background;
// without polling the window freezes at whatever it first fetched.
const REFRESH_MS = 3000;
const TOAST_MS = 4500;

// Shapes mirror the Rust types in ui/src-tauri/src/lib.rs; hand-kept in sync
// until we generate these from the Rust types.
interface AgentStatus {
    version: string;
    agent_connected: boolean;
    tracker_backend: string;
    filter_backend: string;
    tracking_available: boolean;
    pin_configured: boolean;
    strict_mode: boolean;
}

interface UsageRow {
    id: number;
    label: string;
    seconds: number;
    color: string | null;
    limitSeconds: number | null;
    blocked: boolean;
}

interface DaySummary {
    day: number;
    totalSeconds: number;
    apps: UsageRow[];
    categories: UsageRow[];
}

type LimitTarget =
    | { kind: "app"; id: number }
    | { kind: "category"; id: number }
    | { kind: "total" };

interface AppInfo {
    id: number;
    key: string;
    displayName: string;
    primaryCategory: number;
    tags: number[];
    userClassified: boolean;
}

interface CategoryInfo {
    id: number;
    slug: string;
    name: string;
    kind: string;
    color: string;
    builtin: boolean;
}

interface LimitInfo {
    id: number;
    target: LimitTarget;
    defaultMinutes: number;
    weekdayMinutes: (number | null)[];
    enabled: boolean;
}

interface Catalog {
    apps: AppInfo[];
    categories: CategoryInfo[];
    limits: LimitInfo[];
}

type ToastKind = "success" | "error" | "info";
interface Toast {
    id: number;
    kind: ToastKind;
    message: string;
}

function todayKey(): number {
    const now = new Date();
    return now.getFullYear() * 10000 + (now.getMonth() + 1) * 100 + now.getDate();
}

function formatDuration(total: number): string {
    if (total >= 3600) {
        const h = Math.floor(total / 3600);
        const m = Math.round((total % 3600) / 60);
        return `${h}h ${m}m`;
    }
    if (total >= 60) return `${Math.round(total / 60)}m`;
    return `${total}s`;
}

function percent(seconds: number, total: number): string {
    if (total <= 0) return "0";
    return Math.round((seconds / total) * 100).toString();
}

function targetLabel(t: LimitTarget, catalog: Catalog | null): string {
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
    const [status, setStatus] = useState<AgentStatus | null>(null);
    const [summary, setSummary] = useState<DaySummary | null>(null);
    const [catalog, setCatalog] = useState<Catalog | null>(null);
    const [toasts, setToasts] = useState<Toast[]>([]);

    // Limit editor state.
    const [editingTarget, setEditingTarget] = useState<LimitTarget | null>(null);
    const [editMinutes, setEditMinutes] = useState("60");
    const [editEnabled, setEditEnabled] = useState(true);

    // PIN.
    const [showPinSetup, setShowPinSetup] = useState(false);
    const [pinNew, setPinNew] = useState("");
    const [pinCurrent, setPinCurrent] = useState("");
    const [pinPromptFor, setPinPromptFor] = useState<{
        kind: "set_limit" | "delete_limit" | "override";
        target?: LimitTarget;
        minutes?: number;
        enabled?: boolean;
    } | null>(null);
    const [pinInput, setPinInput] = useState("");
    const [pinError, setPinError] = useState<string | null>(null);

    const pushToast = useCallback((kind: ToastKind, message: string) => {
        const id = ++toastSeq;
        setToasts((t) => [...t, { id, kind, message }]);
        window.setTimeout(() => {
            setToasts((t) => t.filter((x) => x.id !== id));
        }, TOAST_MS);
    }, []);

    const first = useRef(true);

    useEffect(() => {
        const refresh = () => {
            invoke<AgentStatus>("get_status")
                .then(setStatus)
                .catch((e: unknown) => {
                    if (first.current) {
                        pushToast("error", `Agent unreachable: ${String(e)}`);
                        first.current = false;
                    }
                });

            invoke<DaySummary>("get_day_summary", { day: todayKey() })
                .then(setSummary)
                .catch((e: unknown) => {
                    if (first.current) {
                        pushToast("error", `Dashboard failed: ${String(e)}`);
                        first.current = false;
                    }
                });

            invoke<Catalog>("get_catalog")
                .then(setCatalog)
                .catch(() => {});
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
            await invoke<string>("set_pin", {
                newPin: pinNew,
                currentPin: pinConfigured ? pinCurrent : null,
            });
            setPinNew("");
            setPinCurrent("");
            setShowPinSetup(false);
            pushToast("success", "PIN saved.");
            setStatus((s) => (s ? { ...s, pin_configured: true } : s));
        } catch (e) {
            setPinError(String(e));
        }
    }

    function startSetLimit(target: LimitTarget, existing?: LimitInfo) {
        setEditingTarget(target);
        setEditMinutes(existing ? String(existing.defaultMinutes) : "60");
        setEditEnabled(existing ? existing.enabled : true);
    }

    function confirmEdit() {
        if (!editingTarget) return;
        const minutes = Math.max(0, parseInt(editMinutes, 10) || 0);
        if (pinConfigured) {
            setPinPromptFor({ kind: "set_limit", target: editingTarget, minutes, enabled: editEnabled });
            setPinInput("");
        } else {
            doSetLimit(editingTarget, minutes, editEnabled, "");
        }
    }

    async function doSetLimit(target: LimitTarget, minutes: number, enabled: boolean, pin: string) {
        try {
            const effective = await invoke<string>("set_limit", {
                target,
                defaultMinutes: minutes,
                enabled,
                pin,
            });
            const when = effective ? ` Takes effect ${new Date(effective).toLocaleString()}.` : "";
            pushToast("success", `Limit saved.${when}`);
            setEditingTarget(null);
            setPinPromptFor(null);
        } catch (e) {
            setPinError(String(e));
        }
    }

    function startDelete(target: LimitTarget) {
        if (pinConfigured) {
            setPinPromptFor({ kind: "delete_limit", target });
            setPinInput("");
        } else {
            doDelete(target, "");
        }
    }

    async function doDelete(target: LimitTarget, pin: string) {
        try {
            const effective = await invoke<string>("delete_limit", { target, pin });
            const when = effective ? ` Takes effect ${new Date(effective).toLocaleString()}.` : "";
            pushToast("info", `Limit removed.${when}`);
            setPinPromptFor(null);
        } catch (e) {
            setPinError(String(e));
        }
    }

    function startOverride(target: LimitTarget) {
        if (pinConfigured) {
            setPinPromptFor({ kind: "override", target });
            setPinInput("");
        } else {
            doOverride(target, "");
        }
    }

    async function doOverride(target: LimitTarget, pin: string) {
        try {
            await invoke("grant_override", { target, seconds: 15 * 60, pin });
            pushToast("success", "+15 minutes granted.");
            setPinPromptFor(null);
        } catch (e) {
            setPinError(String(e));
        }
    }

    function confirmPin() {
        if (!pinPromptFor) return;
        const pin = pinInput;
        switch (pinPromptFor.kind) {
            case "set_limit":
                if (pinPromptFor.target)
                    doSetLimit(
                        pinPromptFor.target,
                        pinPromptFor.minutes ?? 60,
                        pinPromptFor.enabled ?? true,
                        pin,
                    );
                break;
            case "delete_limit":
                if (pinPromptFor.target) doDelete(pinPromptFor.target, pin);
                break;
            case "override":
                if (pinPromptFor.target) doOverride(pinPromptFor.target, pin);
                break;
        }
    }

    return (
        <main className="app">
            {/* Toasts */}
            <div className="toasts" aria-live="polite">
                {toasts.map((t) => (
                    <div key={t.id} className={`toast toast-${t.kind}`}>
                        {t.message}
                    </div>
                ))}
            </div>

            <header className="masthead">
                <div>
                    <h1>Screentime</h1>
                    <p className="subtitle">
                        {connected && status
                            ? `${status.tracker_backend} tracking · v${status.version}`
                            : "Agent not running — start screentime-agent"}
                    </p>
                </div>
                <span className={`live-dot ${connected ? "on" : ""}`} title={connected ? "connected" : "disconnected"} />
            </header>

            {!pinConfigured && connected && (
                <section className="panel card">
                    <div className="card-head">
                        <h2 className="eyebrow">Set a PIN</h2>
                        <button className="ghost" onClick={() => setShowPinSetup((v) => !v)}>
                            {showPinSetup ? "Cancel" : "Set PIN"}
                        </button>
                    </div>
                    <p className="hint">
                        A PIN gates limit changes and overrides. Optional until you set one.
                    </p>
                    {showPinSetup && (
                        <div className="pin-setup">
                            {pinConfigured && (
                                <input
                                    type="password"
                                    placeholder="Current PIN"
                                    value={pinCurrent}
                                    onChange={(e) => setPinCurrent(e.target.value)}
                                />
                            )}
                            <input
                                type="password"
                                placeholder="New PIN"
                                value={pinNew}
                                onChange={(e) => setPinNew(e.target.value)}
                            />
                            <button onClick={handlePinSetup}>Save PIN</button>
                            {pinError && <p className="error-text">{pinError}</p>}
                        </div>
                    )}
                </section>
            )}

            {/* The day as one horizon line. */}
            <section className="hero" aria-label="Today's screen time">
                <div className="hero-meta">
                    <span className="eyebrow">Today</span>
                    <span className="hero-clock">
                        {summary ? new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : ""}
                    </span>
                </div>
                <p className="hero-total">{formatDuration(total)}</p>
                <div className="horizon" aria-hidden="true">
                    <div className="horizon-track">
                        {summary?.categories.length ? (
                            summary.categories.map((c) => (
                                <span
                                    key={c.id}
                                    className="horizon-seg"
                                    style={{
                                        background: c.color ?? "#94a3b8",
                                        flexGrow: c.seconds,
                                    }}
                                />
                            ))
                        ) : (
                            <span className="horizon-seg horizon-empty" />
                        )}
                    </div>
                </div>
            </section>

            {!connected && <p className="hint">Waiting for the agent…</p>}

            {blockedApps.length > 0 && (
                <section className="panel card card-blocked" aria-label="Blocked apps">
                    <div className="card-head">
                        <h2 className="eyebrow">Blocked right now</h2>
                    </div>
                    {blockedApps.map((a) => (
                        <div key={a.id} className="blocked-row">
                            <span className="row-label">{a.label}</span>
                            <button onClick={() => startOverride({ kind: "app", id: a.id })}>
                                +15 min
                            </button>
                        </div>
                    ))}
                </section>
            )}

            {summary && summary.categories.length > 0 && (
                <section className="panel">
                    <h2 className="eyebrow">By category</h2>
                    <ul className="rows">
                        {summary.categories.map((c) => {
                            const limit = catalog?.limits.find(
                                (l) => l.target.kind === "category" && l.target.id === c.id,
                            );
                            return (
                                <li key={c.id} className="row">
                                    <span className="dot" style={{ background: c.color ?? "#94a3b8" }} />
                                    <span className="row-label">{c.label}</span>
                                    <span className="row-bar">
                                        <span
                                            className="row-bar-fill"
                                            style={{
                                                background: c.color ?? "#94a3b8",
                                                width: `${percent(c.seconds, total)}%`,
                                            }}
                                        />
                                    </span>
                                    <span className="row-time">{formatDuration(c.seconds)}</span>
                                    <span className="row-pct">{percent(c.seconds, total)}%</span>
                                    {limit && (
                                        <button
                                            className="ghost badge-btn"
                                            onClick={() => startSetLimit({ kind: "category", id: c.id }, limit)}
                                        >
                                            {formatDuration(limit.defaultMinutes * 60)}/day
                                        </button>
                                    )}
                                </li>
                            );
                        })}
                    </ul>
                </section>
            )}

            {summary && summary.apps.length > 0 && (
                <section className="panel">
                    <h2 className="eyebrow">By app</h2>
                    <ul className="rows">
                        {summary.apps.map((a) => {
                            const limit = catalog?.limits.find(
                                (l) => l.target.kind === "app" && l.target.id === a.id,
                            );
                            return (
                                <li key={a.id} className="row">
                                    <span className="dot" style={{ background: a.color ?? "#94a3b8" }} />
                                    <span className="row-label">
                                        {a.label}
                                        {a.blocked && <span className="blocked-tag">blocked</span>}
                                    </span>
                                    <span className="row-bar">
                                        <span
                                            className="row-bar-fill"
                                            style={{
                                                background: a.color ?? "#94a3b8",
                                                width: `${percent(a.seconds, total)}%`,
                                            }}
                                        />
                                    </span>
                                    <span className="row-time">{formatDuration(a.seconds)}</span>
                                    <span className="row-pct">{percent(a.seconds, total)}%</span>
                                    {limit && (
                                        <button
                                            className="ghost badge-btn"
                                            onClick={() => startSetLimit({ kind: "app", id: a.id }, limit)}
                                        >
                                            {formatDuration(limit.defaultMinutes * 60)}/day
                                        </button>
                                    )}
                                </li>
                            );
                        })}
                    </ul>
                </section>
            )}

            {catalog && catalog.categories.length > 0 && (
                <section className="panel">
                    <h2 className="eyebrow">Limits</h2>
                    <p className="hint">
                        Set a daily budget for a category or an app. Loosening takes effect after the
                        cooldown; tightening applies immediately.
                    </p>
                    <div className="limit-editor">
                        <select
                            onChange={(e) => {
                                const v = e.target.value;
                                if (v === "total") {
                                    const existing = catalog.limits.find((l) => l.target.kind === "total");
                                    startSetLimit({ kind: "total" }, existing);
                                } else {
                                    const id = parseInt(v, 10);
                                    const existing = catalog.limits.find(
                                        (l) => l.target.kind === "category" && l.target.id === id,
                                    );
                                    startSetLimit({ kind: "category", id }, existing);
                                }
                            }}
                            value=""
                        >
                            <option value="" disabled>
                                Add a category limit…
                            </option>
                            {catalog.categories
                                .filter((c) => c.kind === "limitable")
                                .map((c) => (
                                    <option key={c.id} value={String(c.id)}>
                                        {c.name}
                                    </option>
                                ))}
                            <option value="total">Total screen time</option>
                        </select>
                        <select
                            onChange={(e) => {
                                const id = parseInt(e.target.value, 10);
                                if (Number.isNaN(id)) return;
                                const existing = catalog.limits.find(
                                    (l) => l.target.kind === "app" && l.target.id === id,
                                );
                                startSetLimit({ kind: "app", id }, existing);
                            }}
                            value=""
                        >
                            <option value="" disabled>
                                Add an app limit…
                            </option>
                            {catalog.apps.map((a) => (
                                <option key={a.id} value={String(a.id)}>
                                    {a.displayName}
                                </option>
                            ))}
                        </select>
                    </div>

                    {catalog.limits.length > 0 && (
                        <ul className="rows">
                            {catalog.limits.map((l) => (
                                <li key={l.id} className="row">
                                    <span className="row-label">{targetLabel(l.target, catalog)}</span>
                                    <span className="row-time">
                                        {formatDuration(l.defaultMinutes * 60)}/day
                                    </span>
                                    <span className="row-pct">{l.enabled ? "on" : "off"}</span>
                                    <button className="ghost" onClick={() => startSetLimit(l.target, l)}>
                                        edit
                                    </button>
                                    <button className="ghost" onClick={() => startDelete(l.target)}>
                                        remove
                                    </button>
                                </li>
                            ))}
                        </ul>
                    )}
                </section>
            )}

            {connected && summary && total === 0 && (
                <p className="hint">No screen time recorded yet today.</p>
            )}

            {editingTarget && (
                <div className="modal-backdrop" onClick={() => setEditingTarget(null)}>
                    <div className="modal" onClick={(e) => e.stopPropagation()}>
                        <h3>Edit limit</h3>
                        <p className="hint">{targetLabel(editingTarget, catalog)}</p>
                        <label>
                            Minutes per day
                            <input
                                type="number"
                                min={0}
                                value={editMinutes}
                                onChange={(e) => setEditMinutes(e.target.value)}
                            />
                        </label>
                        <label className="checkbox">
                            <input
                                type="checkbox"
                                checked={editEnabled}
                                onChange={(e) => setEditEnabled(e.target.checked)}
                            />
                            Enabled
                        </label>
                        <div className="modal-actions">
                            <button onClick={confirmEdit}>Save</button>
                            <button className="ghost" onClick={() => setEditingTarget(null)}>
                                Cancel
                            </button>
                        </div>
                    </div>
                </div>
            )}

            {pinPromptFor && (
                <div className="modal-backdrop">
                    <div className="modal">
                        <h3>Enter PIN</h3>
                        <p className="hint">
                            {pinPromptFor.kind === "set_limit"
                                ? "PIN required to change this limit."
                                : pinPromptFor.kind === "delete_limit"
                                  ? "PIN required to remove this limit."
                                  : "PIN required to grant more time."}
                        </p>
                        <input
                            type="password"
                            placeholder="PIN"
                            value={pinInput}
                            autoFocus
                            onChange={(e) => {
                                setPinInput(e.target.value);
                                setPinError(null);
                            }}
                            onKeyDown={(e) => {
                                if (e.key === "Enter") confirmPin();
                            }}
                        />
                        {pinError && <p className="error-text">{pinError}</p>}
                        <div className="modal-actions">
                            <button onClick={confirmPin}>Confirm</button>
                            <button className="ghost" onClick={() => setPinPromptFor(null)}>
                                Cancel
                            </button>
                        </div>
                    </div>
                </div>
            )}
        </main>
    );
}