//! Date conversion, locators, and formatters.
//!
//! Dates are represented as `f64` days since the Unix epoch (`1970-01-01
//! 00:00:00`). This matches matplotlib's default date-number convention while
//! keeping the axis interface numeric. The module is intentionally timezone-free:
//! conversions use [`chrono::NaiveDateTime`], and higher layers can decide how
//! to map user-facing timezone-aware values into that representation.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};

use crate::ticker::{Formatter, Locator};

const SECONDS_PER_DAY: f64 = 86_400.0;
const NANOS_PER_DAY: f64 = SECONDS_PER_DAY * 1_000_000_000.0;

/// Frequency selected by [`AutoDateLocator`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateFrequency {
    /// Calendar years.
    Year,
    /// Calendar months.
    Month,
    /// Calendar days.
    Day,
    /// Clock hours.
    Hour,
    /// Clock minutes.
    Minute,
    /// Clock seconds.
    Second,
}

/// Convert a naive date-time to floating days since `1970-01-01 00:00:00`.
#[must_use]
pub fn date2num(dt: NaiveDateTime) -> f64 {
    let utc = dt.and_utc();
    utc.timestamp() as f64 / SECONDS_PER_DAY + utc.timestamp_subsec_nanos() as f64 / NANOS_PER_DAY
}

/// Convert a date to floating days at midnight.
#[must_use]
pub fn date_to_num(date: NaiveDate) -> f64 {
    date2num(date.and_hms_opt(0, 0, 0).expect("midnight is valid"))
}

/// Convert floating days since the Unix epoch to a naive date-time.
///
/// Fractional days are rounded to the nearest nanosecond. Values outside
/// chrono's representable range panic.
#[must_use]
pub fn num2date(value: f64) -> NaiveDateTime {
    assert!(value.is_finite(), "date value must be finite");
    let total_nanos = (value * NANOS_PER_DAY).round();
    assert!(
        total_nanos >= i64::MIN as f64 && total_nanos <= i64::MAX as f64,
        "date value is outside the supported range"
    );
    epoch() + Duration::nanoseconds(total_nanos as i64)
}

/// A coarse-to-fine automatic date tick locator.
///
/// The locator chooses a frequency from year/month/day/hour/minute/second and a
/// nice interval for that frequency so the visible tick count stays at or below
/// `maxticks` when possible.
#[derive(Clone, Debug)]
pub struct AutoDateLocator {
    maxticks: usize,
}

impl AutoDateLocator {
    /// Construct a locator with matplotlib-like default density.
    #[must_use]
    pub fn new() -> Self {
        Self { maxticks: 8 }
    }

    /// Construct a locator with an explicit maximum target tick count.
    #[must_use]
    pub fn with_maxticks(maxticks: usize) -> Self {
        Self {
            maxticks: maxticks.max(2),
        }
    }

    /// Select the frequency and interval for a view range.
    #[must_use]
    pub fn select_interval(&self, vmin: f64, vmax: f64) -> (DateFrequency, i32) {
        let span_days = (vmax - vmin).abs().max(1.0 / SECONDS_PER_DAY);
        let candidates = [
            (
                DateFrequency::Year,
                span_days / 365.2425,
                &[1, 2, 5, 10, 20, 50, 100][..],
            ),
            (
                DateFrequency::Month,
                span_days / 30.436875,
                &[1, 2, 3, 4, 6][..],
            ),
            (DateFrequency::Day, span_days, &[1, 2, 3, 7, 14][..]),
            (
                DateFrequency::Hour,
                span_days * 24.0,
                &[1, 2, 3, 4, 6, 12][..],
            ),
            (
                DateFrequency::Minute,
                span_days * 1_440.0,
                &[1, 5, 10, 15, 30][..],
            ),
            (
                DateFrequency::Second,
                span_days * 86_400.0,
                &[1, 5, 10, 15, 30][..],
            ),
        ];

        for (frequency, units, intervals) in candidates {
            if units < 1.0 && frequency != DateFrequency::Second {
                continue;
            }
            for &interval in intervals {
                if (units / interval as f64).ceil() + 1.0 <= self.maxticks as f64 {
                    return (frequency, interval);
                }
            }
        }

        (DateFrequency::Second, 30)
    }
}

impl Default for AutoDateLocator {
    fn default() -> Self {
        Self::new()
    }
}

impl Locator for AutoDateLocator {
    fn tick_values(&self, vmin: f64, vmax: f64) -> Vec<f64> {
        if !vmin.is_finite() || !vmax.is_finite() {
            return Vec::new();
        }
        let (lo, hi, reversed) = ordered(vmin, vmax);
        let (frequency, interval) = self.select_interval(lo, hi);
        let mut ticks = match frequency {
            DateFrequency::Year => year_ticks(lo, hi, interval),
            DateFrequency::Month => month_ticks(lo, hi, interval),
            DateFrequency::Day => fixed_day_ticks(lo, hi, interval as f64),
            DateFrequency::Hour => fixed_second_ticks(lo, hi, i64::from(interval) * 3_600),
            DateFrequency::Minute => fixed_second_ticks(lo, hi, i64::from(interval) * 60),
            DateFrequency::Second => fixed_second_ticks(lo, hi, i64::from(interval)),
        };
        if reversed {
            ticks.reverse();
        }
        ticks
    }

    fn view_limits(&self, vmin: f64, vmax: f64) -> (f64, f64) {
        if vmin == vmax {
            let pad = 1.0;
            (vmin - pad, vmax + pad)
        } else {
            (vmin, vmax)
        }
    }
}

/// Automatic minor tick locator for date axes.
///
/// This is the minor-tick companion to [`AutoDateLocator`]. It inspects the
/// major locator's selected frequency and chooses one finer subdivision: months
/// between yearly majors, days between monthly majors, hours between daily
/// majors, minutes between hourly majors, and seconds between minute majors.
/// Ticks coincident with the major locator are removed.
#[derive(Clone, Debug)]
pub struct AutoDateMinorLocator {
    major: AutoDateLocator,
}

impl AutoDateMinorLocator {
    /// Construct a minor locator using [`AutoDateLocator`]'s default density.
    #[must_use]
    pub fn new() -> Self {
        Self {
            major: AutoDateLocator::new(),
        }
    }

    /// Construct a minor locator paired with a major locator target density.
    #[must_use]
    pub fn with_maxticks(maxticks: usize) -> Self {
        Self {
            major: AutoDateLocator::with_maxticks(maxticks),
        }
    }

    /// Construct a minor locator from an explicit major locator.
    #[must_use]
    pub fn from_major(major: AutoDateLocator) -> Self {
        Self { major }
    }
}

impl Default for AutoDateMinorLocator {
    fn default() -> Self {
        Self::new()
    }
}

impl Locator for AutoDateMinorLocator {
    fn tick_values(&self, vmin: f64, vmax: f64) -> Vec<f64> {
        if !vmin.is_finite() || !vmax.is_finite() {
            return Vec::new();
        }
        let (lo, hi, reversed) = ordered(vmin, vmax);
        let (major_frequency, major_interval) = self.major.select_interval(lo, hi);
        let Some((minor_frequency, minor_interval)) =
            minor_interval(major_frequency, major_interval)
        else {
            return Vec::new();
        };

        let mut ticks = match minor_frequency {
            DateFrequency::Year => year_ticks(lo, hi, minor_interval),
            DateFrequency::Month => month_ticks(lo, hi, minor_interval),
            DateFrequency::Day => fixed_day_ticks(lo, hi, minor_interval as f64),
            DateFrequency::Hour => fixed_second_ticks(lo, hi, i64::from(minor_interval) * 3_600),
            DateFrequency::Minute => fixed_second_ticks(lo, hi, i64::from(minor_interval) * 60),
            DateFrequency::Second => fixed_second_ticks(lo, hi, i64::from(minor_interval)),
        };
        let major_ticks = self.major.tick_values(lo, hi);
        ticks.retain(|tick| !major_ticks.iter().any(|major| same_tick(*tick, *major)));
        if reversed {
            ticks.reverse();
        }
        ticks
    }

    fn view_limits(&self, vmin: f64, vmax: f64) -> (f64, f64) {
        self.major.view_limits(vmin, vmax)
    }
}

/// `strftime`-based date formatter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateFormatter {
    fmt: String,
}

impl DateFormatter {
    /// Construct a date formatter from a chrono/strftime format string.
    #[must_use]
    pub fn new(fmt: impl Into<String>) -> Self {
        Self { fmt: fmt.into() }
    }
}

impl Formatter for DateFormatter {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        num2date(value).format(&self.fmt).to_string()
    }
}

/// Compact date formatter that suppresses repeated higher-order fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConciseDateFormatter;

impl ConciseDateFormatter {
    /// Construct a concise date formatter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Format a whole tick vector with context from neighboring ticks.
    #[must_use]
    pub fn format_ticks(&self, ticks: &[f64]) -> Vec<String> {
        let dates: Vec<_> = ticks.iter().map(|&tick| num2date(tick)).collect();
        dates
            .iter()
            .enumerate()
            .map(|(i, dt)| concise_label(dates.get(i.wrapping_sub(1)), dt))
            .collect()
    }
}

impl Formatter for ConciseDateFormatter {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        let dt = num2date(value);
        if dt.time().num_seconds_from_midnight() == 0 {
            dt.format("%Y-%m-%d").to_string()
        } else {
            dt.format("%H:%M:%S").to_string()
        }
    }
}

fn epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .expect("epoch date is valid")
        .and_hms_opt(0, 0, 0)
        .expect("epoch time is valid")
}

fn ordered(vmin: f64, vmax: f64) -> (f64, f64, bool) {
    if vmin <= vmax {
        (vmin, vmax, false)
    } else {
        (vmax, vmin, true)
    }
}

fn year_ticks(lo: f64, hi: f64, interval: i32) -> Vec<f64> {
    let start = num2date(lo).year().div_euclid(interval) * interval;
    let end = num2date(hi).year();
    let mut ticks = Vec::new();
    let mut year = start;
    while year <= end + interval {
        if let Some(dt) = date_time(year, 1, 1, 0, 0, 0) {
            push_if_in_range(&mut ticks, date2num(dt), lo, hi);
        }
        year += interval;
    }
    ticks
}

fn month_ticks(lo: f64, hi: f64, interval: i32) -> Vec<f64> {
    let start = num2date(lo);
    let end = num2date(hi);
    let start_month = start.year() * 12 + start.month0() as i32;
    let end_month = end.year() * 12 + end.month0() as i32;
    let mut month_index = start_month.div_euclid(interval) * interval;
    let mut ticks = Vec::new();

    while month_index <= end_month + interval {
        let year = month_index.div_euclid(12);
        let month0 = month_index.rem_euclid(12);
        if let Some(dt) = date_time(year, (month0 + 1) as u32, 1, 0, 0, 0) {
            push_if_in_range(&mut ticks, date2num(dt), lo, hi);
        }
        month_index += interval;
    }
    ticks
}

fn fixed_day_ticks(lo: f64, hi: f64, interval_days: f64) -> Vec<f64> {
    let mut tick = (lo / interval_days).floor() * interval_days;
    let mut ticks = Vec::new();
    while tick <= hi + interval_days {
        push_if_in_range(&mut ticks, tick, lo, hi);
        tick += interval_days;
    }
    ticks
}

fn fixed_second_ticks(lo: f64, hi: f64, interval_seconds: i64) -> Vec<f64> {
    let lo_sec = (lo * SECONDS_PER_DAY).floor() as i64;
    let hi_sec = (hi * SECONDS_PER_DAY).ceil() as i64;
    let mut sec = lo_sec.div_euclid(interval_seconds) * interval_seconds;
    let mut ticks = Vec::new();
    while sec <= hi_sec + interval_seconds {
        push_if_in_range(&mut ticks, sec as f64 / SECONDS_PER_DAY, lo, hi);
        sec += interval_seconds;
    }
    ticks
}

fn push_if_in_range(ticks: &mut Vec<f64>, tick: f64, lo: f64, hi: f64) {
    let eps = 1e-10;
    if tick >= lo - eps && tick <= hi + eps {
        ticks.push(tick);
    }
}

fn same_tick(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-10
}

fn minor_interval(frequency: DateFrequency, interval: i32) -> Option<(DateFrequency, i32)> {
    match frequency {
        DateFrequency::Year => {
            if interval <= 1 {
                Some((DateFrequency::Month, 1))
            } else {
                Some((DateFrequency::Year, 1))
            }
        }
        DateFrequency::Month => {
            if interval <= 1 {
                Some((DateFrequency::Day, 7))
            } else {
                Some((DateFrequency::Month, 1))
            }
        }
        DateFrequency::Day => {
            if interval <= 1 {
                Some((DateFrequency::Hour, 6))
            } else {
                Some((DateFrequency::Day, 1))
            }
        }
        DateFrequency::Hour => {
            if interval <= 1 {
                Some((DateFrequency::Minute, 15))
            } else {
                Some((DateFrequency::Hour, 1))
            }
        }
        DateFrequency::Minute => {
            if interval <= 1 {
                Some((DateFrequency::Second, 15))
            } else {
                Some((DateFrequency::Minute, 1))
            }
        }
        DateFrequency::Second => {
            if interval > 1 {
                Some((DateFrequency::Second, 1))
            } else {
                None
            }
        }
    }
}

fn date_time(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<NaiveDateTime> {
    NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second)
}

fn concise_label(previous: Option<&NaiveDateTime>, dt: &NaiveDateTime) -> String {
    let Some(prev) = previous else {
        return dt.format("%Y-%m-%d").to_string();
    };

    if dt.year() != prev.year() {
        dt.format("%Y").to_string()
    } else if dt.month() != prev.month() {
        dt.format("%b").to_string()
    } else if dt.day() != prev.day() {
        dt.format("%d").to_string()
    } else if dt.hour() != prev.hour() || dt.minute() != prev.minute() {
        dt.format("%H:%M").to_string()
    } else {
        dt.format("%H:%M:%S").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> NaiveDateTime {
        date_time(year, month, day, hour, minute, second).expect("test date is valid")
    }

    #[test]
    fn date_round_trip_preserves_seconds() {
        let original = dt(2026, 6, 30, 12, 34, 56);
        let value = date2num(original);
        let roundtrip = num2date(value);

        assert_eq!(roundtrip, original);
    }

    #[test]
    fn epoch_is_zero_and_dates_are_whole_days() {
        assert_eq!(date2num(epoch()), 0.0);
        assert_eq!(
            date_to_num(NaiveDate::from_ymd_opt(1970, 1, 2).unwrap()),
            1.0
        );
    }

    #[test]
    fn auto_locator_selects_months_for_subyear_range() {
        let locator = AutoDateLocator::new();
        let start = date2num(dt(2026, 1, 1, 0, 0, 0));
        let end = date2num(dt(2026, 7, 1, 0, 0, 0));

        assert_eq!(
            locator.select_interval(start, end),
            (DateFrequency::Month, 1)
        );
        let labels: Vec<_> = locator
            .tick_values(start, end)
            .into_iter()
            .map(num2date)
            .map(|dt| (dt.year(), dt.month(), dt.day()))
            .collect();
        assert_eq!(
            labels,
            [
                (2026, 1, 1),
                (2026, 2, 1),
                (2026, 3, 1),
                (2026, 4, 1),
                (2026, 5, 1),
                (2026, 6, 1),
                (2026, 7, 1),
            ]
        );
    }

    #[test]
    fn auto_locator_selects_hours_for_intraday_range() {
        let locator = AutoDateLocator::with_maxticks(7);
        let start = date2num(dt(2026, 6, 30, 0, 0, 0));
        let end = date2num(dt(2026, 6, 30, 12, 0, 0));

        assert_eq!(
            locator.select_interval(start, end),
            (DateFrequency::Hour, 2)
        );
        let ticks = locator.tick_values(start, end);
        assert_eq!(ticks.len(), 7);
        assert_eq!(num2date(ticks[1]).hour(), 2);
    }

    #[test]
    fn auto_locator_handles_reversed_ranges() {
        let locator = AutoDateLocator::with_maxticks(5);
        let start = date2num(dt(2026, 1, 1, 0, 0, 0));
        let end = date2num(dt(2026, 1, 5, 0, 0, 0));
        let ticks = locator.tick_values(end, start);

        assert!(ticks.windows(2).all(|window| window[0] > window[1]));
    }

    #[test]
    fn auto_minor_locator_adds_months_between_years() {
        let locator = AutoDateMinorLocator::new();
        let start = date2num(dt(2026, 1, 1, 0, 0, 0));
        let end = date2num(dt(2028, 1, 1, 0, 0, 0));
        let labels: Vec<_> = locator
            .tick_values(start, end)
            .into_iter()
            .map(num2date)
            .map(|dt| (dt.year(), dt.month(), dt.day()))
            .collect();

        assert!(labels.contains(&(2026, 2, 1)));
        assert!(labels.contains(&(2027, 12, 1)));
        assert!(!labels.contains(&(2026, 1, 1)));
        assert!(!labels.contains(&(2027, 1, 1)));
        assert!(!labels.contains(&(2028, 1, 1)));
    }

    #[test]
    fn auto_minor_locator_adds_days_between_months() {
        let locator = AutoDateMinorLocator::new();
        let start = date2num(dt(2026, 1, 1, 0, 0, 0));
        let end = date2num(dt(2026, 3, 1, 0, 0, 0));
        let labels: Vec<_> = locator
            .tick_values(start, end)
            .into_iter()
            .map(num2date)
            .map(|dt| (dt.year(), dt.month(), dt.day()))
            .collect();

        assert!(labels.contains(&(2026, 1, 8)));
        assert!(labels.contains(&(2026, 2, 5)));
        assert!(!labels.contains(&(2026, 1, 1)));
        assert!(!labels.contains(&(2026, 2, 1)));
        assert!(!labels.contains(&(2026, 3, 1)));
    }

    #[test]
    fn auto_minor_locator_handles_reversed_ranges() {
        let locator = AutoDateMinorLocator::with_maxticks(7);
        let start = date2num(dt(2026, 6, 30, 0, 0, 0));
        let end = date2num(dt(2026, 6, 30, 12, 0, 0));
        let ticks = locator.tick_values(end, start);

        assert!(!ticks.is_empty());
        assert!(ticks.windows(2).all(|window| window[0] > window[1]));
    }

    #[test]
    fn date_formatter_uses_strftime_pattern() {
        let formatter = DateFormatter::new("%Y/%m/%d %H:%M");
        let value = date2num(dt(2026, 6, 30, 9, 5, 0));

        assert_eq!(formatter.format(value, None), "2026/06/30 09:05");
    }

    #[test]
    fn concise_formatter_suppresses_repeated_context() {
        let formatter = ConciseDateFormatter::new();
        let ticks = [
            date2num(dt(2026, 1, 1, 0, 0, 0)),
            date2num(dt(2026, 2, 1, 0, 0, 0)),
            date2num(dt(2026, 2, 2, 0, 0, 0)),
            date2num(dt(2026, 2, 2, 6, 0, 0)),
        ];

        assert_eq!(
            formatter.format_ticks(&ticks),
            ["2026-01-01", "Feb", "02", "06:00"]
        );
    }
}
