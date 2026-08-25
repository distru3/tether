import { useMemo, useState, type FormEvent } from "react";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { LimitDto } from "../types/generated/LimitDto";
import type { LimitTargetDto } from "../types/generated/LimitTargetDto";
import { targetLabel } from "../format";
import { Dialog } from "./Dialog";

interface LimitEditorDialogProps {
    catalog: CatalogDto | null;
    target: LimitTargetDto | null;
    limit: LimitDto | null;
    busy: boolean;
    onSubmit: (target: LimitTargetDto, minutes: number, enabled: boolean) => void;
    onClose: () => void;
}

function encodeTarget(target: LimitTargetDto | null): string {
    if (target === null) return "";
    return target.kind === "total" ? "total" : `${target.kind}:${target.id}`;
}

function decodeTarget(value: string): LimitTargetDto | null {
    if (value === "total") return { kind: "total" };
    const [kind, rawId] = value.split(":");
    const id = Number.parseInt(rawId ?? "", 10);
    if ((kind === "app" || kind === "category") && Number.isFinite(id)) return { kind, id };
    return null;
}

export function LimitEditorDialog({ catalog, target, limit, busy, onSubmit, onClose }: LimitEditorDialogProps) {
    const limitableCategories = useMemo(
        () => (catalog?.categories ?? []).filter((category) => category.kind === "limitable"),
        [catalog],
    );
    const [chosen, setChosen] = useState(() => encodeTarget(target));
    const [minutes, setMinutes] = useState(() => String(limit?.default_minutes ?? 60));
    const [enabled, setEnabled] = useState(() => limit?.enabled ?? true);
    const [error, setError] = useState<string | null>(null);

    const locked = target !== null;

    const submit = (event: FormEvent) => {
        event.preventDefault();
        if (busy) return;
        const parsed = Number.parseInt(minutes, 10);
        if (!Number.isFinite(parsed) || parsed < 0 || parsed > 1440) {
            setError("Enter minutes between 0 and 1440.");
            return;
        }
        const finalTarget = locked ? target : decodeTarget(chosen);
        if (finalTarget === null) {
            setError("Pick what the order applies to.");
            return;
        }
        onSubmit(finalTarget, parsed, enabled);
    };

    return (
        <Dialog
            label={locked ? "Edit standing order" : "New standing order"}
            onClose={() => {
                if (!busy) onClose();
            }}
        >
            <form onSubmit={submit}>
                <p className="dialog-eyebrow">{locked ? "Edit order" : "New order"}</p>
                <h2 className="dialog-title">
                    {locked && target !== null ? targetLabel(target, catalog) : "Choose a target"}
                </h2>
                {!locked && (
                    <label className="field">
                        <span className="field-label">Applies to</span>
                        <select
                            value={chosen}
                            disabled={busy}
                            onChange={(event) => {
                                setChosen(event.target.value);
                                setError(null);
                            }}
                        >
                            <option value="" disabled>
                                Choose…
                            </option>
                            <option value="total">Total screen time</option>
                            {limitableCategories.map((category) => (
                                <option key={`category:${category.id}`} value={`category:${category.id}`}>
                                    {category.name}
                                </option>
                            ))}
                            {(catalog?.apps ?? []).map((app) => (
                                <option key={`app:${app.id}`} value={`app:${app.id}`}>
                                    {app.display_name}
                                </option>
                            ))}
                        </select>
                    </label>
                )}
                <label className="field">
                    <span className="field-label">Minutes per day</span>
                    <input
                        type="number"
                        min={0}
                        max={1440}
                        step={1}
                        value={minutes}
                        disabled={busy}
                        onChange={(event) => {
                            setMinutes(event.target.value);
                            setError(null);
                        }}
                    />
                </label>
                <label className="field field--inline">
                    <input
                        type="checkbox"
                        checked={enabled}
                        disabled={busy}
                        onChange={(event) => setEnabled(event.target.checked)}
                    />
                    <span>In force</span>
                </label>
                {error !== null && <p className="dialog-error">{error}</p>}
                <div className="dialog-actions">
                    <button type="submit" className="btn btn--primary" disabled={busy}>
                        {busy ? "Setting…" : "Save order"}
                    </button>
                    <button
                        type="button"
                        className="btn btn--secondary"
                        onClick={() => {
                            if (!busy) onClose();
                        }}
                        disabled={busy}
                    >
                        Cancel
                    </button>
                </div>
            </form>
        </Dialog>
    );
}
