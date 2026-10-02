//! Usage-report ingestion from the session helper.

use super::*;

/// Ingest one session-helper report.
///
/// Pipeline, in order:
///
/// 1. **Parse & sanity-check** each `observed_at_utc` against the agent clock.
///    Unparsable stamps are dropped; timestamps wildly in the future (beyond
///    [`REPORT_MAX_FUTURE_SECS`]) or stale (beyond [`REPORT_MAX_AGE_SECS`])
///    are dropped too — both would corrupt day buckets. Moderate past skew is
///    credited but logged, so chronic clock drift is visible without throwing
///    away real usage.
/// 2. **Dedupe**: identical `(app_key, observed_at)` pairs collapse, within
///    the batch and against a TTL'd ring of recently ingested pairs (the wire
///    contract requires idempotency because a lost reply forces a resend).
/// 3. **Chain**: observations are sorted ascending, then consecutive active
///    observations of the same app bridge into runs whenever their gap is at
///    most [`CHAIN_GAP_SECS`]. Each bridging step credits `[previous_end,
///    this_end]`, so credit accumulates incrementally across batches and no
///    per-batch bookkeeping is needed. An observation whose `idle_seconds`
///    reaches the configured threshold closes the app's open run instead —
///    focused-but-idle time accrues nothing, mirroring local-sampler idle
///    semantics. The first observation of a run credits nothing by itself:
///    duration needs two endpoints, which is also why helpers flush repeatedly
///    while an app stays focused rather than sending one final sample.
/// 4. **Bucket & persist**: each credited span is split at local-day
///    boundaries (midnight rollover mid-batch lands on the right days) and
///    written through the same classify-and-record path local sampling uses.
///
/// The reply is always [`Response::Accepted`] carrying the agent's ingest
/// instant unless something failed *before* anything was persisted (unknown
/// taxonomy, poisoned storage); the helper compares `effective_utc` with its
/// own send clock to measure skew.
pub(super) fn report_usage(ctx: &Ctx, report: ReportUsageDto) -> Response {
    let now = ctx.clock.now_utc();
    let tz_offset = ctx.clock.local_offset_seconds();
    let day_start = read_recover(&ctx.policy, "policy").day_start_minutes;
    let idle_threshold = read_recover(&ctx.policy, "policy")
        .idle_threshold_secs
        .max(1);

    // 1. Parse and sanity-check timestamps.
    let mut parsed: Vec<(DateTime<Utc>, ObservationDto)> =
        Vec::with_capacity(report.observations.len());
    for obs in report.observations {
        let Ok(t) = DateTime::parse_from_rfc3339(&obs.observed_at_utc) else {
            tracing::warn!(
                app = %obs.app_key,
                observed_at = %obs.observed_at_utc,
                "dropping observation with unparsable timestamp"
            );
            continue;
        };
        let t = t.with_timezone(&Utc);
        if t > now + Duration::seconds(REPORT_MAX_FUTURE_SECS) {
            tracing::warn!(
                app = %obs.app_key,
                observed_at = %t,
                now = %now,
                "dropping wildly-future observation"
            );
            continue;
        }
        if t < now - Duration::seconds(REPORT_MAX_AGE_SECS) {
            tracing::warn!(app = %obs.app_key, observed_at = %t, "dropping stale observation");
            continue;
        }
        if t < now - Duration::seconds(REPORT_SKEW_WARN_SECS) {
            tracing::warn!(
                app = %obs.app_key,
                behind_secs = (now - t).num_seconds(),
                "observation lags agent clock; crediting anyway"
            );
        }
        parsed.push((t, obs));
    }

    // 2. Dedupe within the batch and against recent batches.
    {
        let mut seen = lock_recover(&ctx.live.seen_observations, "observation dedupe ring");
        let cutoff = (now - Duration::seconds(DEDUPE_TTL_SECS)).timestamp();
        seen.retain(|(_, ts)| *ts >= cutoff);
        parsed.retain(|(t, obs)| {
            let entry = (obs.app_key.to_db_string(), t.timestamp());
            if seen.contains(&entry) {
                false
            } else {
                seen.push(entry);
                true
            }
        });
        if seen.len() > DEDUPE_CAP {
            let excess = seen.len() - DEDUPE_CAP;
            seen.drain(..excess);
        }
    }

    // 3. Order and chain into credited spans.
    parsed.sort_by_key(|(t, _)| *t);
    let mut credited: Vec<(AppKey, DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    {
        let mut chains = lock_recover(&ctx.live.open_chains, "open usage chains");
        for (t, obs) in parsed.clone() {
            let key_string = obs.app_key.to_db_string();
            ctx.live.note_focus(obs.app_key.clone(), t);
            if i64::from(obs.idle_seconds) >= idle_threshold {
                chains.remove(&key_string);
                continue;
            }
            match chains.get_mut(&key_string) {
                Some(state) if (t - state.last).num_seconds() <= CHAIN_GAP_SECS => {
                    credited.push((obs.app_key.clone(), state.last, t));
                    state.last = t;
                }
                _ => {
                    chains.insert(key_string, ChainState { last: t });
                }
            }
        }
    }

    // 4. Split across day boundaries and persist through the shared path.
    if !credited.is_empty() {
        let mut db = lock_db(&ctx.db);
        let default_category = match db.category_id(st_core::category::UNCATEGORIZED_SLUG) {
            Ok(id) => id,
            Err(e) => return error_internal(format!("uncategorized category missing: {e}")),
        };
        for (key, start, end) in credited {
            let span = PendingInterval {
                key: key.clone(),
                display_name: key.basename().to_string(),
                start,
                end,
                day: DayKey(0),
            };
            for piece in split_by_day(&span, tz_offset, day_start) {
                if let Err(e) = persist(&mut db, &piece, default_category, now) {
                    tracing::error!(
                        error = %e,
                        app = %piece.key,
                        "failed to persist reported interval"
                    );
                }
            }
        }
    }

    if let Some(ref fk) = report.focused_key {
        ctx.live.note_focus(fk.clone(), now);
    }

    ctx.live.note_report(now);

    // 6. Compute HUD overlay state for the currently focused app.
    // Always computed so the session layer can support on-demand peek shortcut
    // even when continuous floating HUD overlay is toggled off.
    let mut hud = None;
    let focused_key = report.focused_key.or_else(|| {
        parsed
            .last()
            .map(|(_, obs)| obs.app_key.clone())
            .or_else(|| ctx.live.focus_key(now))
    });
    if let Some(app_key) = focused_key {
        let db_guard = lock_db(&ctx.db);
        let today = DayKey::from_utc(now, tz_offset, day_start);
        if let Ok(snap) = db_guard.day_snapshot(today) {
            if let Ok(Some(app_id)) = db_guard.app_id_for_key(&app_key) {
                if let Ok(Some(record)) = db_guard.app_record(app_id) {
                    let all_categories = record.all_categories();
                    let limits = db_guard.load_limits().unwrap_or_default();
                    let engine = st_core::limits::LimitEngine::with_default_warnings(limits);
                    let decision = engine.evaluate(
                        record.id,
                        &all_categories,
                        true, // we assume it's blockable for the HUD check
                        today.weekday_index().unwrap_or(0),
                        &snap,
                        now,
                    );
                    let match_res = match decision {
                        st_core::limits::Decision::Allow {
                            remaining_secs,
                            binding: Some(target),
                        } => Some((remaining_secs, target)),
                        st_core::limits::Decision::Warn {
                            remaining_secs,
                            binding: target,
                            ..
                        } => Some((remaining_secs, target)),
                        _ => None,
                    };
                    if let Some((remaining_secs, target)) = match_res {
                        let is_timer = snap.active_timer_expires_utc(&target).is_some();
                        hud = Some(st_ipc::HudStateDto {
                            remaining_secs,
                            is_timer,
                        });
                    }
                }
            }
        }
    }

    Response::Accepted {
        hud,
        effective_utc: now.to_rfc3339(),
    }
}

/// Split a usage span into per-local-day pieces so midnight rollover mid-span
/// charges each calendar day for its own share (same rule the sampler applies
/// to all-night sessions).
pub(super) fn split_by_day(
    interval: &PendingInterval,
    tz_offset_secs: i32,
    day_start_minutes: i64,
) -> Vec<PendingInterval> {
    if interval.end <= interval.start {
        return Vec::new();
    }
    let start_day = DayKey::from_utc(interval.start, tz_offset_secs, day_start_minutes);
    let end_day = DayKey::from_utc(interval.end, tz_offset_secs, day_start_minutes);
    if start_day == end_day {
        return vec![PendingInterval {
            day: start_day,
            ..interval.clone()
        }];
    }
    match start_day.end_utc(tz_offset_secs, day_start_minutes) {
        Some(boundary) if boundary > interval.start && boundary < interval.end => {
            let mut head = interval.clone();
            head.day = start_day;
            head.end = boundary;
            let mut tail = interval.clone();
            tail.start = boundary;
            let mut out = vec![head];
            out.extend(split_by_day(&tail, tz_offset_secs, day_start_minutes));
            out
        }
        // An unresolvable boundary cannot be split honestly; charge the whole
        // span to the day it started in rather than dropping real usage.
        _ => vec![PendingInterval {
            day: start_day,
            ..interval.clone()
        }],
    }
}
