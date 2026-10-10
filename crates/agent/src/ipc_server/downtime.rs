//! Downtime schedules and the always-allowed list.

use super::*;

pub(super) fn schedule_to_dto(s: &st_core::schedules::DowntimeSchedule) -> st_ipc::ScheduleDto {
    st_ipc::ScheduleDto {
        id: s.id,
        name: s.name.clone(),
        weekday_mask: s.weekday_mask,
        start_minute: s.start_minute,
        end_minute: s.end_minute,
        enabled: s.enabled,
    }
}

pub(super) fn list_schedules(ctx: &Ctx) -> Response {
    let db = lock_db(&ctx.db);
    match db.list_schedules() {
        Ok(schedules) => Response::Schedules(st_ipc::SchedulesDto {
            schedules: schedules.iter().map(schedule_to_dto).collect(),
        }),
        Err(e) => {
            tracing::error!(error = %e, "failed to list schedules");
            Response::Error {
                code: ErrorCode::Internal,
                message: "Database error".into(),
            }
        }
    }
}

pub(super) fn create_schedule(
    ctx: &Ctx,
    name: &str,
    weekday_mask: u8,
    start_minute: u32,
    end_minute: u32,
) -> Response {
    let db = lock_db(&ctx.db);
    match db.create_schedule(name, weekday_mask, start_minute, end_minute) {
        Ok(id) => Response::ScheduleCreated(st_ipc::ScheduleDto {
            id,
            name: name.to_string(),
            weekday_mask,
            start_minute,
            end_minute,
            enabled: true,
        }),
        Err(e) => {
            tracing::error!(error = %e, "failed to create schedule");
            Response::Error {
                code: ErrorCode::Internal,
                message: e.to_string(),
            }
        }
    }
}

pub(super) fn update_schedule(
    ctx: &Ctx,
    id: i64,
    name: &str,
    weekday_mask: u8,
    start_minute: u32,
    end_minute: u32,
) -> Response {
    let db = lock_db(&ctx.db);
    match db.update_schedule(id, name, weekday_mask, start_minute, end_minute) {
        Ok(()) => accepted(ctx.clock.now_utc()),
        Err(e) => {
            tracing::error!(error = %e, "failed to update schedule");
            Response::Error {
                code: ErrorCode::Internal,
                message: e.to_string(),
            }
        }
    }
}

pub(super) fn set_schedule_enabled(ctx: &Ctx, id: i64, enabled: bool) -> Response {
    let db = lock_db(&ctx.db);
    match db.set_schedule_enabled(id, enabled) {
        Ok(()) => accepted(ctx.clock.now_utc()),
        Err(e) => {
            tracing::error!(error = %e, "failed to set schedule enabled");
            Response::Error {
                code: ErrorCode::Internal,
                message: e.to_string(),
            }
        }
    }
}

pub(super) fn delete_schedule(ctx: &Ctx, id: i64) -> Response {
    let db = lock_db(&ctx.db);
    match db.delete_schedule(id) {
        Ok(()) => accepted(ctx.clock.now_utc()),
        Err(e) => {
            tracing::error!(error = %e, "failed to delete schedule");
            Response::Error {
                code: ErrorCode::Internal,
                message: e.to_string(),
            }
        }
    }
}

pub(super) fn list_allowlist(ctx: &Ctx) -> Response {
    let db = lock_db(&ctx.db);
    let items = match db.list_allowlist() {
        Ok(items) => items,
        Err(e) => {
            tracing::error!(error = %e, "failed to list allowlist");
            return Response::Error {
                code: ErrorCode::Internal,
                message: "Database error".into(),
            };
        }
    };

    let apps = db.list_apps().unwrap_or_default();
    let app_map: HashMap<i64, String> = apps.into_iter().map(|a| (a.id, a.display_name)).collect();
    let sites = db.list_sites().unwrap_or_default();
    let site_map: HashMap<i64, String> = sites.into_iter().map(|s| (s.id, s.domain)).collect();

    let dtos = items
        .into_iter()
        .map(|(subject_type, subject_id)| {
            let name = if subject_type == "app" {
                app_map
                    .get(&subject_id)
                    .cloned()
                    .unwrap_or_else(|| format!("App #{}", subject_id))
            } else if subject_type == "site" {
                site_map
                    .get(&subject_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Site #{}", subject_id))
            } else {
                format!("{} #{}", subject_type, subject_id)
            };
            st_ipc::AllowlistItemDto {
                subject_type,
                subject_id,
                name,
            }
        })
        .collect();

    Response::Allowlist(st_ipc::AllowlistDto { items: dtos })
}

pub(super) fn set_allowlist(
    ctx: &Ctx,
    subject_type: &str,
    subject_id: i64,
    allowed: bool,
) -> Response {
    let db = lock_db(&ctx.db);
    match db.set_allowlist_subject(subject_type, subject_id, allowed) {
        Ok(()) => accepted(ctx.clock.now_utc()),
        Err(e) => {
            tracing::error!(error = %e, "failed to set allowlist");
            Response::Error {
                code: ErrorCode::Internal,
                message: e.to_string(),
            }
        }
    }
}
