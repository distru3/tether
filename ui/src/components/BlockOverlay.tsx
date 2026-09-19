import { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Clock, ShieldAlert, Power, Delete, Check, Lock } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTheme } from "../hooks/useTheme";
import "./BlockOverlay.css";
import {
  getOverlayState,
  overlayExtend,
  overlayQuit,
  hideOverlayWindow,
  describeError,
} from "../api";
import type { OverlayActiveStateDto } from "../api";

export function BlockOverlay() {
  const { t, i18n } = useTranslation();
  // Ensure theme sync across windows
  useTheme();

  const [state, setState] = useState<OverlayActiveStateDto | null>(null);
  const [pin, setPin] = useState("");
  const [wrongPin, setWrongPin] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [exiting, setExiting] = useState(false);
  const exitTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Set document title and body/document transparency on mount
  useEffect(() => {
    document.title = "Tether Overlay";
    document.documentElement.style.background = "transparent";
    document.documentElement.style.backgroundColor = "transparent";
    document.body.style.background = "transparent";
    document.body.style.backgroundColor = "transparent";
    const root = document.getElementById("root");
    if (root) {
      root.style.background = "transparent";
      root.style.backgroundColor = "transparent";
    }
    return () => {
      document.documentElement.style.background = "";
      document.documentElement.style.backgroundColor = "";
      document.body.style.background = "";
      document.body.style.backgroundColor = "";
      if (root) {
        root.style.background = "";
        root.style.backgroundColor = "";
      }
    };
  }, []);

  // Sync active overlay state from host without destroying user input
  const refreshState = useCallback(async () => {
    try {
      const current = await getOverlayState();
      setState((prev) => {
        if (
          prev?.app_id === current?.app_id &&
          prev?.pin_locked === current?.pin_locked &&
          prev?.label === current?.label &&
          prev?.target_hwnd === current?.target_hwnd
        ) {
          return prev;
        }
        return current;
      });
    } catch {
      // Background query failure
    }
  }, []);

  useEffect(() => {
    refreshState();

    const clearExitTimeout = () => {
      if (exitTimeoutRef.current) {
        clearTimeout(exitTimeoutRef.current);
        exitTimeoutRef.current = null;
      }
    };

    const handleUpdate = (payload: OverlayActiveStateDto) => {
      clearExitTimeout();
      setState((prev) => {
        if (prev?.app_id !== payload.app_id) {
          setPin("");
          setWrongPin(false);
          setErrorMsg(null);
        }
        return payload;
      });
      setExiting(false);
    };

    const handleHide = () => {
      clearExitTimeout();
      setExiting(true);
      exitTimeoutRef.current = setTimeout(() => {
        setState(null);
        setPin("");
        setWrongPin(false);
        setErrorMsg(null);
        setExiting(false);
        exitTimeoutRef.current = null;
      }, 240);
    };

    // 1. Listen via global Tauri event emitter
    const unlistenUpdateGlobal = listen<OverlayActiveStateDto>("overlay_update", (event) => {
      handleUpdate(event.payload);
    });

    const unlistenHideGlobal = listen("overlay_hide", () => {
      handleHide();
    });

    const unlistenGracefulExitGlobal = listen("overlay_graceful_exit", () => {
      handleHide();
    });

    // 2. Listen via window-scoped event emitter
    let unlistenUpdateWin: Promise<() => void> | null = null;
    let unlistenHideWin: Promise<() => void> | null = null;
    let unlistenGracefulExitWin: Promise<() => void> | null = null;
    try {
      const win = getCurrentWindow();
      unlistenUpdateWin = win.listen<OverlayActiveStateDto>("overlay_update", (event) => {
        handleUpdate(event.payload);
      });
      unlistenHideWin = win.listen("overlay_hide", () => {
        handleHide();
      });
      unlistenGracefulExitWin = win.listen("overlay_graceful_exit", () => {
        handleHide();
      });
    } catch {
      // Non-Tauri fallback
    }

    // 3. Refresh state on window focus or visibility change
    const handleFocus = () => {
      refreshState();
    };
    window.addEventListener("focus", handleFocus);
    document.addEventListener("visibilitychange", handleFocus);

    return () => {
      clearExitTimeout();
      unlistenUpdateGlobal.then((f) => f());
      unlistenHideGlobal.then((f) => f());
      unlistenGracefulExitGlobal.then((f) => f());
      if (unlistenUpdateWin) unlistenUpdateWin.then((f) => f());
      if (unlistenHideWin) unlistenHideWin.then((f) => f());
      if (unlistenGracefulExitWin) unlistenGracefulExitWin.then((f) => f());
      window.removeEventListener("focus", handleFocus);
      document.removeEventListener("visibilitychange", handleFocus);
    };
  }, [refreshState]);

  // If state is not yet loaded on initial mount, poll briefly until populated
  useEffect(() => {
    if (state !== null) return;
    const initialPoll = setInterval(() => {
      refreshState();
    }, 400);
    return () => clearInterval(initialPoll);
  }, [state, refreshState]);

  const handleExtend = useCallback(async () => {
    if (!state || submitting) return;
    if (state.pin_locked && pin.length === 0) return;

    setSubmitting(true);
    setErrorMsg(null);
    try {
      const ok = await overlayExtend(state.app_id, pin);
      if (ok) {
        setExiting(true);
      }
    } catch (e) {
      setWrongPin(true);
      setPin("");
      setErrorMsg(describeError(e));
    } finally {
      setSubmitting(false);
    }
  }, [state, pin, submitting]);

  const handleQuit = useCallback(async () => {
    if (!state || submitting) return;
    setSubmitting(true);
    setExiting(true);
    try {
      await overlayQuit(state.app_id, state.process_id, state.target_hwnd);
    } catch {
      await hideOverlayWindow();
    } finally {
      setSubmitting(false);
    }
  }, [state, submitting]);

  // Physical keyboard listener
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key >= "0" && e.key <= "9") {
        e.preventDefault();
        setPin((prev) => (prev.length < 8 ? prev + e.key : prev));
        setWrongPin(false);
        setErrorMsg(null);
      } else if (e.key === "Backspace") {
        e.preventDefault();
        setPin((prev) => prev.slice(0, -1));
        setWrongPin(false);
        setErrorMsg(null);
      } else if (e.key === "Enter") {
        e.preventDefault();
        handleExtend();
      } else if (e.key === "Escape") {
        e.preventDefault();
        handleQuit();
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [handleExtend, handleQuit]);

  const handleKeypadPress = (val: string) => {
    if (val === "C") {
      setPin("");
      setWrongPin(false);
      setErrorMsg(null);
    } else if (val === "OK") {
      handleExtend();
    } else {
      if (pin.length < 8) {
        setPin((prev) => prev + val);
        setWrongPin(false);
        setErrorMsg(null);
      }
    }
  };

  if (!state) {
    return null;
  }

  const isRtl = i18n.dir() === "rtl";

  return (
    <div
      id="tether-block-overlay"
      dir={isRtl ? "rtl" : "ltr"}
      className={`tether-overlay-backdrop ${exiting ? "tether-overlay-backdrop--exiting" : ""}`}
    >
      <div
        className={`tether-overlay-card solid-card ${exiting ? "tether-overlay-card--exiting" : ""}`}
      >
        {/* Top Header Row */}
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <div
              style={{
                width: "28px",
                height: "28px",
                borderRadius: "6px",
                background: "var(--color-primary)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "#fff",
              }}
            >
              <Lock size={15} />
            </div>
            <span
              style={{
                fontSize: "12px",
                fontWeight: 700,
                letterSpacing: "0.1em",
                textTransform: "uppercase",
                color: "var(--text-muted)",
                fontFamily: "var(--font-mono, monospace)",
              }}
            >
              Tether
            </span>
          </div>

          <div
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: "6px",
              padding: "4px 10px",
              borderRadius: "20px",
              background: "var(--bg-danger)",
              border: "1px solid var(--border-danger)",
              color: "var(--color-danger)",
              fontSize: "11px",
              fontWeight: 700,
              letterSpacing: "0.06em",
              textTransform: "uppercase",
            }}
          >
            <ShieldAlert size={13} />
            <span>{t("overlay.limitReached")}</span>
          </div>
        </div>

        {/* Blocked App Info */}
        <div style={{ textAlign: "center", marginTop: "4px" }}>
          <h2
            style={{
              fontSize: "24px",
              fontWeight: 700,
              color: "var(--text-primary)",
              margin: "0 0 8px",
              letterSpacing: "-0.01em",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              gap: "10px",
            }}
          >
            <span
              style={{
                width: "10px",
                height: "10px",
                borderRadius: "50%",
                background: "var(--color-danger)",
                display: "inline-block",
                flexShrink: 0,
              }}
            />
            {state?.label || t("overlay.headline")}
          </h2>
          <p
            style={{
              fontSize: "13.5px",
              color: "var(--text-secondary)",
              margin: 0,
              lineHeight: 1.5,
            }}
          >
            {t("overlay.reasonLimit")}
          </p>
        </div>

        {/* PIN Entry Area (if PIN is configured) */}
        {state?.pin_locked && (
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              gap: "14px",
              marginTop: "4px",
            }}
          >
            <span
              style={{
                fontSize: "12px",
                fontWeight: 600,
                color: "var(--text-muted)",
                textTransform: "uppercase",
                letterSpacing: "0.05em",
              }}
            >
              {t("overlay.enterPin")}
            </span>

            {/* Masked Dots */}
            <div
              style={{
                display: "flex",
                gap: "12px",
                padding: "10px 20px",
                borderRadius: "10px",
                background: "var(--bg-recessed)",
                border: wrongPin
                  ? "1px solid var(--color-danger)"
                  : "1px solid var(--border-subtle)",
                transition: "border-color 0.2s ease",
              }}
            >
              {[0, 1, 2, 3].map((idx) => {
                const filled = idx < pin.length;
                return (
                  <div
                    key={idx}
                    style={{
                      width: "14px",
                      height: "14px",
                      borderRadius: "50%",
                      border: filled
                        ? "2px solid var(--color-primary)"
                        : "2px solid var(--border-strong)",
                      background: filled ? "var(--color-primary)" : "transparent",
                      boxShadow: filled ? "0 0 8px var(--color-primary-subtle)" : "none",
                      transition: "all 0.15s ease",
                    }}
                  />
                );
              })}
            </div>

            {/* Error message */}
            {wrongPin && (
              <span
                style={{
                  fontSize: "12px",
                  color: "var(--color-danger)",
                  fontWeight: 600,
                }}
              >
                {errorMsg || t("overlay.incorrectPin")}
              </span>
            )}

            {/* 3x4 Tactile Keypad */}
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(3, 1fr)",
                gap: "8px",
                width: "100%",
                maxWidth: "280px",
                marginTop: "4px",
              }}
            >
              {["1", "2", "3", "4", "5", "6", "7", "8", "9", "C", "0", "OK"].map((key) => {
                const isAction = key === "C" || key === "OK";
                return (
                  <button
                    key={key}
                    type="button"
                    onClick={() => handleKeypadPress(key)}
                    style={{
                      height: "44px",
                      borderRadius: "8px",
                      border: "1px solid var(--border-card)",
                      background: isAction
                        ? key === "OK"
                          ? "var(--color-primary)"
                          : "var(--bg-danger)"
                        : "var(--bg-recessed)",
                      color: isAction
                        ? key === "OK"
                          ? "#FFFFFF"
                          : "var(--color-danger)"
                        : "var(--text-primary)",
                      fontSize: isAction ? "13px" : "18px",
                      fontWeight: 700,
                      fontFamily: isAction ? "inherit" : "var(--font-mono, monospace)",
                      cursor: "pointer",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      transition: "all 0.12s ease",
                    }}
                    onMouseEnter={(e) => {
                      e.currentTarget.style.borderColor = "var(--border-hover)";
                      e.currentTarget.style.background = isAction
                        ? key === "OK"
                          ? "var(--color-primary-hover)"
                          : "var(--bg-danger)"
                        : "var(--bg-surface-hover)";
                    }}
                    onMouseLeave={(e) => {
                      e.currentTarget.style.borderColor = "var(--border-card)";
                      e.currentTarget.style.background = isAction
                        ? key === "OK"
                          ? "var(--color-primary)"
                          : "var(--bg-danger)"
                        : "var(--bg-recessed)";
                    }}
                    onMouseDown={(e) => {
                      e.currentTarget.style.transform = "scale(0.96)";
                    }}
                    onMouseUp={(e) => {
                      e.currentTarget.style.transform = "scale(1)";
                    }}
                  >
                    {key === "C" ? <Delete size={18} /> : key === "OK" ? <Check size={18} /> : key}
                  </button>
                );
              })}
            </div>

            <span
              style={{
                fontSize: "11px",
                color: "var(--text-muted)",
                marginTop: "2px",
              }}
            >
              {t("overlay.keyboardHint")}
            </span>
          </div>
        )}

        {/* Action Buttons */}
        <div style={{ display: "flex", gap: "12px", marginTop: "8px" }}>
          <button
            type="button"
            className="btn btn-primary"
            onClick={handleExtend}
            disabled={submitting || (state?.pin_locked && pin.length === 0)}
            style={{
              flex: 1,
              height: "46px",
              borderRadius: "10px",
              fontSize: "14px",
              fontWeight: 600,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              gap: "8px",
            }}
          >
            <Clock size={16} />
            <span>{t("overlay.extend15")}</span>
          </button>

          <button
            type="button"
            className="btn btn-secondary"
            onClick={handleQuit}
            disabled={submitting}
            style={{
              flex: 1,
              height: "46px",
              borderRadius: "10px",
              fontSize: "14px",
              fontWeight: 600,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              gap: "8px",
              borderColor: "var(--border-danger)",
              color: "var(--color-danger)",
            }}
          >
            <Power size={16} />
            <span>{t("overlay.quitApp")}</span>
          </button>
        </div>
      </div>
    </div>
  );
}
