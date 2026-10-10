//! Read-only queries: status, day/week summaries, catalog, blocked apps.

use super::*;

pub(super) fn status_response(ctx: &Ctx) -> Response {
    let (pin_configured, family_dns_enabled) = {
        let db = lock_db(&ctx.db);
        let pin = db.pin_hash().ok().flatten().is_some();
        let family_dns = db.setting("family_dns_enabled").ok().flatten().as_deref() == Some("true");
        (pin, family_dns)
    };
    // Truthful tracking: reports arriving recently mean the session helper is
    // alive; otherwise only the legacy self-sampling fallback counts.
    let tracking_available = ctx.status.self_sampling
        || ctx
            .live
            .reported_within(RECENT_REPORT_SECS, ctx.clock.now_utc());

    let policy = read_recover(&ctx.policy, "policy");
    Response::Status(StatusDto {
        agent_version: ctx.status.agent_version.clone(),
        tracker_backend: ctx.status.tracker_backend.clone(),
        enforcement_backend: ctx.status.enforcement_backend.clone(),
        filter_backend: ctx.status.filter_backend.clone(),
        tracking_available,
        blocks_encrypted_dns: ctx.status.blocks_encrypted_dns,
        strict_mode: policy.strict_mode,
        pin_configured,
        show_hud_overlay: policy.show_hud_overlay,
        show_hud_in_fullscreen: policy.show_hud_in_fullscreen,
        hud_peek_hotkey: policy.hud_peek_hotkey.clone(),
        alert_volume: policy.alert_volume,
        limit_cooldown_hours: policy.limit_cooldown_hours,
        day_start_minutes: policy.day_start_minutes,
        idle_threshold_secs: policy.idle_threshold_secs,
        path_level: false,
        wildcard_domains: false,
        family_dns_enabled,
        profile: policy.profile.clone(),
    })
}

pub(super) fn day_summary(ctx: &Ctx, day: DayKey) -> Response {
    let db = lock_db(&ctx.db);
    let summary = match db.day_summary(day) {
        Ok(summary) => summary,
        Err(e) => {
            return error_internal(format!("dashboard query failed: {e}"));
        }
    };

    // Which apps are currently blocked, for honest blocked flags.
    let blocked_apps: std::collections::HashSet<i64> = db
        .blocked_subjects()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|(subject, _)| match subject {
            SubjectRef::App(id) => Some(id),
            SubjectRef::Site(_) => None,
        })
        .collect();

    let now = ctx.clock.now_utc();
    let snapshot = db.day_snapshot(summary.day).ok();

    // The row's budget, defined so that `limit_seconds - seconds` is exactly
    // the time left on that limit today: the weekday-resolved allowance minus
    // what the engine counts against it (for a category, that includes
    // tagged apps, which the row's primary-only `seconds` does not). Disabled
    // limits have no budget. Previously this used `default_minutes` for app
    // rows only, ignoring weekday overrides and the enabled flag, and never
    // filled category rows.
    let limits = db.load_limits().unwrap_or_default();
    let weekday = summary.day.weekday_index().unwrap_or(0);
    let limit_secs_for = |target: LimitTarget, row_seconds: i64| -> Option<i64> {
        let limit = limits.iter().find(|l| l.enabled && l.target == target)?;
        let used = snapshot
            .as_ref()
            .map_or(row_seconds, |s| s.seconds_used(&target));
        Some(row_seconds + limit.allowed_secs_for_weekday(weekday) - used)
    };
    let timer_expires = |target: LimitTarget| -> Option<String> {
        snapshot
            .as_ref()
            .and_then(|s| s.active_timer_expires_utc(&target))
            .filter(|t| *t > now)
            .map(|t| t.to_rfc3339())
    };

    Response::DaySummary(st_ipc::DaySummaryDto {
        day: summary.day,
        total_seconds: summary.total_seconds,
        apps: summary
            .apps
            .into_iter()
            .map(|a| UsageRowDto {
                id: a.id,
                label: a.label,
                seconds: a.seconds,
                color: Some(a.category_color),
                limit_seconds: limit_secs_for(LimitTarget::App(a.id), a.seconds),
                blocked: blocked_apps.contains(&a.id),
                timer_expires_utc: timer_expires(LimitTarget::App(a.id)),
            })
            .collect(),
        categories: summary
            .categories
            .into_iter()
            .map(|c| UsageRowDto {
                id: c.id,
                label: c.name,
                seconds: c.seconds,
                color: Some(c.color),
                limit_seconds: limit_secs_for(LimitTarget::Category(c.id), c.seconds),
                blocked: false,
                timer_expires_utc: timer_expires(LimitTarget::Category(c.id)),
            })
            .collect(),
        intervals: summary
            .intervals
            .into_iter()
            .map(|i| st_ipc::IntervalDto {
                start_utc: i.start_utc,
                duration_seconds: i.duration_seconds,
                app_id: i.app_id,
            })
            .collect(),
    })
}

/// Seven-day trend plus the previous week's total. The storage layer already
/// guarantees the shape the wire promises (seven zero-filled days, oldest
/// first), so this handler is pure mapping.
pub(super) fn weekly_summary(ctx: &Ctx, end_day: DayKey) -> Response {
    let db = lock_db(&ctx.db);
    let summary = match db.weekly_summary(end_day) {
        Ok(summary) => summary,
        Err(e) => return error_internal(format!("weekly summary query failed: {e}")),
    };

    Response::WeeklySummary(st_ipc::WeeklySummaryDto {
        days: summary
            .days
            .into_iter()
            .map(|d| st_ipc::DailyTotalDto {
                day: d.day,
                total_seconds: d.total_seconds,
            })
            .collect(),
        previous_week_total: summary.previous_week_total,
    })
}

pub(super) fn catalog(ctx: &Ctx) -> Response {
    let db = lock_db(&ctx.db);
    let now = ctx.clock.now_utc();
    let day = DayKey::from_utc(
        now,
        ctx.clock.local_offset_seconds(),
        read_recover(&ctx.policy, "policy").day_start_minutes,
    );
    let snapshot = db.day_snapshot(day).ok();
    let (apps, categories, limits, pending_limits) = match (
        db.list_apps(),
        db.list_categories(),
        db.list_limit_rows(),
        db.list_pending_limits(),
    ) {
        (Ok(apps), Ok(categories), Ok(limits), Ok(pending)) => (apps, categories, limits, pending),
        (Err(e), _, _, _) => return error_internal(format!("list apps: {e}")),
        (_, Err(e), _, _) => return error_internal(format!("list categories: {e}")),
        (_, _, Err(e), _) => return error_internal(format!("list limits: {e}")),
        (_, _, _, Err(e)) => return error_internal(format!("list pending limits: {e}")),
    };

    Response::Catalog(st_ipc::CatalogDto {
        apps: apps
            .into_iter()
            .map(|a| st_ipc::AppDto {
                id: a.id,
                key: a.key.to_db_string(),
                display_name: a.display_name,
                primary_category: a.primary_category,
                tags: a.tags,
                user_classified: a.user_classified,
            })
            .collect(),
        categories: categories
            .into_iter()
            .map(|c| st_ipc::CategoryDto {
                id: c.id,
                slug: c.slug,
                name: c.name,
                kind: match c.kind {
                    CategoryKind::Limitable => "limitable",
                    CategoryKind::BlockOnly => "block_only",
                    CategoryKind::NeverBlock => "never_block",
                }
                .to_string(),
                color: c.color,
                builtin: c.builtin,
            })
            .collect(),
        limits: limits
            .iter()
            .filter_map(|l| limit_to_dto(l, snapshot.as_ref(), now))
            .collect(),
        pending_limits: pending_limits
            .iter()
            .filter_map(pending_limit_to_dto)
            .collect(),
    })
}

/// Currently-blocked apps, for the overlay owner. Joins `block_state` with the
/// app rows to hand back a label and key the session helper can match.
pub(super) fn blocked_apps(ctx: &Ctx) -> Response {
    let db = lock_db(&ctx.db);
    let blocked = match db.blocked_subjects() {
        Ok(b) => b,
        Err(e) => return error_internal(format!("list blocks: {e}")),
    };
    let mut out = Vec::new();
    for (subject, _reason) in blocked {
        let SubjectRef::App(app_id) = subject else {
            continue;
        };
        let Some(record) = db.app_record(app_id).ok().flatten() else {
            continue;
        };
        out.push(st_ipc::BlockedAppDto {
            app_id,
            label: record.display_name,
            app_key: record.key.to_db_string(),
        });
    }
    Response::BlockedApps(st_ipc::BlockedAppsDto { blocked: out })
}

pub(super) fn limit_to_dto(
    row: &LimitRow,
    snapshot: Option<&st_storage::DaySnapshot>,
    now: DateTime<Utc>,
) -> Option<st_ipc::LimitDto> {
    let limit = row.to_limit()?;
    let target = match limit.target {
        LimitTarget::App(id) => LimitTargetDto::App { id },
        LimitTarget::Category(id) => LimitTargetDto::Category { id },
        LimitTarget::Total => LimitTargetDto::Total,
    };
    let timer_expires_utc = snapshot
        .and_then(|s| s.active_timer_expires_utc(&limit.target))
        .filter(|t| *t > now)
        .map(|t| t.to_rfc3339());
    Some(st_ipc::LimitDto {
        id: limit.id,
        target,
        default_minutes: limit.default_minutes,
        weekday_minutes: limit.weekday_minutes,
        enabled: limit.enabled,
        timer_expires_utc,
    })
}

pub(super) fn pending_limit_to_dto(
    row: &st_storage::PendingLimitRow,
) -> Option<st_ipc::PendingLimitDto> {
    let target = match row.target {
        Some(LimitTarget::App(id)) => LimitTargetDto::App { id },
        Some(LimitTarget::Category(id)) => LimitTargetDto::Category { id },
        Some(LimitTarget::Total) => LimitTargetDto::Total,
        None => return None,
    };
    Some(st_ipc::PendingLimitDto {
        id: row.id,
        target,
        action: row.action.clone(),
        default_minutes: row.default_minutes.map(|v| v as u32),
        weekday_minutes: row.weekday_minutes,
        enabled: row.enabled,
        effective_from_utc: row.effective_from_utc.clone(),
    })
}
