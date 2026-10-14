//! Windows accounts: `ListAccounts` and `SetAccountSupervised`
//! (docs/adr/0005-per-account-usage.md). `auth` asks for the PIN before an
//! account stops being watched.

use super::*;

pub(super) fn account_dto(
    row: st_storage::AccountRow,
    caller: Option<AccountId>,
) -> st_ipc::AccountDto {
    st_ipc::AccountDto {
        id: row.id.0,
        is_caller: caller == Some(row.id),
        name: row.name,
        supervised: row.supervised,
        last_seen_utc: row.last_seen_utc,
    }
}

pub(super) fn list_accounts(ctx: &Ctx, caller: Caller) -> Response {
    match lock_db(&ctx.db).list_accounts() {
        Ok(rows) => Response::Accounts(st_ipc::AccountsDto {
            accounts: rows
                .into_iter()
                .map(|row| account_dto(row, caller.account))
                .collect(),
        }),
        Err(e) => error_internal(format!("list accounts: {e}")),
    }
}

pub(super) fn set_account_supervised(
    ctx: &Ctx,
    account: AccountId,
    supervised: bool,
    now: DateTime<Utc>,
) -> Response {
    let db = lock_db(&ctx.db);
    match db.set_account_supervised(account, supervised) {
        Ok(true) => {}
        Ok(false) => {
            return Response::Error {
                code: ErrorCode::NotFound,
                message: format!("{account} not found"),
            }
        }
        Err(e) => return error_internal(format!("update account: {e}")),
    }
    // Limits stop applying at once: lift whatever this account had.
    if !supervised {
        if let Err(e) = db.clear_account_blocks(account) {
            return error_internal(format!("lift blocks: {e}"));
        }
    }
    let detail = format!(
        "{account} {}",
        if supervised { "watched" } else { "unwatched" }
    );
    if let Err(e) = db.audit(now, "account_supervision", Some(&detail)) {
        tracing::warn!(error = %e, "failed to audit an account change");
    }
    accepted(now)
}
