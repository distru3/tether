//! Limits and overrides, including the anti-impulse cooldown.

use super::*;

/// A limit edit as sent by the UI, grouped so the handler stays readable.
#[derive(Debug, Clone)]
pub(super) struct LimitSpec {
    pub(super) target: LimitTargetDto,
    pub(super) default_minutes: u32,
    pub(super) weekday_minutes: [Option<u32>; 7],
    pub(super) enabled: bool,
}

pub(super) fn set_limit(ctx: &Ctx, spec: LimitSpec, now: DateTime<Utc>) -> Response {
    let db = lock_db(&ctx.db);
    let Some(target) = dto_to_target(&spec.target) else {
        return bad_request("unknown limit target".into());
    };
    if let Err(code) = validate_target(&db, &target) {
        return Response::Error {
            code,
            message: "target cannot carry a time limit".into(),
        };
    }

    // A failed read must never masquerade as "no existing limit": that would
    // route a loosening edit down the apply-immediately path and defeat the
    // anti-impulse cooldown precisely when the database is misbehaving.
    let existing = match db.load_limits() {
        Ok(limits) => limits.into_iter().find(|l| l.target == target),
        Err(e) => return error_internal(format!("load limits: {e}")),
    };
    let effective = match existing {
        // Tightening, a brand-new limit, and switching a limit OFF apply
        // immediately. Disabling is deliberately an *instant* escape hatch:
        // the anti-impulse cooldown exists to stop future-you from granting
        // itself more minutes, not from standing an order down entirely.
        Some(existing)
            if !spec.enabled
                || !is_loosening(&existing, spec.default_minutes, spec.weekday_minutes) =>
        {
            now
        }
        // Only loosening *minutes* while staying enabled waits out the cooldown.
        Some(_) => {
            now + Duration::hours(
                read_recover(&ctx.policy, "policy")
                    .limit_cooldown_hours
                    .max(0),
            )
        }
        None => now,
    };

    let result = if effective > now {
        db.queue_pending_update(
            &target,
            spec.default_minutes,
            spec.weekday_minutes,
            spec.enabled,
            effective,
        )
    } else {
        db.upsert_limit(
            &st_core::limits::Limit {
                id: 0,
                target,
                default_minutes: spec.default_minutes,
                weekday_minutes: spec.weekday_minutes,
                enabled: spec.enabled,
            },
            now,
        )
    };
    match result {
        Ok(()) => Response::Accepted {
            hud: None,
            effective_utc: effective.to_rfc3339(),
        },
        Err(e) => error_internal(format!("set limit: {e}")),
    }
}

pub(super) fn delete_limit(ctx: &Ctx, target: LimitTargetDto, now: DateTime<Utc>) -> Response {
    let db = lock_db(&ctx.db);
    let Some(target) = dto_to_target(&target) else {
        return bad_request("unknown limit target".into());
    };
    // Deleting lifts enforcement instantly (owner decision, 2026-08): the
    // cooldown protects against loosening minutes on impulse, not against
    // standing an order down. The enforcer's next evaluation tick thaws
    // whatever the deleted order had frozen.
    match db.delete_limit_by_target(&target) {
        Ok(()) => Response::Accepted {
            hud: None,
            effective_utc: now.to_rfc3339(),
        },
        Err(e) => error_internal(format!("delete limit: {e}")),
    }
}

pub(super) fn cancel_pending_limit(ctx: &Ctx, target: LimitTargetDto) -> Response {
    let db = lock_db(&ctx.db);
    let Some(target) = dto_to_target(&target) else {
        return bad_request("unknown limit target".into());
    };
    match db.cancel_pending_limit(&target) {
        Ok(()) => {
            // we just use the current time from clock
            Response::Accepted {
                hud: None,
                effective_utc: ctx.clock.now_utc().to_rfc3339(),
            }
        }
        Err(e) => error_internal(format!("cancel pending limit: {e}")),
    }
}

pub(super) fn grant_override(
    ctx: &Ctx,
    target: LimitTargetDto,
    seconds: i64,
    now: DateTime<Utc>,
) -> Response {
    // Strict mode and the PIN are checked by `auth` before dispatch.
    let db = lock_db(&ctx.db);
    let Some(target) = dto_to_target(&target) else {
        return bad_request("unknown limit target".into());
    };
    let seconds = seconds.clamp(1, 24 * 3600);
    // Attribute the bonus to the user's local day — the same computation the
    // sampler/enforcer use — not to the UTC calendar day. Hard-coding offset
    // zero put evening overrides west of UTC onto tomorrow's budget.
    let day = DayKey::from_utc(
        now,
        ctx.clock.local_offset_seconds(),
        read_recover(&ctx.policy, "policy").day_start_minutes,
    );
    match db.grant_override(&target, day, seconds, now, Some("user override")) {
        Ok(()) => accepted(now),
        Err(e) => error_internal(format!("grant override: {e}")),
    }
}

/// A limit is "loosening" if any time dimension went up. Disabling is NOT
/// counted here: it is handled one branch above as an instant action, by
/// design (the cooldown guards minute-loosening, not standing a order down).
pub(super) fn is_loosening(
    existing: &st_core::limits::Limit,
    default_minutes: u32,
    weekday_minutes: [Option<u32>; 7],
) -> bool {
    if default_minutes > existing.default_minutes {
        return true;
    }
    weekday_minutes
        .iter()
        .zip(existing.weekday_minutes.iter())
        .any(|(new, old)| {
            let new_effective = new.unwrap_or(default_minutes);
            let old_effective = old.unwrap_or(existing.default_minutes);
            new_effective > old_effective
        })
}

pub(super) fn validate_target(db: &Db, target: &LimitTarget) -> Result<(), ErrorCode> {
    match target {
        LimitTarget::Total => Ok(()),
        LimitTarget::App(_) => Ok(()),
        LimitTarget::Category(id) => {
            let kind = db.category_kind(*id).ok().flatten();
            match kind {
                Some(CategoryKind::Limitable) => Ok(()),
                Some(CategoryKind::BlockOnly) | Some(CategoryKind::NeverBlock) | None => {
                    Err(ErrorCode::NotLimitable)
                }
            }
        }
    }
}

pub(super) fn dto_to_target(dto: &LimitTargetDto) -> Option<LimitTarget> {
    match dto {
        LimitTargetDto::App { id } => Some(LimitTarget::App(*id)),
        LimitTargetDto::Category { id } => Some(LimitTarget::Category(*id)),
        LimitTargetDto::Total => Some(LimitTarget::Total),
    }
}
