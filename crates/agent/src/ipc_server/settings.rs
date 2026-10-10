//! `SetSetting`: validated keys applied to storage and the live policy.

use super::*;

/// Apply one validated setting. `auth` has already rejected unknown keys,
/// invalid values and unauthorised loosening; parsing again here keeps the
/// handler self-contained and guarantees the stored value is canonical.
pub(super) fn set_setting(ctx: &Ctx, key: &str, value: &str) -> Response {
    let (key, value) = match auth::parse_setting(key, value) {
        Ok(parsed) => parsed,
        Err(denied) => return denied.into(),
    };
    let now = ctx.clock.now_utc();
    if key == SettingKey::FamilyDns {
        // OS work runs without the database lock; see `family_dns`.
        let data_dir = ctx.data_dir.as_deref();
        let result = if value == "true" {
            family_dns::enable(&ctx.db, &*ctx.dns, data_dir)
        } else {
            family_dns::disable(Some(&ctx.db), &*ctx.dns, data_dir)
        };
        return match result {
            Ok(()) => accepted(now),
            Err(e) => error_internal(format!("family DNS: {e}")),
        };
    }
    if let Err(e) = lock_db(&ctx.db).set_setting(key.storage_key(), &value) {
        return error_internal(format!("save setting: {e}"));
    }
    // Window-title capture is read once by the tracker at startup; every
    // other key takes effect immediately through the shared policy.
    write_recover(&ctx.policy, "policy").apply(key, &value);
    accepted(now)
}
