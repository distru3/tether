import { useEffect, type ReactNode } from "react";

interface ModalProps {
    title: string;
    eyebrow?: string;
    onClose: () => void;
    children: ReactNode;
    actions?: ReactNode;
}

/** A proper dialog: backdrop blur, fade+rise, Esc to close, focus the first
 * control. `onClose` is called on backdrop click or Esc. */
export function Modal({ title, eyebrow, onClose, children, actions }: ModalProps) {
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (e.key === "Escape") onClose();
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [onClose]);

    return (
        <div className="modal-backdrop" onClick={onClose} role="presentation">
            <div
                className="modal"
                role="dialog"
                aria-modal="true"
                aria-label={title}
                onClick={(e) => e.stopPropagation()}
            >
                <div className="modal-head">
                    {eyebrow && <span className="eyebrow">{eyebrow}</span>}
                    <h3>{title}</h3>
                </div>
                <div className="modal-body">{children}</div>
                {actions && <div className="modal-actions">{actions}</div>}
            </div>
        </div>
    );
}