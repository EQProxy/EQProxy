//! IMF-fixdate for responses the dataplane generates itself.
//!
//! RFC 9110 §6.6.1 expects the generator of a response to send `Date`. Proxied
//! responses keep the origin's value; only the heads this crate assembles
//! (local errors, custom pages, redirects, auth denials, h2/h3 refusals and
//! gRPC local answers) call [`current_http_date`]. The format is the fixed
//! `Sun, 06 Nov 1994 08:49:37 GMT` of RFC 9110 §5.6.7; no dependency is
//! needed for one conversion from Unix seconds.

use std::time::{SystemTime, UNIX_EPOCH};

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `Date` for the current clock, in IMF-fixdate form.
pub(crate) fn current_http_date() -> String {
    http_date(SystemTime::now())
}

/// Formats `now` as an IMF-fixdate. A clock before the Unix epoch falls back
/// to the epoch: this header is never a reason to fail a response.
pub(crate) fn http_date(now: SystemTime) -> String {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let days = (seconds / 86_400) as i64;
    let second_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    // 1970-01-01 was a Thursday (index 4 with Sunday = 0).
    let weekday = (days + 4).rem_euclid(7) as usize;
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    let second = second_of_day % 60;
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        WEEKDAYS[weekday],
        day,
        MONTHS[month as usize - 1],
        year,
        hour,
        minute,
        second
    )
}

/// Civil date from days since 1970-01-01 (Howard Hinnant's `civil_from_days`).
/// Proleptic Gregorian; valid for the whole range this process can reach.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(seconds: u64) -> String {
        http_date(UNIX_EPOCH + Duration::from_secs(seconds))
    }

    #[test]
    fn the_epoch_is_a_thursday() {
        assert_eq!(at(0), "Thu, 01 Jan 1970 00:00:00 GMT");
    }

    #[test]
    fn the_rfc_example_is_formatted_exactly() {
        // RFC 9110 §5.6.7 uses this instant as its example.
        assert_eq!(at(784_111_777), "Sun, 06 Nov 1994 08:49:37 GMT");
    }

    #[test]
    fn a_leap_day_is_not_lost() {
        assert_eq!(at(1_709_164_800), "Thu, 29 Feb 2024 00:00:00 GMT");
    }

    #[test]
    fn a_recent_instant_matches_its_utc_date() {
        assert_eq!(at(1_700_000_000), "Tue, 14 Nov 2023 22:13:20 GMT");
        assert_eq!(at(2_000_000_000), "Wed, 18 May 2033 03:33:20 GMT");
    }

    #[test]
    fn a_clock_before_the_epoch_falls_back_instead_of_panicking() {
        let before = UNIX_EPOCH
            .checked_sub(Duration::from_secs(1))
            .expect("one second before the epoch");
        assert_eq!(http_date(before), "Thu, 01 Jan 1970 00:00:00 GMT");
    }
}
