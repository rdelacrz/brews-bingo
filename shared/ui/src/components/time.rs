//! Fixed UTC timestamps; retention is supplied by the server, never recomputed.
use crate::client;
use dioxus::prelude::*;

pub(crate) fn use_clock() -> Signal<i64> {
    #[allow(unused_mut)]
    let mut clock = use_signal(client::now_ms);
    use_future(move || async move {
        #[cfg(target_arch = "wasm32")]
        loop {
            gloo_timers::future::TimeoutFuture::new(1_000).await;
            clock.set(client::now_ms());
        }
    });
    clock
}

pub(crate) fn utc_time(milliseconds: i64) -> String {
    let seconds = milliseconds.div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    // Gregorian civil date conversion with the Unix epoch as day zero.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        day_seconds / 3_600,
        day_seconds % 3_600 / 60,
        day_seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn displays_exact_utc_including_leap_days_and_epoch_edges() {
        assert_eq!(utc_time(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc_time(-1), "1969-12-31 23:59:59 UTC");
        assert_eq!(utc_time(1_709_164_800_000), "2024-02-29 00:00:00 UTC");
        assert_eq!(utc_time(1_735_689_599_000), "2024-12-31 23:59:59 UTC");
    }
}
