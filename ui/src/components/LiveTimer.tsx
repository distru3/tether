import React, { useEffect, useState } from "react";
import { Timer } from "lucide-react";
import "./LiveTimer.css";

interface LiveTimerProps {
    expiresUtc: string;
    showIcon?: boolean;
    showPulse?: boolean;
    showLabel?: boolean;
    label?: string;
    className?: string;
}

export function LiveTimer({
    expiresUtc,
    showIcon = true,
    showPulse = true,
    showLabel = false,
    label = "+15m",
    className = "",
}: LiveTimerProps) {
    const [remaining, setRemaining] = useState<number>(0);

    useEffect(() => {
        const calculateRemaining = () => {
            const expires = new Date(expiresUtc).getTime();
            const now = Date.now();
            return Math.max(0, Math.floor((expires - now) / 1000));
        };

        setRemaining(calculateRemaining());

        const interval = setInterval(() => {
            setRemaining(calculateRemaining());
        }, 1000);

        return () => clearInterval(interval);
    }, [expiresUtc]);

    if (remaining <= 0) return null;

    const hrs = Math.floor(remaining / 3600);
    const mins = Math.floor((remaining % 3600) / 60);
    const secs = remaining % 60;

    const formatted = hrs > 0 
        ? `${hrs}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`
        : `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;

    return (
        <span className={`live-timer-badge ${className}`} title={`Extended time remaining: ${formatted}`}>
            {showPulse && <span className="live-timer-dot-pulse" />}
            {showIcon && <Timer size={12} className="live-timer-icon" />}
            {showLabel && <span className="live-timer-label">{label}</span>}
            <span className="live-timer-digits font-mono">{formatted}</span>
        </span>
    );
}
