import type { UsageRowDto } from "../types/generated/UsageRowDto";
import { Section } from "./Section";

interface BlockedBannerProps {
    blocked: UsageRowDto[];
    busy: boolean;
    onOverride: (row: UsageRowDto) => void;
}

export function BlockedBanner({ blocked, busy, onOverride }: BlockedBannerProps) {
    if (blocked.length === 0) return null;

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
            </div>
        </Section>
    );
}
