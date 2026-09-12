import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { UsageRowDto } from "../types/generated/UsageRowDto";
import type { CatalogDto } from "../types/generated/CatalogDto";
import { colorForCategory } from "../categoryColors";
import { BlockedIcon } from "./icons/Icons";

interface BlockedBannerProps {
    blocked: UsageRowDto[];
    busy: boolean;
    onOverride: (row: UsageRowDto) => void;
    catalog?: CatalogDto | null;
}

function minutesUntilLocalMidnight(now: Date): number {
    const midnight = new Date(now);
    midnight.setHours(24, 0, 0, 0);
    return Math.max(0, Math.floor((midnight.getTime() - now.getTime()) / 60000));
}

export function BlockedBanner({ blocked, busy, onOverride, catalog }: BlockedBannerProps) {
    const { t } = useTranslation();
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
                    <span className="badge badge--danger">{t("banner.enforcementActive", { count: blocked.length })}</span>
                </div>
                <span className="blocked-reset-text font-mono">
                    {t("banner.resetsIn", { hours, minutes })}
                </span>
            </div>

            <div className="blocked-items-list">
                {blocked.map((row, index) => {
                    const appObj = catalog?.apps.find((a) => a.id === row.id);
                    const category = appObj ? catalog?.categories.find((c) => c.id === appObj.primary_category) : null;
                    const catName = category?.name ?? "Uncategorized";
                    const catColor = colorForCategory(catName, category?.color ?? row.color, index);

                    return (
                        <div key={row.id} className="blocked-item-row">
                            <div className="blocked-item-info">
                                <BlockedIcon size={16} color="var(--accent-rose)" />
                                <strong className="blocked-item-name">{row.label}</strong>
                                <span 
                                    className="usage-category-pill"
                                    style={{
                                        color: catColor,
                                        backgroundColor: `${catColor}1c`,
                                        borderColor: `${catColor}38`,
                                    }}
                                >
                                    {catName}
                                </span>
                            </div>
                            <button
                                type="button"
                                className="btn btn-danger btn-sm"
                                disabled={busy}
                                onClick={() => onOverride(row)}
                            >
                                {t("banner.override")}
                            </button>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
