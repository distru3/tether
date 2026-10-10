use super::*;
use st_core::clock::TestClock;
use st_core::limits::Limit;
use st_core::limits::UsageSnapshot;
use st_core::pin::verify_pin;
use st_ipc::IpcError;
use std::sync::RwLock;

fn base() -> Limit {
    Limit::new(1, LimitTarget::Total, 30)
}

#[test]
fn tightening_is_not_loosening() {
    let existing = base();
    assert!(!is_loosening(&existing, 15, [None; 7]));
    assert!(!is_loosening(&existing, 30, [None; 7]));
}

#[test]
fn increasing_minutes_is_loosening() {
    let existing = base();
    assert!(is_loosening(&existing, 45, [None; 7]));
}

#[test]
fn raising_a_weekday_override_is_loosening() {
    let existing = base();
    let mut new = [None; 7];
    new[6] = Some(120);
    assert!(is_loosening(&existing, 30, new));
}

#[test]
fn lowering_a_weekday_override_is_tightening() {
    let mut existing = base();
    existing.weekday_minutes[6] = Some(120);
    let mut new = [None; 7];
    new[6] = Some(60);
    assert!(!is_loosening(&existing, 30, new));
}

#[test]
fn mixed_changes_are_treated_as_loosening() {
    let existing = base();
    let mut new = [None; 7];
    new[0] = Some(15); // tighter on Monday
    new[6] = Some(120); // looser on Sunday
    assert!(is_loosening(&existing, 30, new));
}

// -- Handler-level fixtures. ---------------------------------------------

/// DNS backend that must never be reached by tests that don't script it.
struct NoDns;

impl DnsBackend for NoDns {
    fn capture(&self) -> Result<String, String> {
        Err("no DNS in tests".into())
    }
    fn apply(&self, _captured: &str) -> Result<(), String> {
        Err("no DNS in tests".into())
    }
    fn restore(&self, _captured: Option<&str>) -> Result<(), String> {
        Err("no DNS in tests".into())
    }
}

/// Records terminations so the overlay's Quit action can be asserted.
struct FakeProcesses {
    pids: Vec<u32>,
    terminated: Arc<Mutex<Vec<u32>>>,
}

impl Default for FakeProcesses {
    fn default() -> Self {
        Self {
            pids: vec![111, 222],
            terminated: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl ProcessController for FakeProcesses {
    fn find_processes(&mut self, _key: &AppKey) -> st_core::platform::PlatformResult<Vec<u32>> {
        Ok(self.pids.clone())
    }

    fn freeze(&mut self, _pid: u32) -> st_core::platform::PlatformResult<()> {
        Ok(())
    }

    fn thaw(&mut self, _pid: u32) -> st_core::platform::PlatformResult<()> {
        Ok(())
    }

    fn terminate(&mut self, pid: u32) -> st_core::platform::PlatformResult<()> {
        self.terminated.lock().expect("log").push(pid);
        Ok(())
    }

    fn backend(&self) -> &'static str {
        "fake"
    }
}

fn test_ctx(db: Db, clock: TestClock) -> Ctx {
    test_ctx_with_policy(db, clock, |_| ())
}

/// A second context over an existing context's database, for tests that
/// need two moments in time against the same storage.
fn test_ctx_with_db_handle(db: &Arc<Mutex<Db>>, clock: TestClock) -> Ctx {
    Ctx {
        db: Arc::clone(db),
        status: Arc::new(StatusInfo {
            agent_version: "test".into(),
            tracker_backend: "fake".into(),
            enforcement_backend: "fake".into(),
            filter_backend: "none".into(),
            self_sampling: false,
            blocks_encrypted_dns: false,
        }),
        policy: Arc::new(RwLock::new(Policy::default())),
        processes: Arc::new(Mutex::new(Box::new(FakeProcesses::default()))),
        clock: Arc::new(clock),
        live: Arc::new(Live::default()),
        dns: Arc::new(NoDns),
        data_dir: None,
    }
}

fn test_ctx_with_policy(db: Db, clock: TestClock, tweak: impl FnOnce(&mut Policy)) -> Ctx {
    let mut policy = Policy::default();
    tweak(&mut policy);
    Ctx {
        db: Arc::new(Mutex::new(db)),
        status: Arc::new(StatusInfo {
            agent_version: "test".into(),
            tracker_backend: "fake".into(),
            enforcement_backend: "fake".into(),
            filter_backend: "none".into(),
            self_sampling: false,
            blocks_encrypted_dns: false,
        }),
        policy: Arc::new(std::sync::RwLock::new(policy)),
        processes: Arc::new(Mutex::new(Box::new(FakeProcesses::default()))),
        clock: Arc::new(clock),
        live: Arc::new(Live::default()),
        dns: Arc::new(NoDns),
        data_dir: None,
    }
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("valid rfc3339")
        .with_timezone(&Utc)
}

fn error_code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error { code, .. } => Some(*code),
        _ => None,
    }
}

fn expect_accepted(response: Response, expected: DateTime<Utc>) {
    match response {
        Response::Accepted {
            hud: None,
            effective_utc,
        } => assert_eq!(
            DateTime::parse_from_rfc3339(&effective_utc)
                .expect("rfc3339")
                .with_timezone(&Utc),
            expected,
            "Accepted must carry the ingest/effective instant"
        ),
        other => panic!("expected Accepted at {expected}, got {other:?}"),
    }
}

fn seed_game_app(db: &mut Db, path: &str) -> i64 {
    let uncat = db.category_id("uncategorized").expect("uncat");
    let games = db.category_id("games").expect("games");
    let key = AppKey::windows_exe(path);
    let id = db
        .upsert_app(&key, "Steam", None, uncat, at("2026-08-20T09:00:00Z"))
        .expect("app");
    db.set_app_categories(id, games, &[], false)
        .expect("classify");
    id
}

fn record_usage(db: &mut Db, app: i64, secs: i64, day: DayKey) {
    db.record_interval(&st_core::model::UsageInterval {
        subject: SubjectRef::App(app),
        session_id: "test".into(),
        start: at("2026-08-20T10:00:00Z"),
        end: at("2026-08-20T10:00:00Z") + Duration::seconds(secs),
        day_key: day,
    })
    .expect("interval");
}

fn obs(app_key: &str, observed_at: &str, idle_seconds: u32) -> ObservationDto {
    ObservationDto {
        app_key: AppKey::windows_exe(app_key),
        window_title: None,
        idle_seconds,
        observed_at_utc: observed_at.into(),
    }
}

fn report_of(observations: Vec<ObservationDto>) -> Request {
    Request::ReportUsage {
        report: ReportUsageDto {
            observations,
            focused_key: None,
        },
    }
}

fn used_secs_for_key(db: &mut Db, key: &str, day: DayKey) -> i64 {
    let app = db
        .app_id_for_key(&AppKey::windows_exe(key))
        .expect("lookup")
        .expect("ingested app exists");
    db.day_snapshot(day)
        .expect("snapshot")
        .seconds_used(&LimitTarget::App(app))
}

// -- Handler behaviour. ---------------------------------------------------

/// Pull the one-time code out of a PinVault reply, failing loudly on any
/// other variant.
fn expect_vault(response: Response) -> String {
    match response {
        Response::PinVault { recovery_code } => recovery_code,
        other => panic!("expected PinVault, got {other:?}"),
    }
}

#[test]
fn set_pin_first_set_then_change_requires_the_current_pin() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
    );

    let first_code = expect_vault(handle(
        &ctx,
        Request::SetPin {
            new_pin: "1234".into(),
            current_pin: None,
        },
    ));

    // Changing without (or with a wrong) current PIN is refused...
    for wrong in [None, Some("0000")] {
        assert_eq!(
            error_code(&handle(
                &ctx,
                Request::SetPin {
                    new_pin: "5678".into(),
                    current_pin: wrong.map(Into::into)
                }
            )),
            Some(ErrorCode::BadPin)
        );
    }

    // ...and with the right current PIN the change lands.
    let second_code = expect_vault(handle(
        &ctx,
        Request::SetPin {
            new_pin: "5678".into(),
            current_pin: Some("1234".into()),
        },
    ));
    let stored = lock_db(&ctx.db)
        .pin_hash()
        .expect("read")
        .expect("configured");
    assert!(verify_pin("5678", &stored), "new PIN must be live");
    assert_ne!(
        first_code, second_code,
        "every vault rotation must retire the old recovery code"
    );
}

#[test]
fn a_recovery_code_resets_a_forgotten_pin_and_rotates_itself() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
    );
    let code = expect_vault(handle(
        &ctx,
        Request::SetPin {
            new_pin: "1234".into(),
            current_pin: None,
        },
    ));

    // Wrong code refused; right code — even sloppily typed — accepted.
    assert_eq!(
        error_code(&handle(
            &ctx,
            Request::RecoverPin {
                recovery_code: "AAAA-BBBB-CCCC-DDDD".into(),
                new_pin: "9999".into()
            }
        )),
        Some(ErrorCode::BadPin)
    );
    expect_vault(handle(
        &ctx,
        Request::RecoverPin {
            // Lowercase, no dashes, stray spaces: must still verify.
            recovery_code: format!(" {} ", code.replace('-', "").to_lowercase()),
            new_pin: "9999".into(),
        },
    ));

    let db_guard = lock_db(&ctx.db);
    let stored = db_guard.pin_hash().expect("read").expect("configured");
    assert!(verify_pin("9999", &stored), "recovered PIN must be live");

    // Rotation retired the used code...
    drop(db_guard);
    assert_eq!(
        error_code(&handle(
            &ctx,
            Request::RecoverPin {
                recovery_code: code.clone(),
                new_pin: "1111".into()
            }
        )),
        Some(ErrorCode::BadPin)
    );

    // ...but a later change can present either the current PIN or the
    // standing recovery code; each rotation mints a fresh code.
    let fresh = expect_vault(handle(
        &ctx,
        Request::SetPin {
            new_pin: "2222".into(),
            current_pin: Some("9999".into()),
        },
    ));
    assert_ne!(fresh, code);
}

#[test]
fn removing_the_pin_requires_a_credential_and_clears_the_whole_vault() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
    );
    let code = expect_vault(handle(
        &ctx,
        Request::SetPin {
            new_pin: "1234".into(),
            current_pin: None,
        },
    ));

    assert_eq!(
        error_code(&handle(
            &ctx,
            Request::RemovePin {
                credential: "0000".into()
            }
        )),
        Some(ErrorCode::BadPin),
        "no credential, no removal"
    );

    // The recovery code is as good as the PIN for standing down the gate.
    expect_accepted(
        handle(&ctx, Request::RemovePin { credential: code }),
        at("2026-08-20T12:00:00Z"),
    );
    // Scoped so the guard cannot deadlock the idempotent call below.
    {
        let db_guard = lock_db(&ctx.db);
        assert_eq!(db_guard.pin_hash().expect("read"), None);
        assert_eq!(db_guard.recovery_hash().expect("read"), None);
    }

    // Idempotent when no vault exists.
    expect_accepted(
        handle(
            &ctx,
            Request::RemovePin {
                credential: String::new(),
            },
        ),
        at("2026-08-20T12:00:00Z"),
    );
}

#[test]
fn set_limit_tightening_applies_immediately() {
    let mut db = Db::open_in_memory().expect("db");
    let games = db.category_id("games").expect("games");
    seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
    db.upsert_limit(
        &Limit::new(1, LimitTarget::Category(games), 30),
        at("2026-08-20T09:00:00Z"),
    )
    .expect("seed limit");

    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let response = handle(
        &ctx,
        Request::SetLimit {
            target: LimitTargetDto::Category { id: games },
            default_minutes: 15,
            weekday_minutes: [None; 7],
            enabled: true,
            pin: String::new(),
        },
    );

    expect_accepted(response, at("2026-08-20T12:00:00Z"));
    let limits = lock_db(&ctx.db).load_limits().expect("limits");
    assert_eq!(limits[0].default_minutes, 15, "tightened value is live");
}

#[test]
fn set_limit_loosening_waits_out_the_cooldown_then_promotes() {
    let db = Db::open_in_memory().expect("db");
    let games = db.category_id("games").expect("games");
    db.upsert_limit(
        &Limit::new(1, LimitTarget::Category(games), 30),
        at("2026-08-20T09:00:00Z"),
    )
    .expect("seed limit");

    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let response = handle(
        &ctx,
        Request::SetLimit {
            target: LimitTargetDto::Category { id: games },
            default_minutes: 120,
            weekday_minutes: [None; 7],
            enabled: true,
            pin: String::new(),
        },
    );

    expect_accepted(response, at("2026-08-21T12:00:00Z"));
    let mut db_guard = lock_db(&ctx.db);
    assert_eq!(
        db_guard.load_limits().expect("limits")[0].default_minutes,
        30,
        "the tighter value must keep being enforced during cooldown"
    );
    db_guard
        .promote_pending_limits(at("2026-08-21T12:00:00Z"))
        .expect("promote");
    assert_eq!(
        db_guard.load_limits().expect("limits")[0].default_minutes,
        120,
        "promotion applies the loosened value"
    );
}

#[test]
fn a_failed_limit_read_is_internal_rather_than_fail_open() {
    let db = Db::open_in_memory().expect("db");
    let games = db.category_id("games").expect("games");
    db.upsert_limit(
        &Limit::new(1, LimitTarget::Category(games), 30),
        at("2026-08-20T09:00:00Z"),
    )
    .expect("seed limit");
    // Corrupt the column load_limits parses, forcing the read to fail.
    db.conn()
        .execute("UPDATE limits SET weekday_minutes = 'not-json'", [])
        .expect("corrupt");

    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let response = handle(
        &ctx,
        Request::SetLimit {
            target: LimitTargetDto::Category { id: games },
            default_minutes: 15,
            weekday_minutes: [None; 7],
            enabled: true,
            pin: String::new(),
        },
    );
    assert_eq!(error_code(&response), Some(ErrorCode::Internal));
}

#[test]
fn a_non_limitable_category_rejects_the_limit() {
    let db = Db::open_in_memory().expect("db");
    let dev = db.category_id("development").expect("development");
    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));

    let response = handle(
        &ctx,
        Request::SetLimit {
            target: LimitTargetDto::Category { id: dev },
            default_minutes: 30,
            weekday_minutes: [None; 7],
            enabled: true,
            pin: String::new(),
        },
    );
    assert_eq!(error_code(&response), Some(ErrorCode::NotLimitable));
}

#[test]
fn delete_limit_applies_instantly() {
    let db = Db::open_in_memory().expect("db");
    let games = db.category_id("games").expect("games");
    db.upsert_limit(
        &Limit::new(1, LimitTarget::Category(games), 30),
        at("2026-08-20T09:00:00Z"),
    )
    .expect("seed limit");

    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let response = handle(
        &ctx,
        Request::DeleteLimit {
            target: LimitTargetDto::Category { id: games },
            pin: String::new(),
        },
    );
    // Instant by design (owner decision, 2026-08): standing an order down
    // is not an impulse the cooldown needs to guard.
    expect_accepted(response, at("2026-08-20T12:00:00Z"));

    let db_guard = lock_db(&ctx.db);
    assert!(db_guard.load_limits().expect("limits").is_empty());
}

#[test]
fn disabling_a_limit_applies_instantly_while_loosening_still_waits() {
    let db = Db::open_in_memory().expect("db");
    let games = db.category_id("games").expect("games");
    db.upsert_limit(
        &Limit::new(1, LimitTarget::Category(games), 30),
        at("2026-08-20T09:00:00Z"),
    )
    .expect("seed limit");

    // Disable: instant.
    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let response = handle(
        &ctx,
        Request::SetLimit {
            target: LimitTargetDto::Category { id: games },
            default_minutes: 30,
            weekday_minutes: [None; 7],
            enabled: false,
            pin: String::new(),
        },
    );
    expect_accepted(response, at("2026-08-20T12:00:00Z"));
    assert!(
        !lock_db(&ctx.db).load_limits().expect("limits")[0].enabled,
        "the disabled state must be live immediately"
    );

    // Loosening minutes while enabled still queues behind the cooldown.
    let ctx = test_ctx_with_db_handle(&ctx.db, TestClock::new(at("2026-08-20T12:05:00Z"), 0));
    let response = handle(
        &ctx,
        Request::SetLimit {
            target: LimitTargetDto::Category { id: games },
            default_minutes: 120,
            weekday_minutes: [None; 7],
            enabled: true,
            pin: String::new(),
        },
    );
    expect_accepted(response, at("2026-08-21T12:05:00Z"));
    let db_guard = lock_db(&ctx.db);
    assert_eq!(
        db_guard.load_limits().expect("limits")[0].default_minutes,
        30,
        "the tighter value keeps being enforced during cooldown"
    );
}

/// Regression for bug 2: an override granted at 19:00 local in UTC-7 must
/// land on *today's* budget, not on tomorrow's UTC calendar day.
#[test]
fn grant_override_lands_on_the_local_day_not_the_utc_day() {
    // 02:00 UTC on the 21st == 19:00 local on the 20th.
    let utc_minus_seven = -7 * 3600;
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-21T02:00:00Z"), utc_minus_seven),
    );

    let response = handle(
        &ctx,
        Request::GrantOverride {
            target: LimitTargetDto::Total,
            seconds: 900,
            pin: String::new(),
        },
    );
    expect_accepted(response, at("2026-08-21T02:00:00Z"));

    let db_guard = lock_db(&ctx.db);
    let today = db_guard.day_snapshot(DayKey(20260820)).expect("snap");
    let expires = today
        .active_timer_expires_utc(&LimitTarget::Total)
        .expect("timer expires");
    assert_eq!(
        expires
            .signed_duration_since(at("2026-08-21T02:00:00Z"))
            .num_seconds(),
        900,
        "override credited to the local day"
    );
    let utc_day = db_guard.day_snapshot(DayKey(20260821)).expect("snap");
    assert!(
        utc_day
            .active_timer_expires_utc(&LimitTarget::Total)
            .is_none(),
        "the UTC calendar day must not receive the bonus"
    );
}

#[test]
fn grant_override_refused_outright_in_strict_mode() {
    let ctx = test_ctx_with_policy(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
        |p| p.strict_mode = true,
    );
    let response = handle(
        &ctx,
        Request::GrantOverride {
            target: LimitTargetDto::Total,
            seconds: 900,
            pin: String::new(),
        },
    );
    assert_eq!(error_code(&response), Some(ErrorCode::StrictMode));
}

#[test]
fn close_apps_terminates_the_process_tree_and_clears_the_block() {
    let mut db = Db::open_in_memory().expect("db");
    let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
    db.set_block(
        SubjectRef::App(app),
        "limit",
        at("2026-08-20T12:00:00Z"),
        None,
    )
    .expect("block");

    // A shared log the fake writes into, so the test can assert what the
    // controller (hidden behind `Box<dyn ProcessController>`) did.
    let terminated_log = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    ctx.processes = Arc::new(Mutex::new(Box::new(FakeProcesses {
        pids: vec![111, 222],
        terminated: Arc::clone(&terminated_log),
    })));

    let response = handle(
        &ctx,
        Request::CloseApps {
            app_id: app,
            pin: String::new(),
        },
    );
    expect_accepted(response, at("2026-08-20T12:00:00Z"));
    assert!(!lock_db(&ctx.db)
        .is_blocked(SubjectRef::App(app))
        .expect("check"));

    assert_eq!(
        terminated_log.lock().expect("log").clone(),
        vec![111, 222],
        "every pid in the process tree must be terminated"
    );
}

#[test]
fn closing_an_unblocked_app_requires_a_valid_pin_once_configured() {
    let mut db = Db::open_in_memory().expect("db");
    let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
    db.set_pin_hash(&hash_pin("9999").expect("hash"))
        .expect("set pin");

    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let response = handle(
        &ctx,
        Request::CloseApps {
            app_id: app,
            pin: "1111".into(),
        },
    );
    assert_eq!(error_code(&response), Some(ErrorCode::BadPin));
}

#[test]
fn blocked_apps_reports_label_and_key_for_the_overlay_owner() {
    let mut db = Db::open_in_memory().expect("db");
    let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
    db.set_block(
        SubjectRef::App(app),
        "limit",
        at("2026-08-20T12:00:00Z"),
        None,
    )
    .expect("block");

    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let response = handle(&ctx, Request::BlockedApps);
    let Response::BlockedApps(dto) = response else {
        panic!("expected BlockedApps, got {response:?}");
    };
    assert_eq!(dto.blocked.len(), 1);
    assert_eq!(dto.blocked[0].app_id, app);
    assert_eq!(dto.blocked[0].label, "Steam");
    assert_eq!(
        dto.blocked[0].app_key,
        "win-exe:c:\\games\\steam\\steam.exe"
    );
}

#[test]
fn catalog_maps_apps_categories_and_limits() {
    let mut db = Db::open_in_memory().expect("db");
    let games = db.category_id("games").expect("games");
    let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
    db.upsert_limit(
        &Limit::new(7, LimitTarget::App(app), 45),
        at("2026-08-20T09:00:00Z"),
    )
    .expect("limit");

    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let Response::Catalog(dto) = handle(&ctx, Request::Catalog) else {
        panic!("expected Catalog");
    };

    assert_eq!(dto.apps.len(), 1);
    assert_eq!(dto.apps[0].id, app);
    assert_eq!(dto.apps[0].key, "win-exe:c:\\games\\steam\\steam.exe");
    assert_eq!(dto.apps[0].primary_category, games);

    assert!(dto
        .categories
        .iter()
        .any(|c| c.slug == "games" && c.kind == "limitable"));
    assert!(dto
        .categories
        .iter()
        .any(|c| c.slug == "development" && c.kind == "never_block"));

    assert_eq!(dto.limits.len(), 1);
    assert_eq!(
        dto.limits[0].target,
        LimitTargetDto::App { id: app },
        "the limit must point at the seeded app"
    );
    assert_eq!(dto.limits[0].default_minutes, 45);
}

#[test]
fn day_summary_maps_rows_limit_seconds_and_blocked_flags() {
    let mut db = Db::open_in_memory().expect("db");
    let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
    record_usage(&mut db, app, 600, DayKey(20260820));
    db.upsert_limit(
        &Limit::new(3, LimitTarget::App(app), 30),
        at("2026-08-20T09:00:00Z"),
    )
    .expect("limit");
    db.set_block(
        SubjectRef::App(app),
        "limit",
        at("2026-08-20T12:00:00Z"),
        None,
    )
    .expect("block");

    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let Response::DaySummary(dto) = handle(
        &ctx,
        Request::DaySummary {
            day: DayKey(20260820),
        },
    ) else {
        panic!("expected DaySummary");
    };

    assert_eq!(dto.total_seconds, 600);
    assert_eq!(dto.apps.len(), 1);
    assert_eq!(dto.apps[0].id, app);
    assert_eq!(dto.apps[0].seconds, 600);
    assert_eq!(dto.apps[0].limit_seconds, Some(1800));
    assert_eq!(
        dto.categories[0].limit_seconds, None,
        "no category limit configured"
    );
    assert!(dto.apps[0].blocked, "the block must be surfaced honestly");
    assert_eq!(dto.categories.len(), 1);
    assert_eq!(dto.categories[0].seconds, 600);
}

#[test]
fn weekly_summary_maps_seven_zero_filled_days_oldest_first_with_previous_week_total() {
    let mut db = Db::open_in_memory().expect("db");
    let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
    // One day inside the reported week (2026-08-28..09-03, crossing the
    // month boundary), one day in the previous week (..08-27).
    record_usage(&mut db, app, 600, DayKey(20260829));
    record_usage(&mut db, app, 1200, DayKey(20260827));

    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let Response::WeeklySummary(dto) = handle(
        &ctx,
        Request::WeeklySummary {
            end_day: DayKey(20260903),
        },
    ) else {
        panic!("expected WeeklySummary");
    };

    assert_eq!(dto.days.len(), 7, "always exactly seven days");
    assert_eq!(dto.days[0].day, DayKey(20260828), "oldest first");
    assert_eq!(
        dto.days[0].total_seconds, 0,
        "empty day is zero, not missing"
    );
    assert_eq!(dto.days[1].day, DayKey(20260829));
    assert_eq!(dto.days[1].total_seconds, 600);
    assert_eq!(dto.days[6].day, DayKey(20260903), "end day is last");
    assert_eq!(dto.days[6].total_seconds, 0);
    assert_eq!(dto.previous_week_total, 1200);
}

#[test]
fn weekly_summary_with_an_invalid_day_key_is_an_internal_error() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
    );
    let response = handle(
        &ctx,
        Request::WeeklySummary {
            end_day: DayKey(20260230), // February 30th does not exist.
        },
    );
    assert_eq!(error_code(&response), Some(ErrorCode::Internal));
}

// -- Report ingestion. ----------------------------------------------------

#[test]
fn report_usage_credits_ordered_active_observations_regardless_of_wire_order() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-25T10:05:00Z"), 0),
    );

    // Deliberately shuffled: t+60 first, then t and t+30.
    let response = handle(
        &ctx,
        report_of(vec![
            obs("c:\\apps\\game.exe", "2026-08-25T10:01:00Z", 0),
            obs("c:\\apps\\game.exe", "2026-08-25T10:00:00Z", 0),
            obs("c:\\apps\\game.exe", "2026-08-25T10:00:30Z", 0),
        ]),
    );
    expect_accepted(response, at("2026-08-25T10:05:00Z"));

    let mut db_guard = lock_db(&ctx.db);
    assert_eq!(
        used_secs_for_key(&mut db_guard, "c:\\apps\\game.exe", DayKey(20260825)),
        60
    );
}

#[test]
fn report_usage_idle_observation_breaks_accrual_until_activity_resumes() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-25T10:05:00Z"), 0),
    );
    let game = "c:\\apps\\game.exe";

    // Active, then focused-but-idle past the threshold, then active again.
    handle(
        &ctx,
        report_of(vec![
            obs(game, "2026-08-25T10:00:00Z", 0),
            obs(game, "2026-08-25T10:00:30Z", 90),
        ]),
    );
    handle(&ctx, report_of(vec![obs(game, "2026-08-25T10:01:00Z", 90)]));
    // Activity resumes: a fresh run starts here.
    handle(
        &ctx,
        report_of(vec![
            obs(game, "2026-08-25T10:03:00Z", 0),
            obs(game, "2026-08-25T10:03:30Z", 0),
        ]),
    );

    let mut db_guard = lock_db(&ctx.db);
    // Only the resumed run bridges: [10:03:00, 10:03:30] = 30s. The idle
    // gap contributed nothing even though the app stayed foregrounded.
    assert_eq!(
        used_secs_for_key(&mut db_guard, "c:\\apps\\game.exe", DayKey(20260825)),
        30
    );
}

#[test]
fn report_usage_skips_wildly_future_timestamps_but_still_accepts() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-25T10:05:00Z"), 0),
    );

    let response = handle(
        &ctx,
        report_of(vec![
            obs("c:\\apps\\game.exe", "2026-08-25T11:00:00Z", 0),
            obs("c:\\apps\\game.exe", "2026-08-25T10:04:50Z", 0),
        ]),
    );
    // One bad observation must not fail the batch.
    expect_accepted(response, at("2026-08-25T10:05:00Z"));

    let db_guard = lock_db(&ctx.db);
    assert_eq!(
        db_guard
            .day_summary(DayKey(20260825))
            .expect("summary")
            .total_seconds,
        0,
        "future-dated observations credit nothing (and the surviving \
         singleton observation credits no time on its own)"
    );
}

#[test]
fn report_usage_dedupes_a_resent_batch_across_requests() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-25T10:05:00Z"), 0),
    );
    let batch = || {
        report_of(vec![
            obs("c:\\apps\\game.exe", "2026-08-25T10:00:00Z", 0),
            obs("c:\\apps\\game.exe", "2026-08-25T10:00:30Z", 0),
        ])
    };

    handle(&ctx, batch());
    // The reply to the first send was lost; the helper resends verbatim.
    handle(&ctx, batch());

    let mut db_guard = lock_db(&ctx.db);
    assert_eq!(
        used_secs_for_key(&mut db_guard, "c:\\apps\\game.exe", DayKey(20260825)),
        30,
        "a resent batch must be counted once"
    );
}

#[test]
fn report_usage_splits_credit_at_the_local_day_boundary_mid_batch() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-21T00:05:00Z"), 0),
    );

    handle(
        &ctx,
        report_of(vec![
            obs("c:\\apps\\game.exe", "2026-08-20T23:59:50Z", 0),
            obs("c:\\apps\\game.exe", "2026-08-21T00:00:20Z", 0),
        ]),
    );

    let mut db_guard = lock_db(&ctx.db);
    assert_eq!(
        used_secs_for_key(&mut db_guard, "c:\\apps\\game.exe", DayKey(20260820)),
        10,
        "the old day keeps its share"
    );
    assert_eq!(
        used_secs_for_key(&mut db_guard, "c:\\apps\\game.exe", DayKey(20260821)),
        20,
        "the new day starts accruing at the boundary"
    );
}

#[test]
fn report_usage_marks_tracking_available_in_status() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-25T10:05:00Z"), 0),
    );

    let Response::Status(before) = handle(&ctx, Request::Status) else {
        panic!("expected Status");
    };
    assert!(
        !before.tracking_available,
        "no reports and no self-sampling: tracking is down"
    );

    handle(
        &ctx,
        report_of(vec![obs("c:\\apps\\game.exe", "2026-08-25T10:04:50Z", 0)]),
    );

    let Response::Status(after) = handle(&ctx, Request::Status) else {
        panic!("expected Status");
    };
    assert!(
        after.tracking_available,
        "a fresh report means tracking works"
    );
}

/// Shared server boilerplate for the live-pipe tests.
#[cfg(windows)]
fn spawn_test_server(name: &'static str) -> IpcServerHandle {
    spawn(
        name,
        ServerDeps {
            db: Arc::new(Mutex::new(Db::open_in_memory().expect("db"))),
            status: StatusInfo {
                agent_version: "test".into(),
                tracker_backend: "fake".into(),
                enforcement_backend: "fake".into(),
                filter_backend: "none".into(),
                self_sampling: false,
                blocks_encrypted_dns: false,
            },
            policy: Arc::new(RwLock::new(Policy::default())),
            processes: Arc::new(Mutex::new(Box::<FakeProcesses>::default())),
            clock: Arc::new(TestClock::new(at("2026-08-20T12:00:00Z"), 0)),
            dns: Arc::new(NoDns),
            data_dir: None,
        },
    )
}

/// The concurrency contract: one connection, many frames. Legacy one-shot
/// clients exercise the same worker loop with a single frame.
#[test]
#[cfg(windows)]
fn a_persistent_connection_serves_many_frames_in_order() {
    let name: &'static str =
        Box::leak(format!("screentime_agent_test_{}", std::process::id()).into_boxed_str());
    let _server = spawn_test_server(name);

    let mut stream = st_ipc::transport::client_connect(name).expect("connect");
    for expected in ["pong", "status"] {
        let request = if expected == "pong" {
            Request::Ping
        } else {
            Request::Status
        };
        st_ipc::write_message(&mut stream, &request).expect("write");
        let response: Response = st_ipc::read_message(&mut stream).expect("read");
        match (&response, expected) {
            (Response::Pong, "pong") => {}
            (Response::Status(_), "status") => {}
            _ => panic!("frame {expected} answered with {response:?}"),
        }
    }
    drop(stream);

    // A legacy one-shot client still gets served after the persistent
    // connection above held a worker.
    let mut second = st_ipc::transport::client_connect(name).expect("reconnect");
    st_ipc::write_message(&mut second, &Request::Ping).expect("write");
    assert!(matches!(
        st_ipc::read_message::<_, Response>(&mut second),
        Ok(Response::Pong)
    ));
}

/// Malformed bytes must close that one connection without killing the
/// server or poisoning anything shared.
#[test]
fn read_errors_surface_as_closed_or_codec_failures_only() {
    // Directly pins the worker-loop classification the server relies on:
    // clean EOF is Closed; garbage frames are codec errors, never panics.
    let empty: &[u8] = &[];
    assert!(matches!(
        st_ipc::read_message::<_, Request>(&mut std::io::Cursor::new(empty)),
        Err(IpcError::Closed)
    ));
}

/// Regression for the deaf-agent incident: every connection is served by a
/// worker holding one slot, and before the SlotGuard fix a worker that
/// died unexpectedly (panic path) skipped its release — leaking capacity
/// until `acquire` blocked forever and no pipe instance ever existed
/// again. Flooding well past [`MAX_WORKERS`] malformed connections must
/// leave the server fully alive.
#[test]
#[cfg(windows)]
fn a_flood_of_broken_connections_cannot_exhaust_the_worker_pool() {
    let name: &'static str =
        Box::leak(format!("screentime_agent_test_flood_{}", std::process::id()).into_boxed_str());
    spawn_test_server(name);

    use std::io::Write;
    for _ in 0..(MAX_WORKERS * 3) {
        let mut stream = st_ipc::transport::client_connect(name).expect("connect");
        // Well-formed length prefix, nonsense body: the frame layer rejects
        // it and the worker closes the connection.
        stream.write_all(&4u32.to_le_bytes()).expect("write len");
        stream.write_all(b"junk").expect("write body");
        drop(stream);
    }

    let mut probe = st_ipc::transport::client_connect(name)
        .expect("server must still be listening after far more broken connections than workers");
    st_ipc::write_message(&mut probe, &Request::Ping).expect("write ping");
    assert!(matches!(
        st_ipc::read_message::<_, Response>(&mut probe),
        Ok(Response::Pong)
    ));
}

#[test]
fn schedule_and_allowlist_ipc_roundtrip() {
    let db = Db::open_in_memory().expect("db");
    let clock = TestClock::new(
        DateTime::parse_from_rfc3339("2026-03-30T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        0,
    );
    let ctx = test_ctx(db, clock);

    // 1. Initially empty schedules
    let res = handle(&ctx, Request::ListSchedules);
    match res {
        Response::Schedules(dto) => assert_eq!(dto.schedules.len(), 0),
        other => panic!("expected Schedules, got {other:?}"),
    }

    // 2. Create schedule
    let res = handle(
        &ctx,
        Request::CreateSchedule {
            name: "Bedtime".into(),
            weekday_mask: 127,
            start_minute: 1320,
            end_minute: 420,
        },
    );
    let sched_id = match res {
        Response::ScheduleCreated(dto) => {
            assert_eq!(dto.name, "Bedtime");
            assert_eq!(dto.start_minute, 1320);
            assert_eq!(dto.end_minute, 420);
            assert_eq!(dto.weekday_mask, 127);
            assert!(dto.enabled);
            dto.id
        }
        other => panic!("expected ScheduleCreated, got {other:?}"),
    };

    // 3. List schedules returns 1
    let res = handle(&ctx, Request::ListSchedules);
    match res {
        Response::Schedules(dto) => {
            assert_eq!(dto.schedules.len(), 1);
            assert_eq!(dto.schedules[0].id, sched_id);
        }
        other => panic!("expected Schedules, got {other:?}"),
    }

    // 4. Update schedule
    let res = handle(
        &ctx,
        Request::UpdateSchedule {
            id: sched_id,
            name: "Deep Sleep".into(),
            weekday_mask: 31,
            start_minute: 1380,
            end_minute: 480,
            pin: None,
        },
    );
    assert!(matches!(res, Response::Accepted { .. }));

    // 5. Toggle enabled
    let res = handle(
        &ctx,
        Request::SetScheduleEnabled {
            id: sched_id,
            enabled: false,
            pin: None,
        },
    );
    assert!(matches!(res, Response::Accepted { .. }));

    // Verify updated state
    let res = handle(&ctx, Request::ListSchedules);
    match res {
        Response::Schedules(dto) => {
            assert_eq!(dto.schedules[0].name, "Deep Sleep");
            assert!(!dto.schedules[0].enabled);
        }
        other => panic!("expected Schedules, got {other:?}"),
    }

    // 6. Delete schedule
    let res = handle(
        &ctx,
        Request::DeleteSchedule {
            id: sched_id,
            pin: None,
        },
    );
    assert!(matches!(res, Response::Accepted { .. }));

    let res = handle(&ctx, Request::ListSchedules);
    match res {
        Response::Schedules(dto) => assert_eq!(dto.schedules.len(), 0),
        other => panic!("expected Schedules, got {other:?}"),
    }

    // 7. Allowlist roundtrip
    let res = handle(
        &ctx,
        Request::SetAllowlist {
            subject_type: "app".into(),
            subject_id: 42,
            allowed: true,
            pin: None,
        },
    );
    assert!(matches!(res, Response::Accepted { .. }));

    let res = handle(&ctx, Request::ListAllowlist);
    match res {
        Response::Allowlist(dto) => {
            assert_eq!(dto.items.len(), 1);
            assert_eq!(dto.items[0].subject_type, "app");
            assert_eq!(dto.items[0].subject_id, 42);
        }
        other => panic!("expected Allowlist, got {other:?}"),
    }

    // Remove from allowlist
    let res = handle(
        &ctx,
        Request::SetAllowlist {
            subject_type: "app".into(),
            subject_id: 42,
            allowed: false,
            pin: None,
        },
    );
    assert!(matches!(res, Response::Accepted { .. }));

    let res = handle(&ctx, Request::ListAllowlist);
    match res {
        Response::Allowlist(dto) => assert_eq!(dto.items.len(), 0),
        other => panic!("expected Allowlist, got {other:?}"),
    }
}
// -- Central authorization (auth.rs). -------------------------------------

fn ctx_with_pin(pin: &str) -> Ctx {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
    );
    expect_vault(handle(
        &ctx,
        Request::SetPin {
            new_pin: pin.into(),
            current_pin: None,
        },
    ));
    ctx
}

fn set_setting_req(key: &str, value: &str, pin: Option<&str>) -> Request {
    Request::SetSetting {
        key: key.into(),
        value: value.into(),
        pin: pin.map(Into::into),
    }
}

fn stored_setting(ctx: &Ctx, key: &str) -> Option<String> {
    lock_db(&ctx.db).setting(key).expect("read")
}

#[test]
fn set_setting_refuses_vault_and_unknown_keys_even_without_a_pin() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
    );
    for key in [
        "pin_hash",
        "recovery_hash",
        "original_dns_config",
        "nonsense",
    ] {
        assert_eq!(
            error_code(&handle(&ctx, set_setting_req(key, "x", None))),
            Some(ErrorCode::BadRequest),
            "{key}"
        );
        assert_eq!(stored_setting(&ctx, key), None, "{key} must not be written");
    }
}

#[test]
fn set_setting_validates_values() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
    );
    for (key, value) in [
        ("alert_volume", "250"),
        ("strict_mode", "yes"),
        ("limit_cooldown_hours", "-3"),
        ("day_start_minutes", "1440"),
    ] {
        assert_eq!(
            error_code(&handle(&ctx, set_setting_req(key, value, None))),
            Some(ErrorCode::BadRequest),
            "{key}={value}"
        );
    }
}

#[test]
fn loosening_settings_need_the_pin_and_tightening_ones_do_not() {
    let ctx = ctx_with_pin("1234");

    // Tightening: free.
    assert!(matches!(
        handle(&ctx, set_setting_req("strict_mode", "true", None)),
        Response::Accepted { .. }
    ));
    assert!(read_recover(&ctx.policy, "policy").strict_mode);
    assert!(matches!(
        handle(&ctx, set_setting_req("limit_cooldown_hours", "48", None)),
        Response::Accepted { .. }
    ));

    // Loosening without the PIN: refused, nothing changes.
    for (key, value) in [
        ("strict_mode", "false"),
        ("limit_cooldown_hours", "0"),
        ("day_start_minutes", "240"),
        ("idle_threshold_secs", "30"),
    ] {
        assert_eq!(
            error_code(&handle(&ctx, set_setting_req(key, value, None))),
            Some(ErrorCode::BadPin),
            "{key}={value}"
        );
    }
    assert!(read_recover(&ctx.policy, "policy").strict_mode);
    assert_eq!(read_recover(&ctx.policy, "policy").limit_cooldown_hours, 48);

    // With the PIN: applied, to storage and to the live policy.
    assert!(matches!(
        handle(
            &ctx,
            set_setting_req("limit_cooldown_hours", "0", Some("1234"))
        ),
        Response::Accepted { .. }
    ));
    assert_eq!(read_recover(&ctx.policy, "policy").limit_cooldown_hours, 0);
    assert_eq!(
        stored_setting(&ctx, "limit_cooldown_hours").as_deref(),
        Some("0")
    );
}

#[test]
fn profile_is_free_before_a_pin_exists_and_gated_after() {
    // First run: no PIN yet, so setup can record who Tether is for.
    let fresh = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
    );
    assert!(matches!(
        handle(&fresh, set_setting_req("profile", "guardian", None)),
        Response::Accepted { .. }
    ));
    assert_eq!(read_recover(&fresh.policy, "policy").profile, "guardian");
    assert_eq!(
        error_code(&handle(&fresh, set_setting_req("profile", "parent", None))),
        Some(ErrorCode::BadRequest)
    );

    // With a PIN, switching profile either way needs it.
    let ctx = ctx_with_pin("1234");
    assert_eq!(
        error_code(&handle(&ctx, set_setting_req("profile", "guardian", None))),
        Some(ErrorCode::BadPin)
    );
    assert!(matches!(
        handle(&ctx, set_setting_req("profile", "guardian", Some("1234"))),
        Response::Accepted { .. }
    ));
    match handle(&ctx, Request::Status) {
        Response::Status(status) => assert_eq!(status.profile, "guardian"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn cosmetic_settings_never_need_the_pin() {
    let ctx = ctx_with_pin("1234");
    for (key, value) in [
        ("alert_volume", "40"),
        ("show_hud_overlay", "false"),
        ("hud_peek_hotkey", "Ctrl+Alt+P"),
    ] {
        assert!(
            matches!(
                handle(&ctx, set_setting_req(key, value, None)),
                Response::Accepted { .. }
            ),
            "{key}"
        );
    }
    assert_eq!(read_recover(&ctx.policy, "policy").alert_volume, 40);
}

#[test]
fn categorize_and_schedule_edits_need_the_pin() {
    let ctx = ctx_with_pin("1234");
    let app = seed_game_app(&mut lock_db(&ctx.db), "C:\\games\\steam\\steam.exe");
    let education = lock_db(&ctx.db).category_id("education").expect("cat");

    let categorize = |pin: Option<&str>| Request::Categorize {
        app_id: app,
        primary: Some(education),
        tags: vec![],
        pin: pin.map(Into::into),
    };
    assert_eq!(
        error_code(&handle(&ctx, categorize(None))),
        Some(ErrorCode::BadPin)
    );
    assert!(matches!(
        handle(&ctx, categorize(Some("1234"))),
        Response::Accepted { .. }
    ));

    // Creating a schedule tightens: free.
    let Response::ScheduleCreated(created) = handle(
        &ctx,
        Request::CreateSchedule {
            name: "Bedtime".into(),
            weekday_mask: 0x7f,
            start_minute: 22 * 60,
            end_minute: 7 * 60,
        },
    ) else {
        panic!("expected ScheduleCreated");
    };

    // Enabling is free; disabling, editing and deleting need the PIN.
    let toggle = |enabled: bool, pin: Option<&str>| Request::SetScheduleEnabled {
        id: created.id,
        enabled,
        pin: pin.map(Into::into),
    };
    assert!(matches!(
        handle(&ctx, toggle(true, None)),
        Response::Accepted { .. }
    ));
    assert_eq!(
        error_code(&handle(&ctx, toggle(false, None))),
        Some(ErrorCode::BadPin)
    );
    assert_eq!(
        error_code(&handle(
            &ctx,
            Request::UpdateSchedule {
                id: created.id,
                name: "Bedtime".into(),
                weekday_mask: 0x01,
                start_minute: 0,
                end_minute: 1,
                pin: None,
            }
        )),
        Some(ErrorCode::BadPin)
    );
    assert_eq!(
        error_code(&handle(
            &ctx,
            Request::DeleteSchedule {
                id: created.id,
                pin: None,
            }
        )),
        Some(ErrorCode::BadPin)
    );
    assert!(matches!(
        handle(
            &ctx,
            Request::DeleteSchedule {
                id: created.id,
                pin: Some("1234".into()),
            }
        ),
        Response::Accepted { .. }
    ));
}

#[test]
fn allowlisting_needs_the_pin_but_removing_from_the_allowlist_does_not() {
    let ctx = ctx_with_pin("1234");
    let app = seed_game_app(&mut lock_db(&ctx.db), "C:\\games\\steam\\steam.exe");
    let allow = |allowed: bool, pin: Option<&str>| Request::SetAllowlist {
        subject_type: "app".into(),
        subject_id: app,
        allowed,
        pin: pin.map(Into::into),
    };
    assert_eq!(
        error_code(&handle(&ctx, allow(true, None))),
        Some(ErrorCode::BadPin)
    );
    assert!(matches!(
        handle(&ctx, allow(true, Some("1234"))),
        Response::Accepted { .. }
    ));
    assert!(matches!(
        handle(&ctx, allow(false, None)),
        Response::Accepted { .. }
    ));
}

#[test]
fn repeated_wrong_pins_lock_out_even_the_right_pin_until_expiry() {
    let ctx = ctx_with_pin("1234");
    let loosen = |pin: &str| set_setting_req("limit_cooldown_hours", "1", Some(pin));

    // Empty PINs are "not supplied" and never count.
    for _ in 0..20 {
        assert_eq!(
            error_code(&handle(&ctx, loosen(""))),
            Some(ErrorCode::BadPin)
        );
    }
    for _ in 0..st_core::pin::THROTTLE_FREE_ATTEMPTS {
        assert_eq!(
            error_code(&handle(&ctx, loosen("0000"))),
            Some(ErrorCode::BadPin)
        );
    }
    // One more wrong attempt trips the lockout...
    assert_eq!(
        error_code(&handle(&ctx, loosen("0000"))),
        Some(ErrorCode::BadPin)
    );
    // ...after which even the right PIN is refused.
    assert_eq!(
        error_code(&handle(&ctx, loosen("1234"))),
        Some(ErrorCode::RateLimited)
    );
    // The vault endpoints share the same throttle.
    assert_eq!(
        error_code(&handle(
            &ctx,
            Request::RemovePin {
                credential: "1234".into()
            }
        )),
        Some(ErrorCode::RateLimited)
    );

    // Same live state (throttle), later wall clock: the lockout expired.
    let later = Ctx {
        clock: Arc::new(TestClock::new(
            at("2026-08-20T12:00:00Z")
                + Duration::seconds(st_core::pin::THROTTLE_BASE_LOCKOUT_SECS + 1),
            0,
        )),
        ..ctx.clone()
    };
    assert!(matches!(
        handle(&later, loosen("1234")),
        Response::Accepted { .. }
    ));
}

#[test]
fn rejected_credentials_are_audited() {
    let ctx = ctx_with_pin("1234");
    let _ = handle(
        &ctx,
        set_setting_req("limit_cooldown_hours", "1", Some("9999")),
    );
    let count: i64 = lock_db(&ctx.db)
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE kind = 'credential_rejected'",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(count, 1);
}

#[test]
fn overrides_in_strict_mode_are_refused_before_the_pin_is_spent() {
    let ctx = ctx_with_pin("1234");
    assert!(matches!(
        handle(&ctx, set_setting_req("strict_mode", "true", None)),
        Response::Accepted { .. }
    ));
    for _ in 0..10 {
        assert_eq!(
            error_code(&handle(
                &ctx,
                Request::GrantOverride {
                    target: LimitTargetDto::Total,
                    seconds: 900,
                    pin: "0000".into(),
                }
            )),
            Some(ErrorCode::StrictMode)
        );
    }
    // No failures were recorded, so the right PIN still works elsewhere.
    assert!(matches!(
        handle(
            &ctx,
            set_setting_req("limit_cooldown_hours", "1", Some("1234"))
        ),
        Response::Accepted { .. }
    ));
}

#[test]
fn manual_blocks_normalise_dedupe_and_remove_every_copy() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
    );
    let add = |d: &str| handle(&ctx, Request::AddManualBlock { domain: d.into() });
    assert!(matches!(add("Example.COM."), Response::Accepted { .. }));
    assert!(matches!(add("example.com"), Response::Accepted { .. }));
    assert_eq!(
        error_code(&add("not a domain!")),
        Some(ErrorCode::BadRequest)
    );
    let Response::ManualBlocks { domains } = handle(&ctx, Request::ListManualBlocks) else {
        panic!("expected ManualBlocks");
    };
    assert_eq!(domains, ["example.com"], "stored once, normalised");

    // A duplicate written by an older version is removed too.
    lock_db(&ctx.db)
        .add_block_rule(None, None, "example.com", true, "block")
        .expect("legacy duplicate");
    let remove = |d: &str| {
        handle(
            &ctx,
            Request::RemoveManualBlock {
                domain: d.into(),
                pin: String::new(),
            },
        )
    };
    assert!(matches!(remove("EXAMPLE.com"), Response::Accepted { .. }));
    let Response::ManualBlocks { domains } = handle(&ctx, Request::ListManualBlocks) else {
        panic!("expected ManualBlocks");
    };
    assert!(domains.is_empty());
    assert_eq!(
        error_code(&remove("example.com")),
        Some(ErrorCode::NotFound)
    );
}

#[test]
fn verify_pin_checks_against_the_vault() {
    let ctx = ctx_with_pin("1234");
    let verify = |pin: &str| handle(&ctx, Request::VerifyPin { pin: pin.into() });
    assert_eq!(error_code(&verify("")), Some(ErrorCode::BadPin));
    assert_eq!(error_code(&verify("0000")), Some(ErrorCode::BadPin));
    assert!(matches!(verify("1234"), Response::Accepted { .. }));
}

#[test]
fn day_summary_budgets_honour_weekday_overrides_disabled_limits_and_categories() {
    let mut db = Db::open_in_memory().expect("db");
    let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
    let games = db.category_id("games").expect("games");
    // 2026-08-22 is a Saturday (Monday-first index 5).
    record_usage(&mut db, app, 600, DayKey(20260822));
    let mut weekend = Limit::new(0, LimitTarget::App(app), 30);
    weekend.weekday_minutes[5] = Some(90);
    db.upsert_limit(&weekend, at("2026-08-22T09:00:00Z"))
        .expect("app limit");
    let mut disabled = Limit::new(0, LimitTarget::Category(games), 60);
    disabled.enabled = false;
    db.upsert_limit(&disabled, at("2026-08-22T09:00:00Z"))
        .expect("category limit");

    let ctx = test_ctx(db, TestClock::new(at("2026-08-22T12:00:00Z"), 0));
    let summary = |ctx: &Ctx| {
        let Response::DaySummary(dto) = handle(
            ctx,
            Request::DaySummary {
                day: DayKey(20260822),
            },
        ) else {
            panic!("expected DaySummary");
        };
        dto
    };
    let dto = summary(&ctx);
    assert_eq!(
        dto.apps[0].limit_seconds,
        Some(90 * 60),
        "Saturday override"
    );
    assert_eq!(dto.categories[0].limit_seconds, None, "disabled limit");

    let mut enabled = Limit::new(0, LimitTarget::Category(games), 60);
    enabled.enabled = true;
    lock_db(&ctx.db)
        .upsert_limit(&enabled, at("2026-08-22T09:00:00Z"))
        .expect("enable");
    let dto = summary(&ctx);
    let row = &dto.categories[0];
    assert_eq!(
        row.limit_seconds.expect("budget") - row.seconds,
        60 * 60 - 600,
        "budget minus usage is the time left"
    );
}

#[test]
fn quitting_a_blocked_app_needs_no_pin() {
    let mut db = Db::open_in_memory().expect("db");
    let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
    db.set_pin_hash(&hash_pin("9999").expect("hash"))
        .expect("set pin");
    db.set_block(
        SubjectRef::App(app),
        "limit",
        at("2026-08-20T11:00:00Z"),
        Some(at("2026-08-21T00:00:00Z")),
    )
    .expect("block");
    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    let response = handle(
        &ctx,
        Request::CloseApps {
            app_id: app,
            pin: String::new(),
        },
    );
    assert!(
        matches!(response, Response::Accepted { .. }),
        "{response:?}"
    );
}

#[test]
fn usage_reports_from_other_programs_are_refused() {
    let ctx = test_ctx(
        Db::open_in_memory().expect("db"),
        TestClock::new(at("2026-08-20T12:00:00Z"), 0),
    );
    let report = report_of(vec![obs("C:\\x\\a.exe", "2026-08-20T11:59:58Z", 0)]);
    assert_eq!(
        error_code(&handle_from(&ctx, auth::Peer::Other, report.clone())),
        Some(ErrorCode::BadRequest)
    );
    // Uninspectable peers are let through so tracking never silently stops.
    assert!(matches!(
        handle_from(&ctx, auth::Peer::Unknown, report),
        Response::Accepted { .. }
    ));
    assert_eq!(
        error_code(&handle_from(
            &ctx,
            auth::Peer::Other,
            Request::RegisterDiscoveredApps { apps: vec![] }
        )),
        Some(ErrorCode::BadRequest)
    );
}

#[test]
fn block_reasons_are_recorded_for_blocked_apps_without_a_pin_and_counted_by_day() {
    let mut db = Db::open_in_memory().expect("db");
    let blocked = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
    let free = seed_game_app(&mut db, "C:\\games\\other\\other.exe");
    db.set_block(
        SubjectRef::App(blocked),
        "limit",
        at("2026-08-20T12:00:00Z"),
        None,
    )
    .expect("block");
    let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
    // A PIN is set: recording a reason must still not ask for it.
    expect_vault(handle(
        &ctx,
        Request::SetPin {
            new_pin: "4826".into(),
            current_pin: None,
        },
    ));

    let record = |app_id, reason: &str| {
        handle(
            &ctx,
            Request::RecordBlockReason {
                app_id,
                reason: reason.into(),
            },
        )
    };
    expect_accepted(record(blocked, "finish"), at("2026-08-20T12:00:00Z"));
    // Answering again the same day replaces the answer.
    expect_accepted(record(blocked, "habit"), at("2026-08-20T12:00:00Z"));

    // Only a blocked app has a block screen to answer from.
    match record(free, "bored") {
        Response::Error { code, .. } => assert_eq!(code, ErrorCode::NotFound),
        other => panic!("expected not_found, got {other:?}"),
    }
    match record(blocked, "because") {
        Response::Error { code, .. } => assert_eq!(code, ErrorCode::BadRequest),
        other => panic!("expected bad_request, got {other:?}"),
    }

    let day = DayKey(20260820);
    match handle(
        &ctx,
        Request::BlockReasons {
            from_day: day,
            to_day: day,
        },
    ) {
        Response::BlockReasons(dto) => {
            assert_eq!(dto.counts.len(), 1);
            assert_eq!(dto.counts[0].reason, "habit");
            assert_eq!(dto.counts[0].count, 1);
        }
        other => panic!("expected counts, got {other:?}"),
    }
    // Backwards or absurd ranges are refused.
    match handle(
        &ctx,
        Request::BlockReasons {
            from_day: day,
            to_day: DayKey(20260801),
        },
    ) {
        Response::Error { code, .. } => assert_eq!(code, ErrorCode::BadRequest),
        other => panic!("expected bad_request, got {other:?}"),
    }
}
