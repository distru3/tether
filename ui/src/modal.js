import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useEffect } from "react";
/** A proper dialog: backdrop blur, fade+rise, Esc to close, focus the first
 * control. `onClose` is called on backdrop click or Esc. */
export function Modal({ title, eyebrow, onClose, children, actions }) {
    useEffect(() => {
        const onKey = (e) => {
            if (e.key === "Escape")
                onClose();
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [onClose]);
    return (_jsx("div", { className: "modal-backdrop", onClick: onClose, role: "presentation", children: _jsxs("div", { className: "modal", role: "dialog", "aria-modal": "true", "aria-label": title, onClick: (e) => e.stopPropagation(), children: [_jsxs("div", { className: "modal-head", children: [eyebrow && _jsx("span", { className: "eyebrow", children: eyebrow }), _jsx("h3", { children: title })] }), _jsx("div", { className: "modal-body", children: children }), actions && _jsx("div", { className: "modal-actions", children: actions })] }) }));
}
