import { useState, type FormEvent } from "react";

import { describeError, recoverPin } from "../api";
import { Dialog } from "./Dialog";

interface PinGateProps {
    label: string;
    error: string | null;
    busy: boolean;
    onSubmit: (pin: string) => void;
    onClose: () => void;
}

export function PinGate({ label, error, busy, onSubmit, onClose }: PinGateProps) {
    const [mode, setMode] = useState<"pin" | "forgot">("pin");
    const [pin, setPin] = useState("");
    const [code, setCode] = useState("");
    const [newPin, setNewPin] = useState("");
    const [confirmPin, setConfirmPin] = useState("");
    const [localError, setLocalError] = useState<string | null>(null);
    const [recovering, setRecovering] = useState(false);

    const submit = (event: FormEvent) => {
        event.preventDefault();
        const value = pin.trim();
        if (value.length === 0 || busy) return;
        setPin("");
        onSubmit(value);
    };

    const submitRecovery = async (event: FormEvent) => {
        event.preventDefault();
        if (recovering) return;
        if (code.trim().length === 0 || newPin.trim().length === 0) {
            setLocalError("Fill in the recovery code and a new PIN.");
            return;
        }
        if (newPin !== confirmPin) {
            setLocalError("The two new PINs differ.");
            return;
        }
        setLocalError(null);
        setRecovering(true);
        try {
            await recoverPin(code.trim(), newPin.trim());
            // Vault rotated: back to sign-in with the PIN they now know.
            setMode("pin");
            setCode("");
            setNewPin("");
            setConfirmPin("");
            setPin("");
            setLocalError(null);
        } catch (e: unknown) {
            setLocalError(describeError(e));
        } finally {
            setRecovering(false);
        }
    };

    const guardedClose = () => {
        if (!busy && !recovering) onClose();
    };

    return (
        <Dialog label="PIN required" onClose={guardedClose}>
            {mode === "pin" ? (
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
                    {localError !== null && mode === "pin" && (
                        <p className="dialog-error">{localError}</p>
                    )}
                    <div className="dialog-actions">
                        <button
                            type="submit"
                            className="btn btn--primary"
                            disabled={busy || pin.trim().length === 0}
                        >
                            {busy ? "Checking…" : "Confirm"}
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
                    <button
                        type="button"
                        className="linklike"
                        onClick={() => {
                            setLocalError(null);
                            setMode("forgot");
                        }}
                        disabled={busy || recovering}
                    >
                        Forgot the PIN?
                    </button>
                </form>
            ) : (
                <form onSubmit={(e) => void submitRecovery(e)}>
                    <p className="dialog-eyebrow">Recovery</p>
                    <h2 className="dialog-title">Forgot the PIN</h2>
                    <p className="panel-note">
                        Enter the recovery code you wrote down when the PIN was set, and choose a
                        replacement. The old code stops working immediately.
                    </p>
                    <label className="field">
                        <span className="field-label">Recovery code</span>
                        <input
                            className="recovery-input"
                            type="text"
                            autoComplete="off"
                            spellCheck={false}
                            placeholder="XXXX-XXXX-XXXX-XXXX"
                            value={code}
                            disabled={recovering}
                            aria-label="Recovery code"
                            onChange={(event) => {
                                setCode(event.target.value);
                                setLocalError(null);
                            }}
                        />
                    </label>
                    <label className="field">
                        <span className="field-label">New PIN</span>
                        <input
                            type="password"
                            inputMode="numeric"
                            autoComplete="new-password"
                            value={newPin}
                            disabled={recovering}
                            aria-label="New PIN"
                            onChange={(event) => {
                                setNewPin(event.target.value);
                                setLocalError(null);
                            }}
                        />
                    </label>
                    <label className="field">
                        <span className="field-label">Repeat new PIN</span>
                        <input
                            type="password"
                            inputMode="numeric"
                            autoComplete="new-password"
                            value={confirmPin}
                            disabled={recovering}
                            aria-label="Repeat new PIN"
                            onChange={(event) => {
                                setConfirmPin(event.target.value);
                                setLocalError(null);
                            }}
                        />
                    </label>
                    {(localError !== null || error !== null) && (
                        <p className="dialog-error">{localError ?? error}</p>
                    )}
                    <div className="dialog-actions">
                        <button
                            type="submit"
                            className="btn btn--primary"
                            disabled={recovering}
                        >
                            {recovering ? "Replacing…" : "Replace PIN"}
                        </button>
                        <button
                            type="button"
                            className="btn btn--secondary"
                            onClick={() => {
                                setLocalError(null);
                                setMode("pin");
                            }}
                            disabled={recovering}
                        >
                            Back
                        </button>
                    </div>
                </form>
            )}
        </Dialog>
    );
}
