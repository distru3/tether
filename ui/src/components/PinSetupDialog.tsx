import { useState, type FormEvent } from "react";

import { describeError, removePin, type PinVaultReply } from "../api";
import { Dialog } from "./Dialog";

interface PinSetupDialogProps {
    pinConfigured: boolean;
    busy: boolean;
    onClose: () => void;
    onSubmit: (newPin: string, currentPin: string) => Promise<PinVaultReply>;
}

export function PinSetupDialog({ pinConfigured, busy, onClose, onSubmit }: PinSetupDialogProps) {
    const [currentPin, setCurrentPin] = useState("");
    const [newPin, setNewPin] = useState("");
    const [confirmPin, setConfirmPin] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [issuedCode, setIssuedCode] = useState<string | null>(null);
    const [removing, setRemoving] = useState(false);
    const [removeCredential, setRemoveCredential] = useState("");

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
        onSubmit(newPin.trim(), currentPin.trim())
            .then((reply) => setIssuedCode(reply.recovery_code))
            .catch((e: unknown) => setError(describeError(e)));
    };

    const submitRemove = async () => {
        if (removing) return;
        setRemoving(true);
        try {
            await removePin(removeCredential.trim());
            onClose();
        } catch (e: unknown) {
            setError(describeError(e));
        } finally {
            setRemoving(false);
        }
    };

    const guardedClose = () => {
        if (!busy && !removing) onClose();
    };

    if (issuedCode !== null) {
        return (
            <Dialog label="Recovery code" onClose={guardedClose}>
                <p className="dialog-eyebrow">House key</p>
                <h2 className="dialog-title">Write this down</h2>
                <p className="panel-note">
                    This is the recovery code for your new PIN. It is shown <strong>once</strong> —
                    the app stores only a hash, so nobody can show it again. It replaces the PIN
                    if you forget it, and stops working the next time the PIN changes.
                </p>
                <div className="recovery-code" role="textbox" aria-label="Recovery code" tabIndex={0}>
                    {issuedCode}
                </div>
                <div className="dialog-actions">
                    <button
                        type="button"
                        className="btn btn--primary"
                        onClick={onClose}
                        disabled={removing}
                    >
                        I've written it down
                    </button>
                </div>
            </Dialog>
        );
    }

    return (
        <Dialog label="Set a PIN" onClose={guardedClose}>
            <form onSubmit={submit}>
                <p className="dialog-eyebrow">House key</p>
                <h2 className="dialog-title">{pinConfigured ? "Change the PIN" : "Set a PIN"}</h2>
                <p className="panel-note">
                    A PIN gates standing-order changes and time grants. Optional until you set one.
                </p>
                {pinConfigured && (
                    <label className="field">
                        <span className="field-label">Current PIN or recovery code</span>
                        <input
                            type="password"
                            autoComplete="off"
                            value={currentPin}
                            disabled={busy}
                            aria-label="Current PIN or recovery code"
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
                    <button
                        type="button"
                        className="btn btn--secondary"
                        onClick={guardedClose}
                        disabled={busy}
                    >
                        Cancel
                    </button>
                </div>
            </form>
            {pinConfigured && (
                <div className="remove-vault">
                    <p className="dialog-eyebrow">Stand down the gate</p>
                    <div className="remove-vault-row">
                        <input
                            type="password"
                            autoComplete="off"
                            placeholder="PIN or recovery code"
                            value={removeCredential}
                            disabled={removing || busy}
                            aria-label="PIN or recovery code to remove the vault"
                            onChange={(event) => {
                                setRemoveCredential(event.target.value);
                                setError(null);
                            }}
                        />
                        <button
                            type="button"
                            className="linklike linklike--danger"
                            disabled={removing || busy || removeCredential.trim().length === 0}
                            onClick={() => void submitRemove()}
                        >
                            {removing ? "Removing…" : "Remove PIN"}
                        </button>
                    </div>
                </div>
            )}
        </Dialog>
    );
}
