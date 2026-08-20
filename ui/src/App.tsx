import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Shape must match the Rust `AgentStatus` struct in `ui/src-tauri/src/lib.rs`.
// The two definitions are hand-kept in sync for now; before the dashboard grows
// beyond a handful of fields, generate this from the Rust types with `ts-rs`
// or `specta` so drift becomes impossible.
interface AgentStatus {
    version: string;
    agent_connected: boolean;
    tracker_backend: string;
    filter_backend: string;
    tracking_available: boolean;
}

export function App() {
    const [status, setStatus] = useState<AgentStatus | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        // `get_status` is a #[tauri::command] defined in src-tauri/src/lib.rs.
        // Every UI-to-Rust call is invoked this way; it is Tauri's replacement
        // for Electron's IPC and it works over a WebSocket-like channel inside
        // the app's own process.
        invoke<AgentStatus>("get_status")
            .then(setStatus)
            .catch((e: unknown) => setError(String(e)));
    }, []);

    return (
        <main className="app">
            <header>
                <h1>Screentime</h1>
                <p className="subtitle">
                    Development shell. The dashboard is a stub until M1 wires it to the agent.
                </p>
            </header>

            {error && <p className="error">Command failed: {error}</p>}

            {status ? (
                <section className="status-card">
                    <h2>Agent status</h2>
                    <dl>
                        <dt>Version</dt>
                        <dd>{status.version}</dd>

                        <dt>Agent connected</dt>
                        <dd>{status.agent_connected ? "yes" : "no (M2)"}</dd>

                        <dt>Tracker backend</dt>
                        <dd>{status.tracker_backend}</dd>

                        <dt>Filter backend</dt>
                        <dd>{status.filter_backend}</dd>

                        <dt>Tracking available</dt>
                        <dd>{status.tracking_available ? "yes" : "no"}</dd>
                    </dl>
                </section>
            ) : (
                !error && <p>Loading…</p>
            )}
        </main>
    );
}
