//! Manual web blocks (the hosts-file domain list).

use super::*;

pub(super) fn list_manual_blocks(ctx: &Ctx) -> Response {
    let db = lock_db(&ctx.db);
    match db.list_block_rules() {
        Ok(rows) => {
            let domains = rows
                .into_iter()
                .filter(|r| r.blocklist_id.is_none() && r.category_id.is_none())
                .map(|r| r.domain)
                .collect();
            Response::ManualBlocks { domains }
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to list manual blocks");
            Response::Error {
                code: ErrorCode::Internal,
                message: "Database error".into(),
            }
        }
    }
}

/// Manual web blocks: rules with neither a blocklist nor a category.
pub(super) fn manual_rule_ids(db: &Db, domain: &str) -> st_storage::Result<Vec<i64>> {
    Ok(db
        .list_block_rules()?
        .into_iter()
        .filter(|r| r.blocklist_id.is_none() && r.category_id.is_none() && r.domain == domain)
        .map(|r| r.id)
        .collect())
}

pub(super) fn add_manual_block(ctx: &Ctx, domain: &str) -> Response {
    let Some(domain) = st_storage::normalize_domain(domain) else {
        return bad_request(format!("not a valid domain: {domain}"));
    };
    let db = lock_db(&ctx.db);
    // Idempotent: a second add must not create a duplicate that a single
    // remove would leave behind.
    match manual_rule_ids(&db, &domain) {
        Ok(ids) if !ids.is_empty() => return accepted(ctx.clock.now_utc()),
        Ok(_) => {}
        Err(e) => return error_internal(format!("list block rules: {e}")),
    }
    match db.add_block_rule(None, None, &domain, true, "block") {
        Ok(_) => accepted(ctx.clock.now_utc()),
        Err(e) => error_internal(format!("add block rule: {e}")),
    }
}

pub(super) fn remove_manual_block(ctx: &Ctx, domain: &str) -> Response {
    let Some(domain) = st_storage::normalize_domain(domain) else {
        return bad_request(format!("not a valid domain: {domain}"));
    };
    let db = lock_db(&ctx.db);
    let ids = match manual_rule_ids(&db, &domain) {
        Ok(ids) => ids,
        Err(e) => return error_internal(format!("list block rules: {e}")),
    };
    if ids.is_empty() {
        return Response::Error {
            code: ErrorCode::NotFound,
            message: format!("{domain} is not blocked"),
        };
    }
    // Remove every copy, including duplicates written by older versions.
    for id in ids {
        if let Err(e) = db.delete_block_rule(id) {
            return error_internal(format!("remove block rule: {e}"));
        }
    }
    accepted(ctx.clock.now_utc())
}
