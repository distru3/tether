import React, { useEffect, useState } from "react";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import { BlockedIcon } from "./icons/Icons";

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
        <div className="glass-card blocked-banner-card animate-pulse-subtle">
            <div className="blocked-banner-header">
                <div className="blocked-badge-group">
                    <span className="pulse-indicator pulse-indicator--danger" />
                    <span className="badge badge--danger">ENFORCEMENT ACTIVE ({blocked.length})</span>
                </div>
                <span className="blocked-reset-text font-mono">
                    Resets in {hours}h {minutes}m
                </span>
            </div>

            <div className="blocked-items-list">
                {blocked.map((row) => (
                    <div key={row.id} className="blocked-item-row">
                        <div className="blocked-item-info">
                            <BlockedIcon size={16} color="var(--accent-rose)" />
                            <strong className="blocked-item-name">{row.label}</strong>
                        </div>
                        <button
                            type="button"
                            className="btn btn-danger btn-sm"
                            disabled={busy}
                            onClick={() => onOverride(row)}
                        >
                            +15 min Override
                        </button>
                    </div>
                ))}
            </div>
        </div>
    );
}
