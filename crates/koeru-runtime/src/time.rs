//! 台帳に入れる時刻の組み立て。

use std::time::{SystemTime, UNIX_EPOCH};

/// 現在時刻を RFC 3339 で。
///
/// 秒までで足りる。 台帳に入るのは順序を保つためで、精密な時刻ではない。
///
/// 素の算術で組む。 日付だけのために依存を増やさない。 `koeru-app` もここを使う
/// ——時計は1つだけ持つ。
#[must_use]
pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(i64::try_from(days).unwrap_or(0));
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// 1970-01-01 からの日数を年月日にする（Howard Hinnant の `civil_from_days`）。
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "西暦での妥当な範囲でしか使わない日付の算術"
)]
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 現在時刻を_rfc3339_で組める() {
        let s = now_rfc3339();
        assert_eq!(s.len(), "2026-09-28T00:00:00Z".len());
        assert!(s.ends_with('Z'));
    }

    #[test]
    fn 紀元の日は1970年1月1日() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }
}
