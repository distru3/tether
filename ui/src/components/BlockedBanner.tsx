import { useEffect, useState } from "react";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import { Section } from "./Section";

interface BlockedBannerProps {
    blocked: UsageRowDto[];
    busy: boolean;
    onOverride: (row: UsageRowDto) => void;
}

function minutesUntilLocalMidnight(now: Date): number {
    const midnight = new Date(now);
    midnight.setHours(24, 0, 0, 0);
    return Math.max(0, Math.floor((midnight.getTime() - now.getTime()) / 60000));
}

export function BlockedBanner({ blocked, busy, onOverride }: BlockedBannerProps) {
    // Blocks expire at local midnight, so the countdown rides the wall clock
    // whether or not any rows are currently listed.
    const [now, setNow] = useState(() => new Date());

    useEffect(() => {
        const timer = window.setInterval(() => setNow(new Date()), 60000);
        return () => window.clearInterval(timer);
    }, []);

    if (blocked.length === 0) return null;

    const totalMinutes = minutesUntilLocalMidnight(now);
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;

    return (
        <Section label={`Enforcement — ${blocked.length}`}>
            <div className="blocked-banner">
                <span className="stamp" aria-hidden="true">
                    Over limit
                </span>
                <ul className="blocked-list">
                    {blocked.map((row) => (
                        <li key={row.id} className="blocked-item">
                            <span className="blocked-name">{row.label}</span>
                            <button
                                type="button"
                                className="textbtn textbtn--red"
                                disabled={busy}
                                onClick={() => onOverride(row)}
                            >
                                +15 min
                            </button>
                        </li>
                    ))}
                </ul>
                <span className="blocked-reset">
                    Resets in {hours}h {minutes}m
                </span>
            </div>
        </Section>
    );
}
