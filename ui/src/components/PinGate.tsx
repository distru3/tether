import { useState, type FormEvent } from "react";
import { Dialog } from "./Dialog";

interface PinGateProps {
    label: string;
    error: string | null;
    busy: boolean;
    onSubmit: (pin: string) => void;
    onClose: () => void;
}

export function PinGate({ label, error, busy, onSubmit, onClose }: PinGateProps) {
    const [pin, setPin] = useState("");

    const submit = (event: FormEvent) => {
        event.preventDefault();
        const value = pin.trim();
        if (value.length === 0 || busy) return;
        setPin("");
        onSubmit(value);
    };

    const guardedClose = () => {
        if (!busy) onClose();
    };

    return (
        <Dialog label="PIN required" onClose={guardedClose}>
            <form onSubmit={submit}>
                <p className="dialog-eyebrow">PIN required</p>
                <h2 className="dialog-title">{label}</h2>
                <input
                    className="pin-input"
                    type="password"
                    inputMode="numeric"
                    autoComplete="off"
                    placeholder="····"
                    value={pin}
                    disabled={busy}
                    aria-label="PIN"
                    onChange={(event) => setPin(event.target.value)}
                />
                {error !== null && <p className="dialog-error">{error}</p>}
                <div className="dialog-actions">
                    <button
                        type="submit"
                        className="btn btn--primary"
                        disabled={busy || pin.trim().length === 0}
                    >
                        {busy ? "Checking…" : "Confirm"}
                    </button>
                    <button type="button" className="btn btn--secondary" onClick={guardedClose} disabled={busy}>
                        Cancel
                    </button>
                </div>
            </form>
        </Dialog>
    );
}
