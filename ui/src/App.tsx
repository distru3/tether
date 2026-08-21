import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Refresh the dashboard this often. The agent writes usage in the background;
// without polling the window freezes at whatever it first fetched.
const REFRESH_MS = 3000;

// Shapes mirror the Rust types in ui/src-tauri/src/lib.rs; hand-kept in sync
// until we generate these from the Rust types.
interface AgentStatus {
    version: string;
    agent_connected: boolean;
    tracker_backend: string;
    filter_backend: string;
    tracking_available: boolean;
}

interface UsageRow {
    id: number;
    label: string;
    seconds: number;
    color: string | null;
}

interface DaySummary {
    day: number;
    totalSeconds: number;
    apps: UsageRow[];
    categories: UsageRow[];
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

export function App() {
    const [status, setStatus] = useState<AgentStatus | null>(null);
    const [summary, setSummary] = useState<DaySummary | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        // Refresh on an interval so the dashboard tracks live usage. A failed
        // fetch keeps the last good data on screen; only a failed *first* load
        // surfaces an error, so the agent starting late isn't treated as fatal.
        let first = true;

        const refresh = () => {
            invoke<AgentStatus>("get_status")
                .then(setStatus)
                .catch((e: unknown) => {
                    if (first) setError(String(e));
                });

            invoke<DaySummary>("get_day_summary", { day: todayKey() })
                .then(setSummary)
                .catch((e: unknown) => {
                    if (first) setError(String(e));
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

    return (
        <main className="app">
            <header>
                <h1>Screentime</h1>
                <p className="subtitle">
                    {connected && status
                        ? `Connected · ${status.tracker_backend} · v${status.version}`
                        : "Agent not running — start screentime-agent to see live data"}
                </p>
            </header>

            {error && <p className="error">Command failed: {error}</p>}

            <section className="hero" aria-label="Today's screen time">
                <p className="eyebrow">
                    Today
                    {updated && <span className="updated">updated {updated}</span>}
                </p>
                <p className="hero-total">{formatDuration(total)}</p>
                <div className="day-bar" aria-hidden="true">
                    {summary?.categories.length ? (
                        summary.categories.map((c) => (
                            <span
                                key={c.id}
                                className="day-bar-seg"
                                style={{
                                    background: c.color ?? "#94a3b8",
                                    flexGrow: c.seconds,
                                }}
                            />
                        ))
                    ) : (
                        <span className="day-bar-seg day-bar-empty" />
                    )}
                </div>
            </section>

            {!connected && !error && <p className="hint">Waiting for the agent…</p>}

            {summary && summary.categories.length > 0 && (
                <section className="panel" aria-label="By category">
                    <h2 className="eyebrow">By category</h2>
                    <ul className="rows">
                        {summary.categories.map((c) => (
                            <li key={c.id} className="row">
                                <span
                                    className="dot"
                                    style={{ background: c.color ?? "#94a3b8" }}
                                />
                                <span className="row-label">{c.label}</span>
                                <span className="row-time">{formatDuration(c.seconds)}</span>
                                <span className="row-pct">{percent(c.seconds, total)}%</span>
                            </li>
                        ))}
                    </ul>
                </section>
            )}

            {summary && summary.apps.length > 0 && (
                <section className="panel" aria-label="By app">
                    <h2 className="eyebrow">By app</h2>
                    <ul className="rows">
                        {summary.apps.map((a) => (
                            <li key={a.id} className="row">
                                <span
                                    className="dot"
                                    style={{ background: a.color ?? "#94a3b8" }}
                                />
                                <span className="row-label">{a.label}</span>
                                <span className="row-time">{formatDuration(a.seconds)}</span>
                                <span className="row-pct">{percent(a.seconds, total)}%</span>
                            </li>
                        ))}
                    </ul>
                </section>
            )}

            {connected && summary && total === 0 && (
                <p className="hint">No screen time recorded yet today.</p>
            )}
        </main>
    );
}