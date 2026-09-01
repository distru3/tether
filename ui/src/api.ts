import { invoke } from "@tauri-apps/api/core";

import type { ErrorCode } from "./types/generated/ErrorCode";
import type { CatalogDto } from "./types/generated/CatalogDto";
import type { DaySummaryDto } from "./types/generated/DaySummaryDto";
import type { LimitTargetDto } from "./types/generated/LimitTargetDto";
import type { StatusDto } from "./types/generated/StatusDto";
import type { WeeklySummaryDto } from "./types/generated/WeeklySummaryDto";

export type { ErrorCode };

export const OVERRIDE_SECONDS = 15 * 60;

/**
 * Monday-first per-day minute overrides, mirroring `LimitDto.weekday_minutes`:
 * null on a day means "fall back to default_minutes that day".
 */
export type WeekdayMinutes = [
    number | null,
    number | null,
    number | null,
    number | null,
    number | null,
    number | null,
    number | null,
];

export function getStatus(): Promise<StatusDto> {
    return invoke("get_status");
}

export function getDaySummary(day: number): Promise<DaySummaryDto> {
    return invoke("get_day_summary", { day });
}

export function getWeeklySummary(endDay: number): Promise<WeeklySummaryDto> {
    return invoke("get_weekly_summary", { endDay });
}

export function getCatalog(): Promise<CatalogDto> {
    return invoke("get_catalog");
}

export interface PinVaultReply {
    recovery_code: string;
}

/** Set or change the PIN. The reply carries the one-time recovery code. */
export function setPin(newPin: string, currentPin: string | null): Promise<PinVaultReply> {
    return invoke("set_pin", { newPin, currentPin });
}

/** Replace a forgotten PIN with its recovery code; a fresh code comes back. */
export function recoverPin(recoveryCode: string, newPin: string): Promise<PinVaultReply> {
    return invoke("recover_pin", { recoveryCode, newPin });
}

/** Dismantle the vault; the credential may be the PIN or the recovery code. */
export function removePin(credential: string): Promise<void> {
    return invoke("remove_pin", { credential });
}

export function setSetting(key: string, value: string): Promise<void> {
    return invoke("set_setting", { key, value });
}

export function setLimit(
    target: LimitTargetDto,
    defaultMinutes: number,
    weekdayMinutes: (number | null)[],
    enabled: boolean,
    pin: string,
): Promise<string> {
    return invoke("set_limit", {
        target,
        defaultMinutes,
        weekdayMinutes,
        enabled,
        pin,
    });
}

export function deleteLimit(target: LimitTargetDto, pin: string): Promise<string> {
    return invoke("delete_limit", { target, pin });
}

export function cancelPendingLimit(target: LimitTargetDto, pin: string): Promise<string> {
    return invoke("cancel_pending_limit", { target, pin });
}

export function grantOverride(target: LimitTargetDto, seconds: number, pin: string): Promise<void> {
    return invoke("grant_override", { target, seconds, pin });
}

export function categorizeApp(appId: number, primaryCategoryId: number, tagCategoryIds: number[]): Promise<void> {
    return invoke("categorize", { appId, primary: primaryCategoryId, tags: tagCategoryIds });
}

export function listManualBlocks(): Promise<{ domains: string[] }> {
    return invoke("list_manual_blocks");
}

export function addManualBlock(domain: string): Promise<void> {
    return invoke("add_manual_block", { domain });
}

export function removeManualBlock(domain: string, pin: string): Promise<void> {
    return invoke("remove_manual_block", { domain, pin });
}

const COPY: Record<ErrorCode, string> = {
    bad_pin: "That PIN doesn't match.",
    cooldown_active: "Loosened limits take effect later — see the notice.",
    strict_mode: "Overrides are disabled in strict mode.",
    not_limitable: "That target can't carry a standing order.",
    not_found: "That entry is no longer on the books.",
    bad_request: "The request didn't parse — try again.",
    internal: "Something went wrong on our side.",
};

export function mapErrorCode(code: string, message?: string): string {
    switch (code) {
        case "bad_pin":
            return COPY.bad_pin;
        case "cooldown_active":
            return COPY.cooldown_active;
        case "strict_mode":
            return COPY.strict_mode;
        case "not_limitable":
            return COPY.not_limitable;
        case "not_found":
            return COPY.not_found;
        case "bad_request":
            return COPY.bad_request;
        case "internal":
            return COPY.internal;
        case "unreachable":
            return "Can't reach the screentime agent right now.";
        case "unexpected_response":
            return message !== undefined && message.trim().length > 0
                ? message
                : "Unexpected response from the backend.";
        default:
            return "Unexpected response from the backend.";
    }
}

function errorFields(e: unknown): { code: string; message: string } | null {
    if (typeof e !== "object" || e === null) return null;
    const record = e as Record<string, unknown>;
    if (typeof record.code !== "string") return null;
    const message = typeof record.message === "string" ? record.message : "";
    return { code: record.code, message };
}

export function describeError(e: unknown): string {
    const fields = errorFields(e);
    if (fields !== null) return mapErrorCode(fields.code, fields.message);
    if (typeof e === "string" && e.includes("unreachable")) return mapErrorCode("unreachable");
    return mapErrorCode("unexpected_response");
}
