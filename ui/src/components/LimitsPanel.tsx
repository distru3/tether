import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import { describeWeekdayOverrides, targetLabel } from "../format";
import { Section } from "./Section";

interface LimitsPanelProps {
    limits: LimitDto[];
    catalog: CatalogDto | null;
    busy: boolean;
    pinConfigured: boolean;
    onToggle: (limit: LimitDto, next: boolean) => void;
    onEdit: (target: LimitTargetDto, limit: LimitDto | null) => void;
    onRemove: (target: LimitTargetDto) => void;
    onNew: () => void;
    onOpenPinSetup: () => void;
}

export function LimitsPanel({
    limits,
    catalog,
    busy,
    pinConfigured,
    onToggle,
    onEdit,
    onRemove,
    onNew,
    onOpenPinSetup,
}: LimitsPanelProps) {
    return (
        <Section label={`Standing orders — ${limits.length}`}>
            {limits.length === 0 ? (
                <p className="panel-note">No standing orders yet.</p>
            ) : (
                <ul>
                    {limits.map((limit) => {
                        const varies = describeWeekdayOverrides(limit.weekday_minutes);
                        return (
                            <li key={limit.id} className="orow">
                                <span className="orow-target">{targetLabel(limit.target, catalog)}</span>
                                <span className="orow-amount">
                                    {limit.default_minutes}m / day
                                    {varies !== null && (
                                        <span className="orow-varies" title={`Per-day: ${varies}`}>
                                            {" "}
                                            · varies
                                        </span>
                                    )}
                                </span>
                                <span className="check-group">
                                    <input
                                        type="checkbox"
                                        id={`order-${limit.id}`}
                                        className="check-input"
                                        checked={limit.enabled}
                                        disabled={busy}
                                        onChange={(event) => onToggle(limit, event.target.checked)}
                                    />
                                    <label htmlFor={`order-${limit.id}`} className="check-label">
                                        {limit.enabled ? "in force" : "suspended"}
                                    </label>
                                </span>
                                <span className="orow-actions">
                                    <button
                                        type="button"
                                        className="textbtn"
                                        disabled={busy}
                                        onClick={() => onEdit(limit.target, limit)}
                                    >
                                        Edit
                                    </button>
                                    <button
                                        type="button"
                                        className="textbtn textbtn--red"
                                        disabled={busy}
                                        onClick={() => onRemove(limit.target)}
                                    >
                                        Remove
                                    </button>
                                </span>
                            </li>
                        );
                    })}
                </ul>
            )}
            <button type="button" className="add-order" disabled={busy} onClick={onNew}>
                + New standing order
            </button>
            {!pinConfigured && (
                <p className="panel-note panel-note--action">
                    Orders are ungated until you set a PIN.{" "}
                    <button type="button" className="textbtn" onClick={onOpenPinSetup}>
                        Set a PIN
                    </button>
                </p>
            )}
        </Section>
    );
}
