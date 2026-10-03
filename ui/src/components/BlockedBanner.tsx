import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import type { CatalogDto } from "../types/generated/CatalogDto";
import { BlockedIcon } from "./icons/Icons";

interface BlockedBannerProps {
    blocked: UsageRowDto[];
    busy: boolean;
    onOverride: (row: UsageRowDto) => void;
    catalog?: CatalogDto | null;
    /** Minutes after local midnight at which the day (and every block) resets. */
    dayStartMinutes?: number;
    /** Strict mode refuses extensions, so the button would only fail. */
    strictMode?: boolean;
}

/** Minutes until the next day boundary, honouring the configured day start. */
function minutesUntilReset(now: Date, dayStartMinutes: number): number {
    const reset = new Date(now);
    reset.setHours(0, dayStartMinutes, 0, 0);
    if (reset.getTime() <= now.getTime()) reset.setDate(reset.getDate() + 1);
    return Math.max(0, Math.floor((reset.getTime() - now.getTime()) / 60000));
}

export function BlockedBanner({ blocked, busy, onOverride, dayStartMinutes = 0, strictMode = false }: BlockedBannerProps) {
    const { t } = useTranslation();
    const [now, setNow] = useState(() => new Date());

    useEffect(() => {
        const timer = window.setInterval(() => setNow(new Date()), 60000);
        return () => window.clearInterval(timer);
    }, []);

    if (blocked.length === 0) return null;

    const totalMinutes = minutesUntilReset(now, dayStartMinutes);
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;

    return (
        <section className="tt-card tt-blocked" aria-labelledby="blocked-title">
            <div className="tt-card-head">
                <h2 id="blocked-title" className="tt-card-title">{t("banner.title", { count: blocked.length })}</h2>
                <span className="tt-sub">{t("banner.resetsIn", { hours, minutes })}</span>
            </div>
            <ul className="tt-list">
                {blocked.map((row) => (
                    <li key={row.id} className="tt-row tt-blocked-row">
                        <span className="tt-blocked-name">
                            <BlockedIcon size={16} aria-hidden="true" />
                            <strong>{row.label}</strong>
                        </span>
                        {!strictMode && (
                            <button type="button" className="tt-btn tt-btn--outline tt-btn--sm" disabled={busy} onClick={() => onOverride(row)}>
                                {t("banner.override")}
                            </button>
                        )}
                    </li>
                ))}
            </ul>
        </section>
    );
}
