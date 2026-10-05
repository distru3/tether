import { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Check, Delete, Moon } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTheme } from "../hooks/useTheme";
import { useNowMinute } from "../hooks/useNowMinute";
import "./BlockOverlay.css";
import {
  getCatalog,
  getDaySummary,
  getOverlayState,
  getStatus,
  listAllowlist,
  listSchedules,
  overlayExtend,
  overlayQuit,
  recordBlockReason,
  describeError,
} from "../api";
import type { OverlayActiveStateDto } from "../api";
import { blockReason, type BlockReason } from "../blockModel";
import { formatDuration, setDayStartMinutes, targetLabel, todayKey } from "../format";
import { clockLabel, limitRule } from "../limitText";
import { dayWindowStart, nowPosition } from "../todayModel";
import type { CatalogDto } from "../types/generated/CatalogDto";
import type { DaySummaryDto } from "../types/generated/DaySummaryDto";
import type { ScheduleDto } from "../types/generated/ScheduleDto";
import type { StatusDto } from "../types/generated/StatusDto";

interface BlockContext {
  status: StatusDto | null;
  catalog: CatalogDto | null;
  summary: DaySummaryDto | null;
  schedules: ScheduleDto[];
  allowlisted: number[];
}

const EMPTY_CONTEXT: BlockContext = { status: null, catalog: null, summary: null, schedules: [], allowlisted: [] };

/** What the block screen says about the block: loaded fresh for each app. */
function useBlockContext(appId: number | null): BlockContext {
  const [ctx, setCtx] = useState<BlockContext>(EMPTY_CONTEXT);
  useEffect(() => {
    if (appId === null) return;
    let live = true;
    (async () => {
      // Each source is optional: the screen still works with what loads.
      const status = await getStatus().catch(() => null);
      if (status) setDayStartMinutes(status.day_start_minutes);
      const [catalog, summary, schedules, allow] = await Promise.all([
        getCatalog().catch(() => null),
        getDaySummary(todayKey()).catch(() => null),
        listSchedules().catch(() => null),
        listAllowlist().catch(() => null),
      ]);
      if (!live) return;
      setCtx({
        status,
        catalog,
        summary,
        schedules: schedules?.schedules ?? [],
        allowlisted: (allow?.items ?? []).filter((i) => i.subject_type === "app").map((i) => i.subject_id),
      });
    })();
    return () => {
      live = false;
    };
  }, [appId]);
  return ctx;
}

function timeOf(d: Date): string {
  return clockLabel(d.getHours() * 60 + d.getMinutes());
}

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
    setErrorMsg(null);
    try {
      // The host hides the overlay only once the app is actually closed.
      await overlayQuit(state.app_id, state.process_id, state.target_hwnd);
      setExiting(true);
    } catch (e) {
      // Never uncover a blocked app because closing it failed.
      setErrorMsg(t("overlay.quitFailed", { reason: describeError(e) }));
    } finally {
      setSubmitting(false);
    }
  }, [state, submitting, t]);

  const now = useNowMinute();
  const ctx = useBlockContext(state?.app_id ?? null);
  const [keypadOpen, setKeypadOpen] = useState(false);
  const [answered, setAnswered] = useState<string | null>(null);

  // A new app on the screen starts with the keypad closed and no answer.
  useEffect(() => {
    setKeypadOpen(false);
    setAnswered(null);
  }, [state?.app_id]);

  // The answer is optional and only feeds Activity, so a failure to save it
  // is not worth interrupting anyone for.
  const answer = (value: string) => {
    if (!state) return;
    setAnswered(value);
    recordBlockReason(state.app_id, value).catch(() => {});
  };

  // Keyboard: digits, Backspace and Enter work the PIN pad once it is open;
  // Escape only closes the pad. (Escape used to quit the blocked app, which
  // a reflex press in a game would do by accident.)
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (!keypadOpen) return;
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
        setKeypadOpen(false);
        setPin("");
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [handleExtend, keypadOpen]);

  const handleKeypadPress = (val: string) => {
    if (val === "C") {
      setPin("");
      setWrongPin(false);
      setErrorMsg(null);
    } else if (val === "OK") {
      handleExtend();
    } else if (pin.length < 8) {
      setPin((prev) => prev + val);
      setWrongPin(false);
      setErrorMsg(null);
    }
  };

  if (!state) {
    return null;
  }

  const isRtl = i18n.dir() === "rtl";
  const dayKey = todayKey(now);
  const dayStart = ctx.status?.day_start_minutes ?? 0;
  const reason: BlockReason = blockReason({
    appId: state.app_id,
    catalog: ctx.catalog,
    summary: ctx.summary,
    schedules: ctx.schedules,
    allowlisted: ctx.allowlisted,
    dayKey,
    dayStartMinutes: dayStart,
    now,
  });
  const app = state.label || t("overlay.thisApp");
  const guardian = ctx.status?.profile === "guardian";
  const inSeconds = Math.max(60, Math.round((reason.until.getTime() - now.getTime()) / 1000));
  const back = t("overlay.backAt", { time: timeOf(reason.until), in: formatDuration(inSeconds - (inSeconds % 60)) });

  let eyebrow: string | null = null;
  let title: string;
  switch (reason.kind) {
    case "downtime":
      eyebrow = t("overlay.scheduleEyebrow", { from: timeOf(reason.band.start), to: timeOf(reason.band.end) });
      title = t("overlay.titleDowntime", { name: reason.band.schedule.name });
      break;
    case "budget": {
      const name = targetLabel(reason.limit.target, ctx.catalog);
      eyebrow = t("overlay.budgetEyebrow", { name, rule: limitRule(reason.limit) });
      title = t("overlay.titleBudget", { name });
      break;
    }
    case "total":
      eyebrow = t("overlay.totalEyebrow", { rule: limitRule(reason.limit) });
      title = t("overlay.titleTotal");
      break;
    default:
      title = t("overlay.titleUnknown", { app });
  }

  // Strip: the agent's day, locked from now until the app comes back.
  const winStart = dayWindowStart(dayKey, dayStart).getTime();
  const nowAt = nowPosition(dayKey, dayStart, now) ?? 0;
  const untilAt = Math.min(1, Math.max(nowAt, (reason.until.getTime() - winStart) / 86_400_000));
  // Extra time does not lift a schedule (the agent re-blocks), so it is not offered then.
  const canAddTime = reason.kind !== "downtime" && !(ctx.status?.strict_mode ?? false);

  return (
    <div
      id="tether-block-overlay"
      role="main"
      dir={isRtl ? "rtl" : "ltr"}
      data-theme="dark"
      data-theme-mode="dark"
      className={`tether-overlay-backdrop ${exiting ? "tether-overlay-backdrop--exiting" : ""}`}
    >
      <div className={`tt-block ${exiting ? "tether-overlay-card--exiting" : ""}`}>
        <div className={`tt-block-badge${reason.kind === "budget" ? ` tt-hue-${reason.hue}` : ""}`} aria-hidden="true">
          <span>{reason.kind === "downtime" ? <Moon size={28} /> : formatDuration(0)}</span>
        </div>

        <div className="tt-block-head">
          {eyebrow && <span className="tt-block-eyebrow">{eyebrow}</span>}
          <h1 className="tt-block-title">{title}</h1>
          <p className="tt-block-body">{t("overlay.waiting", { app })}</p>
        </div>

        <div className="tt-block-strip-wrap" role="img" aria-label={t("overlay.stripAria", { now: timeOf(now), back })}>
          <div className="tt-block-strip">
            <span className="tt-block-strip-past" style={{ width: `${nowAt * 100}%` }} />
            <span className="tt-block-strip-locked" style={{ insetInlineStart: `${nowAt * 100}%`, width: `${(untilAt - nowAt) * 100}%` }} />
            <span className="tt-block-strip-now" style={{ insetInlineStart: `${nowAt * 100}%` }} />
          </div>
          <div className="tt-block-strip-scale" aria-hidden="true">
            <span>{timeOf(new Date(winStart))}</span>
            <span className="tt-block-strip-back">{back}</span>
          </div>
        </div>

        <fieldset className="tt-block-reasons">
          <legend>{t("overlay.reasonQuestion")}</legend>
          <div>
            {(["finish", "bored", "habit"] as const).map((r) => (
              <button
                key={r}
                type="button"
                aria-pressed={answered === r}
                onClick={() => answer(r)}
              >
                {t(`overlay.reason.${r}`)}
              </button>
            ))}
          </div>
        </fieldset>

        {errorMsg && !wrongPin && (
          <p role="alert" className="tt-block-error">
            {errorMsg}
          </p>
        )}

        <div className="tt-block-actions">
          <button type="button" className="tt-block-primary" onClick={handleQuit} disabled={submitting}>
            {t("overlay.closeApp", { app })}
          </button>
          {canAddTime && !state.pin_locked && (
            <button type="button" className="tt-block-secondary" onClick={handleExtend} disabled={submitting}>
              {t("overlay.extend15")}
            </button>
          )}
        </div>

        {canAddTime && state.pin_locked && !keypadOpen && (
          <button type="button" className="tt-block-link" onClick={() => setKeypadOpen(true)}>
            {guardian ? t("overlay.pinLinkGuardian") : t("overlay.pinLinkSelf")}
          </button>
        )}

        {canAddTime && state.pin_locked && keypadOpen && (
          <div className="tt-block-pin">
            <span className="tt-block-pin-label" id="tt-block-pin-label">{t("overlay.enterPin")}</span>
            <div className={`tt-block-dots${wrongPin ? " tt-block-dots--wrong" : ""}`} aria-hidden="true">
              {Array.from({ length: Math.max(4, pin.length) }, (_, idx) => (
                <span key={idx} className={idx < pin.length ? "tt-block-dot tt-block-dot--on" : "tt-block-dot"} />
              ))}
            </div>
            {wrongPin && (
              <span role="alert" className="tt-block-error">
                {errorMsg || t("overlay.incorrectPin")}
              </span>
            )}
            <div className="tt-block-keypad" role="group" aria-labelledby="tt-block-pin-label">
              {["1", "2", "3", "4", "5", "6", "7", "8", "9", "C", "0", "OK"].map((key) => (
                <button
                  key={key}
                  type="button"
                  onClick={() => handleKeypadPress(key)}
                  className={key === "OK" ? "tt-block-key tt-block-key--ok" : key === "C" ? "tt-block-key tt-block-key--clear" : "tt-block-key"}
                  aria-label={key === "C" ? t("overlay.clearPin") : key === "OK" ? t("overlay.submitPin") : undefined}
                  disabled={key === "OK" && (submitting || pin.length === 0)}
                >
                  {key === "C" ? <Delete size={18} aria-hidden="true" /> : key === "OK" ? <Check size={18} aria-hidden="true" /> : key}
                </button>
              ))}
            </div>
            <span className="tt-block-hint">{t("overlay.keyboardHint")}</span>
          </div>
        )}

        {reason.kind === "downtime" && (
          <p className="tt-block-note">{guardian ? t("overlay.downtimeNoteGuardian") : t("overlay.downtimeNoteSelf")}</p>
        )}
      </div>
    </div>
  );
}
