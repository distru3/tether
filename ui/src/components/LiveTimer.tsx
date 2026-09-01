import React, { useEffect, useState } from "react";

interface LiveTimerProps {
    expiresUtc: string;
}

export function LiveTimer({ expiresUtc }: LiveTimerProps) {
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
        <span className="badge badge-sm badge--info" style={{ fontFamily: "var(--font-mono)", fontWeight: 600, color: "oklch(0.9 0.05 240)", background: "oklch(0.4 0.1 260)" }}>
            ⏱ {formatted}
        </span>
    );
}
