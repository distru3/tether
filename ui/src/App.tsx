import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import type {
    AgentStatus,
    Catalog,
    DaySummary,
    LimitInfo,
    LimitTarget,
} from "./types";
import { targetLabel, todayKey } from "./types";
import { ToastStack, useToasts } from "./toast";
import { Modal } from "./modal";
import { Horizon, Legend } from "./horizon";
import { LimitRowItem, UsageRowItem } from "./rows";

const REFRESH_MS = 3000;

function formatHero(total: number): string {
    if (total >= 3600) {
        const h = Math.floor(total / 3600);
        const m = Math.round((total % 3600) / 60);
        return `${h}<span class="hero-unit">h</span> ${m}<span class="hero-unit">m</span>`;
    }
    if (total >= 60) return `${Math.round(total / 60)}<span class="hero-unit">m</span>`;
    return `${total}<span class="hero-unit">s</span>`;
}

export function App() {
    const [status, setStatus] = useState<AgentStatus | null>(null);
    const [summary, setSummary] = useState<DaySummary | null>(null);
    const [catalog, setCatalog] = useState<Catalog | null>(null);
    const { toasts, push } = useToasts();

    const [editingTarget, setEditingTarget] = useState<LimitTarget | null>(null);
    const [editMinutes, setEditMinutes] = useState("60");
    const [editEnabled, setEditEnabled] = useState(true);

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

    const first = useRef(true);

    useEffect(() => {
        const refresh = () => {
            invoke<AgentStatus>("get_status")
                .then(setStatus)
                .catch((e: unknown) => {
                    if (first.current) {
                        push("error", `Agent unreachable: ${String(e)}`);
                        first.current = false;
                    }
                });
            invoke<DaySummary>("get_day_summary", { day: todayKey() })
                .then(setSummary)
                .catch((e: unknown) => {
                    if (first.current) {
                        push("error", `Dashboard failed: ${String(e)}`);
                        first.current = false;
                    }
                });
            invoke<Catalog>("get_catalog").then(setCatalog).catch(() => {});
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
            await invoke<string>("set_pin", {
                newPin: pinNew,
                currentPin: pinConfigured ? pinCurrent : null,
            });
            setPinNew("");
            setPinCurrent("");
            setShowPinSetup(false);
            push("success", "PIN saved.");
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
            push("success", `Limit saved.${when}`);
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
            push("info", `Limit removed.${when}`);
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
            push("success", "+15 minutes granted.");
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


    function limitForTarget(id: number): LimitInfo | undefined {
        return catalog?.limits.find(
            (l) => (l.target.kind === "app" || l.target.kind === "category") && l.target.id === id,
        );
    }
    function limitFor(t: LimitTarget): LimitInfo | undefined {
        return catalog?.limits.find((l) => {
            if (t.kind === "total") return l.target.kind === "total";
            if (t.kind === "app") return l.target.kind === "app" && l.target.id === t.id;
            return l.target.kind === "category" && l.target.id === t.id;
        });
    }
    return (
        <main className="app">
            <ToastStack toasts={toasts} />

            <header className="masthead">
                <div className="brand">
                    <h1>Screentime</h1>
                    <p className="subtitle">
                        {connected && status
                            ? `${status.tracker_backend} tracking � v${status.version}`
                            : "Agent not running"}
                    </p>
                </div>
                <div className="conn">
                    <span className={`live-dot ${connected ? "on" : ""}`} />
                    <span className="conn-label">{connected ? "live" : "offline"}</span>
                </div>
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

            {/* The day as one frontier line. */}
            <section className="hero" aria-label="Today's screen time">
                <div className="hero-meta">
                    <span className="eyebrow">Spent today</span>
                    <span className="hero-clock">
                        {summary
                            ? new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
                            : ""}
                    </span>
                </div>
                <p className="hero-total" dangerouslySetInnerHTML={{ __html: formatHero(total) }} />
                {summary ? (
                    <Horizon summary={summary} />
                ) : (
                    <div className="horizon" aria-hidden="true">
                        <div className="horizon-track">
                            <span className="horizon-seg horizon-empty" />
                        </div>
                    </div>
                )}
                {summary && summary.categories.length > 0 && <Legend categories={summary.categories} />}
            </section>

            {!connected && <p className="hint">Waiting for the agent�</p>}
            {connected && summary && total === 0 && (
                <div className="empty">
                    <p className="empty-title">A clean slate</p>
                    <p className="hint">No screen time recorded yet today. Go make some.</p>
                </div>
            )}

            {blockedApps.length > 0 && (
                <section className="alert-blocked" aria-label="Blocked apps">
                    <div className="alert-blocked-icon">?</div>
                    <div className="alert-blocked-body">
                        <h2 className="alert-blocked-title">Blocked right now</h2>
                        {blockedApps.map((a) => (
                            <div key={a.id} className="blocked-row">
                                <span className="row-label">{a.label}</span>
                                <button className="btn-primary" onClick={() => startOverride({ kind: "app", id: a.id })}>
                                    +15 min
                                </button>
                            </div>
                        ))}
                    </div>
                </section>
            )}

            {summary && summary.categories.length > 0 && (
                <section className="panel">
                    <h2 className="eyebrow">By category</h2>
                    <ul className="rows">
                        {summary.categories.map((c) => (
                            <UsageRowItem
                                key={c.id}
                                row={c}
                                total={total}
                                limit={limitForTarget(c.id)}
                                onEditLimit={(t) => startSetLimit(t, limitForTarget(c.id))}
                            />
                        ))}
                    </ul>
                </section>
            )}

            {summary && summary.apps.length > 0 && (
                <section className="panel">
                    <h2 className="eyebrow">By app</h2>
                    <ul className="rows">
                        {summary.apps.map((a) => (
                            <UsageRowItem
                                key={a.id}
                                row={a}
                                total={total}
                                limit={limitForTarget(a.id)}
                                onEditLimit={(t) => startSetLimit(t, limitForTarget(a.id))}
                            />
                        ))}
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
                    <div className="limit-add">
                        <select
                            className="limit-select"
                            onChange={(e) => {
                                const v = e.target.value;
                                if (v === "total") {
                                    const existing = limitFor({ kind: "total" });
                                    startSetLimit({ kind: "total" }, existing);
                                } else {
                                    const id = parseInt(v, 10);
                                    const existing = limitForTarget(id);
                                    startSetLimit({ kind: "category", id }, existing);
                                }
                            }}
                            value=""
                        >
                            <option value="" disabled>
                                Add a category limit�
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
                            className="limit-select"
                            onChange={(e) => {
                                const id = parseInt(e.target.value, 10);
                                if (Number.isNaN(id)) return;
                                const existing = limitForTarget(id);
                                startSetLimit({ kind: "app", id }, existing);
                            }}
                            value=""
                        >
                            <option value="" disabled>
                                Add an app limit�
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
                                <LimitRowItem
                                    key={l.id}
                                    limit={l}
                                    catalog={catalog}
                                    onEdit={(t) => startSetLimit(t, l)}
                                    onRemove={(t) => startDelete(t)}
                                />
                            ))}
                        </ul>
                    )}
                </section>
            )}

            {editingTarget && (
                <Modal title="Edit limit" eyebrow={targetLabel(editingTarget, catalog)} onClose={() => setEditingTarget(null)}>
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
                        <button className="btn-primary" onClick={confirmEdit}>Save</button>
                        <button className="ghost" onClick={() => setEditingTarget(null)}>
                            Cancel
                        </button>
                    </div>
                </Modal>
            )}

            {pinPromptFor && (
                <Modal
                    title="Enter PIN"
                    eyebrow={pinPromptFor.kind === "set_limit" ? "Change limit" : pinPromptFor.kind === "delete_limit" ? "Remove limit" : "Grant time"}
                    onClose={() => setPinPromptFor(null)}
                >
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
                        <button className="btn-primary" onClick={confirmPin}>Confirm</button>
                        <button className="ghost" onClick={() => setPinPromptFor(null)}>
                            Cancel
                        </button>
                    </div>
                </Modal>
            )}
        </main>
    );
}