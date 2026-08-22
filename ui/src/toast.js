import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { useCallback, useState } from "react";
const TOAST_MS = 4500;
let toastSeq = 0;
/** A self-contained toast stack: render `<ToastStack />` once, use `push()`. */
export function useToasts() {
    const [toasts, setToasts] = useState([]);
    const push = useCallback((kind, message) => {
        const id = ++toastSeq;
        setToasts((t) => [...t, { id, kind, message }]);
        window.setTimeout(() => {
            setToasts((t) => t.filter((x) => x.id !== id));
        }, TOAST_MS);
    }, []);
    return { toasts, push };
}
const ICONS = {
    success: "✓",
    error: "✕",
    info: "i",
};
export function ToastStack({ toasts }) {
    return (_jsx("div", { className: "toasts", "aria-live": "polite", children: toasts.map((t) => (_jsxs("div", { className: `toast toast-${t.kind}`, role: "status", children: [_jsx("span", { className: "toast-icon", children: ICONS[t.kind] }), _jsx("span", { className: "toast-msg", children: t.message })] }, t.id))) }));
}
