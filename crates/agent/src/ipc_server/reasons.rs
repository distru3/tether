//! Block-screen reasons: `RecordBlockReason` and `BlockReasons`.
//!
//! Recording one never changes enforcement, so `auth` lets it through
//! without a PIN. It is accepted only for an app that is blocked right now
//! (that is when the block screen asks), and the agent files it under its own
//! current day rather than one a client names.

use st_core::reasons::BlockReason;

use super::*;

/// Longest range `BlockReasons` answers, in days.
const MAX_RANGE_DAYS: i64 = 366;

pub(super) fn record_block_reason(
    ctx: &Ctx,
    app_id: i64,
    reason: &str,
    now: DateTime<Utc>,
) -> Response {
    let Some(reason) = BlockReason::parse(reason) else {
        return bad_request(format!("unknown reason: {reason}"));
    };
    let day = DayKey::from_utc(
        now,
        ctx.clock.local_offset_seconds(),
        read_recover(&ctx.policy, "policy").day_start_minutes,
    );
    let db = lock_db(&ctx.db);
    match db.is_blocked(st_core::model::SubjectRef::App(app_id)) {
        Ok(true) => {}
        Ok(false) => {
            return Response::Error {
                code: ErrorCode::NotFound,
                message: "that app is not blocked".into(),
            }
        }
        Err(e) => return error_internal(format!("read block state: {e}")),
    }
    match db.record_block_reason(day, app_id, reason, now) {
        Ok(()) => accepted(now),
        Err(e) => error_internal(format!("record block reason: {e}")),
    }
}

pub(super) fn block_reasons(ctx: &Ctx, from: DayKey, to: DayKey) -> Response {
    let (Some(start), Some(end)) = (from.to_date(), to.to_date()) else {
        return bad_request("invalid day".into());
    };
    let span = (end - start).num_days();
    if !(0..MAX_RANGE_DAYS).contains(&span) {
        return bad_request(format!(
            "range must be 1 to {MAX_RANGE_DAYS} days, oldest first"
        ));
    }
    let db = lock_db(&ctx.db);
    match db.block_reason_counts(from, to) {
        Ok(rows) => Response::BlockReasons(st_ipc::BlockReasonsDto {
            counts: rows
                .into_iter()
                .map(|(reason, count)| st_ipc::BlockReasonCountDto {
                    reason: reason.as_str().to_string(),
                    count,
                })
                .collect(),
        }),
        Err(e) => error_internal(format!("block reasons: {e}")),
    }
}
