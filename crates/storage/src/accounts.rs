//! Windows accounts (`0006_accounts.sql`, docs/adr/0005-per-account-usage.md).
//!
//! The agent learns accounts from the processes that connect to it: the SID
//! comes from the peer's token, never from the wire. Usage, blocks, extra
//! time and block-screen answers carry the account they belong to.

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use st_core::model::AccountId;

use super::{Db, Result};

/// One account as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRow {
    pub id: AccountId,
    pub sid: String,
    pub name: String,
    /// Limits apply. New accounts are watched.
    pub supervised: bool,
    pub last_seen_utc: String,
}

/// Tables whose rows belong to an account; legacy rows sit at account 0.
const OWNED_TABLES: [&str; 5] = [
    "usage_daily",
    "usage_intervals",
    "block_state",
    "overrides",
    "block_reasons",
];

impl Db {
    /// The account for `sid`, added (watched) the first time it is seen. Its
    /// name and last-seen time are refreshed on every call.
    ///
    /// The very first account adopts everything recorded before accounts
    /// existed (account 0): on a one-person PC that history is theirs.
    pub fn ensure_account(
        &mut self,
        sid: &str,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<AccountId> {
        let tx = self.conn.transaction()?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM accounts WHERE sid = ?1",
                params![sid],
                |r| r.get(0),
            )
            .optional()?;
        let id = match existing {
            Some(id) => {
                tx.execute(
                    "UPDATE accounts SET name = ?2, last_seen_utc = ?3 WHERE id = ?1",
                    params![id, name, now.to_rfc3339()],
                )?;
                id
            }
            None => {
                let first: i64 = tx.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))?;
                tx.execute(
                    "INSERT INTO accounts (sid, name, supervised, first_seen_utc, last_seen_utc)
                     VALUES (?1, ?2, 1, ?3, ?3)",
                    params![sid, name, now.to_rfc3339()],
                )?;
                let id = tx.last_insert_rowid();
                if first == 0 {
                    for table in OWNED_TABLES {
                        tx.execute(
                            &format!("UPDATE {table} SET account_id = ?1 WHERE account_id = 0"),
                            params![id],
                        )?;
                    }
                    tracing::info!(account = id, "first account adopted earlier usage");
                }
                id
            }
        };
        tx.commit()?;
        Ok(AccountId(id))
    }

    /// Every known account, by name.
    pub fn list_accounts(&self) -> Result<Vec<AccountRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sid, name, supervised, last_seen_utc FROM accounts
             ORDER BY name COLLATE NOCASE, id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AccountRow {
                    id: AccountId(r.get(0)?),
                    sid: r.get(1)?,
                    name: r.get(2)?,
                    supervised: r.get::<_, i64>(3)? != 0,
                    last_seen_utc: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Watch or stop watching an account. `false` when there is no such
    /// account.
    pub fn set_account_supervised(&self, account: AccountId, supervised: bool) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE accounts SET supervised = ?2 WHERE id = ?1",
            params![account.0, i64::from(supervised)],
        )?;
        Ok(n > 0)
    }

    /// Whether limits apply to `account`. An account the database does not
    /// know (account 0 included) counts as watched: enforcement fails closed.
    pub fn is_supervised(&self, account: AccountId) -> Result<bool> {
        let flag: Option<i64> = self
            .conn
            .query_row(
                "SELECT supervised FROM accounts WHERE id = ?1",
                params![account.0],
                |r| r.get(0),
            )
            .optional()?;
        Ok(flag.is_none_or(|f| f != 0))
    }
}

#[cfg(test)]
mod tests {
    use st_core::daykey::DayKey;
    use st_core::model::AccountId;

    use crate::testutil::{interval, now};
    use crate::Db;

    #[test]
    fn the_first_account_adopts_earlier_usage_and_later_ones_do_not() {
        let mut db = Db::open_in_memory().expect("open");
        let day = DayKey(20260820);
        let mut legacy = interval(7, 600, day);
        legacy.account = AccountId::NOBODY;
        db.record_interval(&legacy).expect("legacy usage");

        let first = db
            .ensure_account("S-1-5-21-1", "Sam", now())
            .expect("first");
        let second = db
            .ensure_account("S-1-5-21-2", "Alex", now())
            .expect("second");
        assert_ne!(first, second);
        assert_eq!(
            db.day_summary(first, day).expect("first").total_seconds,
            0,
            "app 7 has no apps row"
        );
        let owned: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM usage_daily WHERE account_id = ?1",
                [first.0],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(owned, 1, "legacy row moved to the first account");
        let left: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM usage_daily WHERE account_id = 0",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(left, 0);
    }

    #[test]
    fn usage_blocks_and_answers_stay_with_their_account() {
        use st_core::limits::{LimitTarget, UsageSnapshot};
        use st_core::model::{AppKey, SubjectRef};
        use st_core::reasons::BlockReason;

        let mut db = Db::open_in_memory().expect("open");
        let sam = db.ensure_account("S-1-5-21-1", "Sam", now()).expect("sam");
        let alex = db
            .ensure_account("S-1-5-21-2", "Alex", now())
            .expect("alex");
        let games = db.category_id("games").expect("games");
        let app = db
            .upsert_app(&AppKey::windows_exe("C:\\g.exe"), "G", None, games, now())
            .expect("app");
        let day = DayKey(20260820);
        let mut used = interval(app, 900, day);
        used.account = sam;
        db.record_interval(&used).expect("record");
        db.set_block(sam, SubjectRef::App(app), "limit", now(), None)
            .expect("block");
        db.record_block_reason(sam, day, app, BlockReason::Habit, now())
            .expect("reason");

        let target = LimitTarget::App(app);
        assert_eq!(
            db.day_snapshot(sam, day)
                .expect("sam")
                .seconds_used(&target),
            900
        );
        assert_eq!(
            db.day_snapshot(alex, day)
                .expect("alex")
                .seconds_used(&target),
            0
        );
        assert_eq!(
            db.weekly_summary(alex, day).expect("week").days[6].total_seconds,
            0
        );
        assert!(db
            .is_blocked(sam, SubjectRef::App(app))
            .expect("sam blocked"));
        assert!(!db
            .is_blocked(alex, SubjectRef::App(app))
            .expect("alex free"));
        assert!(db
            .block_reason_counts(alex, day, day)
            .expect("alex")
            .is_empty());

        db.clear_account_blocks(sam).expect("clear");
        assert!(db.all_blocks().expect("all").is_empty());
    }

    #[test]
    fn accounts_are_found_again_renamed_and_watched_by_default() {
        let mut db = Db::open_in_memory().expect("open");
        let id = db.ensure_account("S-1-5-21-1", "sam", now()).expect("add");
        assert_eq!(
            db.ensure_account("S-1-5-21-1", "Sam", now())
                .expect("again"),
            id
        );
        let all = db.list_accounts().expect("list");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Sam");
        assert!(all[0].supervised);

        assert!(db.set_account_supervised(id, false).expect("unwatch"));
        assert!(!db.is_supervised(id).expect("read"));
        assert!(!db
            .set_account_supervised(AccountId(99), true)
            .expect("missing"));
        assert!(db
            .is_supervised(AccountId(99))
            .expect("unknown fails closed"));
    }
}
