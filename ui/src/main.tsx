import { getCurrentWindow } from "@tauri-apps/api/window";
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./i18n";
import { initDirection } from "./i18n";
import "./styles/tokens.css";
import "./styles/app.css";
import "./styles/redesign.css";

const root = document.getElementById("root");
if (!root) {
    throw new Error("missing #root element");
}

// Apply RTL/LTR direction based on saved language.
initDirection();

getCurrentWindow().show();

ReactDOM.createRoot(root).render(
    <React.StrictMode>
        <App />
    </React.StrictMode>,
);
