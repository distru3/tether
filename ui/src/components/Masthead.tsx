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
        <header className="masthead">
            <h1 className="masthead-brand">Screentime</h1>
            <p className="masthead-dateline">
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
        </header>
    );
}
