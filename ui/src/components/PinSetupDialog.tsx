import { useState, type FormEvent } from "react";
import { describeError } from "../api";
import { Dialog } from "./Dialog";

interface PinSetupDialogProps {
    pinConfigured: boolean;
    busy: boolean;
    onClose: () => void;
    onSubmit: (newPin: string, currentPin: string) => Promise<void>;
}

export function PinSetupDialog({ pinConfigured, busy, onClose, onSubmit }: PinSetupDialogProps) {
    const [currentPin, setCurrentPin] = useState("");
    const [newPin, setNewPin] = useState("");
    const [confirmPin, setConfirmPin] = useState("");
    const [error, setError] = useState<string | null>(null);

    const submit = (event: FormEvent) => {
        event.preventDefault();
        if (busy) return;
        if (newPin.trim().length === 0) {
            setError("Enter a new PIN.");
            return;
        }
        if (newPin !== confirmPin) {
            setError("The two new PINs differ.");
            return;
        }
        setError(null);
        onSubmit(newPin.trim(), currentPin.trim()).catch((e: unknown) => setError(describeError(e)));
    };

    const guardedClose = () => {
        if (!busy) onClose();
    };

    return (
        <Dialog label="Set a PIN" onClose={guardedClose}>
            <form onSubmit={submit}>
                <p className="dialog-eyebrow">House key</p>
                <h2 className="dialog-title">Set a PIN</h2>
                <p className="panel-note">
                    A PIN gates standing-order changes and time grants. Optional until you set one.
                </p>
                {pinConfigured && (
                    <label className="field">
                        <span className="field-label">Current PIN</span>
                        <input
                            type="password"
                            inputMode="numeric"
                            autoComplete="off"
                            value={currentPin}
                            disabled={busy}
                            aria-label="Current PIN"
                            onChange={(event) => setCurrentPin(event.target.value)}
                        />
                    </label>
                )}
                <label className="field">
                    <span className="field-label">New PIN</span>
                    <input
                        type="password"
                        inputMode="numeric"
                        autoComplete="new-password"
                        value={newPin}
                        disabled={busy}
                        aria-label="New PIN"
                        onChange={(event) => setNewPin(event.target.value)}
                    />
                </label>
                <label className="field">
                    <span className="field-label">Repeat new PIN</span>
                    <input
                        type="password"
                        inputMode="numeric"
                        autoComplete="new-password"
                        value={confirmPin}
                        disabled={busy}
                        aria-label="Repeat new PIN"
                        onChange={(event) => setConfirmPin(event.target.value)}
                    />
                </label>
                {error !== null && <p className="dialog-error">{error}</p>}
                <div className="dialog-actions">
                    <button type="submit" className="btn btn--primary" disabled={busy}>
                        {busy ? "Setting…" : "Save PIN"}
                    </button>
                    <button type="button" className="btn btn--secondary" onClick={guardedClose} disabled={busy}>
                        Cancel
                    </button>
                </div>
            </form>
        </Dialog>
    );
}
