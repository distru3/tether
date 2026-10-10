import { getCurrentWindow } from "@tauri-apps/api/window";
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./i18n";
import { initDirection } from "./i18n";
// Fonts are bundled, not fetched: the app's CSP (`default-src 'self'`) blocks
// Google Fonts, and a screen-time guard must look right offline.
import "@fontsource-variable/rubik";
import "@fontsource-variable/unbounded";
import "@fontsource-variable/jetbrains-mono";
import "./styles/tokens.css";
import "./styles/app.css";
import "./styles/redesign.css";
import "./styles/tether.css";

const root = document.getElementById("root");
if (!root) {
    throw new Error("missing #root element");
}

// Apply RTL/LTR direction based on saved language.
initDirection();

// Every window loads this page, but only the dashboard shows itself once
// loaded. The tray panel (`tray`) and the block screen (`overlay`) are
// always-on-top windows the host opens when needed (tray_panel.rs,
// overlay_bridge.rs); showing them here put an invisible window over the
// dashboard at startup that swallowed clicks.
const current = getCurrentWindow();
if (current.label === "main") {
    void current.show();
}

ReactDOM.createRoot(root).render(
    <React.StrictMode>
        <App />
    </React.StrictMode>,
);
