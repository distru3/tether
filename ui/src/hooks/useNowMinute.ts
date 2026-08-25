import { useEffect, useState } from "react";

function msToNextMinute(): number {
    const now = new Date();
    return 60000 - (now.getSeconds() * 1000 + now.getMilliseconds()) + 50;
}

export function useNowMinute(): Date {
    const [now, setNow] = useState(() => new Date());

    useEffect(() => {
        let kickoff = 0;
        let minuteTimer = 0;
        const arm = () => {
            kickoff = window.setTimeout(() => {
                setNow(new Date());
                minuteTimer = window.setInterval(() => setNow(new Date()), 60000);
            }, msToNextMinute());
        };
        arm();
        return () => {
            window.clearTimeout(kickoff);
            if (minuteTimer !== 0) window.clearInterval(minuteTimer);
        };
    }, []);

    return now;
}
