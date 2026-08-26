import { getCurrentWindow } from "@tauri-apps/api/window";

import { formatDateline } from "../format";
import type { Phase } from "../hooks/useDashboard";

interface MastheadProps {
    phase: Phase;
    now: Date;
}

export function Masthead({ phase, now }: MastheadProps) {
    const settled = phase !== "connecting";
    const live = phase === "live";
    return (
        // The whole bar drags the frameless window; the control buttons are
        // separate elements so their clicks are their own.
        <header className="masthead" data-tauri-drag-region>
            <h1 className="masthead-brand" data-tauri-drag-region>
                Screentime
            </h1>
            <p className="masthead-dateline" data-tauri-drag-region>
                <span>{formatDateline(now)}</span>
                {settled && (
                    <>
                        <span aria-hidden="true"> · </span>
                        <span>{live ? "LIVE" : "OFFLINE"}</span>
                        <span
                            className={live ? "status-dot status-dot--live" : "status-dot status-dot--off"}
                            aria-hidden="true"
                        />
                    </>
                )}
            </p>
            <div className="wincontrols">
                <button
                    type="button"
                    className="winbtn"
                    aria-label="Minimize window"
                    onClick={() => void getCurrentWindow().minimize()}
                >
                    –
                </button>
                <button
                    type="button"
                    className="winbtn"
                    aria-label="Toggle maximize window"
                    onClick={() => void getCurrentWindow().toggleMaximize()}
                >
                    ▢
                </button>
                <button
                    type="button"
                    className="winbtn winbtn--close"
                    aria-label="Close window"
                    onClick={() => void getCurrentWindow().close()}
                >
                    ✕
                </button>
            </div>
        </header>
    );
}
