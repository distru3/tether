//! App actions: quitting a blocked app, recategorising, discovery.

use super::*;

/// "Quit" from the block overlay: terminate the app's process tree. This is a
/// deliberate user action (not the removed auto-freeze), so `auth` PIN-gates
/// it like every other mutating request.
pub(super) fn close_apps(ctx: &Ctx, app_id: i64) -> Response {
    let db = lock_db(&ctx.db);
    let Some(record) = db.app_record(app_id).ok().flatten() else {
        return Response::Error {
            code: ErrorCode::NotFound,
            message: format!("app {app_id} not found"),
        };
    };
    let key: AppKey = record.key;
    drop(db);

    let mut processes = lock_recover(&ctx.processes, "process controller");
    let pids = match processes.find_processes(&key) {
        Ok(pids) => pids,
        Err(e) => return error_internal(format!("find processes: {e}")),
    };
    for pid in pids {
        if let Err(e) = processes.terminate(pid) {
            tracing::warn!(pid, error = %e, "terminate failed (process may have exited)");
        }
    }
    drop(processes);

    let db = lock_db(&ctx.db);
    let _ = db.clear_block(SubjectRef::App(app_id));
    accepted(ctx.clock.now_utc())
}

pub(super) fn categorize(ctx: &Ctx, app_id: i64, primary: Option<i64>, tags: &[i64]) -> Response {
    let mut db = lock_db(&ctx.db);
    match primary {
        Some(p) => match db.set_app_categories(app_id, p, tags, true) {
            Ok(()) => accepted(ctx.clock.now_utc()),
            Err(e) => error_internal(format!("categorise: {e}")),
        },
        None => {
            // Auto-detect requested: reset to uncategorized and clear user flag
            let uncat = match db.category_id("uncategorized") {
                Ok(id) => id,
                Err(e) => return error_internal(format!("missing uncategorized category: {e}")),
            };
            match db.set_app_categories(app_id, uncat, &[], false) {
                Ok(()) => accepted(ctx.clock.now_utc()),
                Err(e) => error_internal(format!("categorise: {e}")),
            }
        }
    }
}

pub(super) fn register_discovered_apps(
    ctx: &Ctx,
    apps: Vec<st_ipc::DiscoveredAppDto>,
    now: DateTime<Utc>,
) -> Response {
    let mut db = lock_db(&ctx.db);
    let default_category = match db.category_id("uncategorized") {
        Ok(id) => id,
        Err(e) => return error_internal(format!("missing uncategorized category: {e}")),
    };

    let mut registered_count = 0usize;
    for app in apps {
        let app_id = match db.upsert_app(
            &app.key,
            &app.display_name,
            app.publisher.as_deref(),
            default_category,
            now,
        ) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(key = %app.key.to_db_string(), error = %e, "failed to upsert discovered app");
                continue;
            }
        };
        registered_count += 1;

        // Auto-classify if not user classified and currently uncategorized
        if let Ok((primary, user_classified)) = db.app_category_state(app_id) {
            if !user_classified && primary == default_category {
                if let Some(classification) = crate::classify::classify(
                    &app.key,
                    Some(&app.display_name),
                    app.publisher.as_deref(),
                ) {
                    if let Ok(primary_id) = db.category_id(classification.primary) {
                        let mut tags = Vec::with_capacity(classification.tags.len());
                        for tag in classification.tags {
                            if let Ok(tag_id) = db.category_id(tag) {
                                tags.push(tag_id);
                            }
                        }
                        let _ = db.set_app_categories(app_id, primary_id, &tags, false);
                    }
                }
            }
        }
    }

    tracing::debug!(
        count = registered_count,
        "proactively registered discovered apps in catalog"
    );
    accepted(now)
}
