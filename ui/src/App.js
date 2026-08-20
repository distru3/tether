import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
export function App() {
    const [status, setStatus] = useState(null);
    const [error, setError] = useState(null);
    useEffect(() => {
        // `get_status` is a #[tauri::command] defined in src-tauri/src/lib.rs.
        // Every UI-to-Rust call is invoked this way; it is Tauri's replacement
        // for Electron's IPC and it works over a WebSocket-like channel inside
        // the app's own process.
        invoke("get_status")
            .then(setStatus)
            .catch((e) => setError(String(e)));
    }, []);
    return (_jsxs("main", { className: "app", children: [_jsxs("header", { children: [_jsx("h1", { children: "Screentime" }), _jsx("p", { className: "subtitle", children: "Development shell. The dashboard is a stub until M1 wires it to the agent." })] }), error && _jsxs("p", { className: "error", children: ["Command failed: ", error] }), status ? (_jsxs("section", { className: "status-card", children: [_jsx("h2", { children: "Agent status" }), _jsxs("dl", { children: [_jsx("dt", { children: "Version" }), _jsx("dd", { children: status.version }), _jsx("dt", { children: "Agent connected" }), _jsx("dd", { children: status.agent_connected ? "yes" : "no (M2)" }), _jsx("dt", { children: "Tracker backend" }), _jsx("dd", { children: status.tracker_backend }), _jsx("dt", { children: "Filter backend" }), _jsx("dd", { children: status.filter_backend }), _jsx("dt", { children: "Tracking available" }), _jsx("dd", { children: status.tracking_available ? "yes" : "no" })] })] })) : (!error && _jsx("p", { children: "Loading\u2026" }))] }));
}
