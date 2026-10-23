//! Le calendrier, en fonctions pures : intervalles, minuits, jours locaux.

use chrono::NaiveDate;

use crate::error::{Error, Result};

/// Un intervalle semi-ouvert `[start, end)`, en secondes depuis l'epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: i64,
    pub end: i64,
}

/// Deux intervalles se chevauchent-ils ? Des bornes qui se touchent ne comptent pas.
pub fn overlaps(a: Interval, b: Interval) -> bool {
    a.start < b.end && b.start < a.end
}

/// Le numéro du jour local (jours depuis le 1970-01-01 local) à l'instant `at`.
fn local_day(at: i64, offset: &dyn Fn(i64) -> i32) -> i64 {
    (at + i64::from(offset(at))).div_euclid(86_400)
}

/// La date locale (`AAAA-MM-JJ`) de l'instant `at`.
pub fn local_date(at: i64, offset: &dyn Fn(i64) -> i32) -> String {
    date_from_day(local_day(at, offset))
        .format("%Y-%m-%d")
        .to_string()
}

fn date_from_day(day: i64) -> NaiveDate {
    // 719_163 : jours entre le 0001-01-01 et le 1970-01-01.
    NaiveDate::from_num_days_from_ce_opt((day + 719_163) as i32)
        .expect("jour hors de la plage de chrono")
}

fn day_number(date: NaiveDate) -> i64 {
    i64::from(chrono::Datelike::num_days_from_ce(&date)) - 719_163
}

/// Le premier instant dont le jour local est au moins `day`. Le jour local
/// croît avec l'instant ; les décalages réels restent dans ±15 h, d'où la fenêtre.
fn first_instant_of(day: i64, offset: &dyn Fn(i64) -> i32) -> i64 {
    let (mut lo, mut hi) = (day * 86_400 - 15 * 3600, day * 86_400 + 15 * 3600);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if local_day(mid, offset) >= day {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}

/// Les limites d'un jour local : début inclus, minuit suivant exclu.
pub fn day_bounds(date: &str, offset: &dyn Fn(i64) -> i32) -> Result<Interval> {
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| Error::InvalidDate(date.to_string()))?;
    let day = day_number(parsed);
    Ok(Interval {
        start: first_instant_of(day, offset),
        end: first_instant_of(day + 1, offset),
    })
}

/// Découpe un intervalle à chaque minuit local qu'il traverse : un morceau par jour.
pub fn split_at_midnights(iv: Interval, offset: &dyn Fn(i64) -> i32) -> Vec<(String, Interval)> {
    let mut parts = Vec::new();
    let mut start = iv.start;
    while start < iv.end {
        let day = local_day(start, offset);
        let next = first_instant_of(day + 1, offset);
        let end = next.min(iv.end);
        parts.push((
            date_from_day(day).format("%Y-%m-%d").to_string(),
            Interval { start, end },
        ));
        start = end;
    }
    parts
}

/// Tous les minuits locaux `m` tels que `start < m <= end`, du plus ancien au plus récent.
pub fn midnights_between(start: i64, end: i64, offset: &dyn Fn(i64) -> i32) -> Vec<i64> {
    let mut out = Vec::new();
    if end <= start {
        return out;
    }
    let mut day = local_day(start, offset) + 1;
    loop {
        let midnight = first_instant_of(day, offset);
        if midnight > end {
            return out;
        }
        out.push(midnight);
        day += 1;
    }
}

/// Une durée au format du texte Monday : `2 h 10`, minutes sur deux chiffres, arrondies vers le bas.
pub fn format_duration(secs: i64) -> String {
    let secs = secs.max(0);
    format!("{} h {:02}", secs / 3600, (secs % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{Clock, FakeClock};
    use chrono::NaiveDate;

    fn ts(y: i32, m: u32, d: u32, h: u32, mi: u32) -> i64 {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
            .and_utc()
            .timestamp()
    }
    fn utc(_: i64) -> i32 {
        0
    }
    fn iv(start: i64, end: i64) -> Interval {
        Interval { start, end }
    }

    #[test]
    fn touching_intervals_do_not_overlap() {
        assert!(!overlaps(iv(0, 10), iv(10, 20)));
        assert!(!overlaps(iv(10, 20), iv(0, 10)));
    }

    #[test]
    fn contained_interval_overlaps() {
        assert!(overlaps(iv(0, 100), iv(20, 30)));
        assert!(overlaps(iv(20, 30), iv(0, 100)));
    }

    #[test]
    fn identical_intervals_overlap() {
        assert!(overlaps(iv(5, 9), iv(5, 9)));
    }

    #[test]
    fn split_over_one_midnight() {
        let parts = split_at_midnights(iv(ts(2026, 10, 23, 23, 0), ts(2026, 10, 24, 1, 0)), &utc);
        assert_eq!(
            parts,
            vec![
                (
                    "2026-10-23".to_string(),
                    iv(ts(2026, 10, 23, 23, 0), ts(2026, 10, 24, 0, 0))
                ),
                (
                    "2026-10-24".to_string(),
                    iv(ts(2026, 10, 24, 0, 0), ts(2026, 10, 24, 1, 0))
                ),
            ]
        );
    }

    #[test]
    fn split_over_three_midnights() {
        let parts = split_at_midnights(iv(ts(2026, 10, 20, 22, 0), ts(2026, 10, 23, 2, 0)), &utc);
        let dates: Vec<_> = parts.iter().map(|(d, _)| d.as_str()).collect();
        assert_eq!(
            dates,
            ["2026-10-20", "2026-10-21", "2026-10-22", "2026-10-23"]
        );
        assert_eq!(
            parts.iter().map(|(_, p)| p.end - p.start).sum::<i64>(),
            52 * 3600
        );
    }

    #[test]
    fn split_exactly_at_midnight_gives_one_piece() {
        let parts = split_at_midnights(iv(ts(2026, 10, 23, 22, 0), ts(2026, 10, 24, 0, 0)), &utc);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].0, "2026-10-23");
    }

    #[test]
    fn a_start_exactly_at_midnight_belongs_to_the_new_day() {
        assert_eq!(local_date(ts(2026, 10, 24, 0, 0), &utc), "2026-10-24");
        assert_eq!(local_date(ts(2026, 10, 24, 0, 0) - 1, &utc), "2026-10-23");
    }

    // Paris : CET (+1 h) puis CEST (+2 h) à 01:00 UTC le 29 mars 2026 ; retour à 01:00 UTC le 25 octobre.
    fn paris(at: i64) -> i32 {
        if at >= ts(2026, 3, 29, 1, 0) && at < ts(2026, 10, 25, 1, 0) {
            7200
        } else {
            3600
        }
    }

    #[test]
    fn dst_spring_day_is_23_hours() {
        let b = day_bounds("2026-03-29", &paris).unwrap();
        assert_eq!(b.end - b.start, 23 * 3600);
        assert_eq!(b.start, ts(2026, 3, 28, 23, 0));
    }

    #[test]
    fn dst_autumn_day_is_25_hours() {
        let b = day_bounds("2026-10-25", &paris).unwrap();
        assert_eq!(b.end - b.start, 25 * 3600);
        assert_eq!(b.start, ts(2026, 10, 24, 22, 0));
    }

    #[test]
    fn a_split_across_the_dst_night_keeps_each_day_exact() {
        let parts = split_at_midnights(iv(ts(2026, 3, 28, 22, 0), ts(2026, 3, 29, 23, 0)), &paris);
        // 28 mars (22:00 → 23:00 UTC), le 29 (23 h exactement), puis le 30 (22:00 → 23:00 UTC).
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].1.end, ts(2026, 3, 28, 23, 0));
        assert_eq!(parts[1].0, "2026-03-29");
        assert_eq!(parts[1].1.end - parts[1].1.start, 23 * 3600);
        assert_eq!(parts[2].0, "2026-03-30");
    }

    #[test]
    fn a_fake_clock_can_change_its_offset_over_time() {
        let clock = FakeClock::new(ts(2026, 3, 29, 0, 0), 3600).with_offset_fn(Box::new(paris));
        assert_eq!(clock.utc_offset(ts(2026, 3, 28, 12, 0)), 3600);
        assert_eq!(clock.utc_offset(ts(2026, 3, 30, 12, 0)), 7200);
        clock.advance(7200);
        assert_eq!(clock.now(), ts(2026, 3, 29, 2, 0));
        clock.set_now(5);
        assert_eq!(clock.now(), 5);
    }

    #[test]
    fn midnights_between_lists_every_midnight_in_the_range() {
        let m = midnights_between(ts(2026, 10, 20, 23, 30), ts(2026, 10, 23, 2, 0), &utc);
        assert_eq!(
            m,
            vec![
                ts(2026, 10, 21, 0, 0),
                ts(2026, 10, 22, 0, 0),
                ts(2026, 10, 23, 0, 0)
            ]
        );
    }

    #[test]
    fn midnights_between_includes_the_end_but_not_the_start() {
        assert_eq!(
            midnights_between(ts(2026, 10, 23, 23, 0), ts(2026, 10, 24, 0, 0), &utc),
            vec![ts(2026, 10, 24, 0, 0)]
        );
        assert!(midnights_between(ts(2026, 10, 24, 0, 0), ts(2026, 10, 24, 5, 0), &utc).is_empty());
        assert!(midnights_between(ts(2026, 10, 24, 5, 0), ts(2026, 10, 24, 1, 0), &utc).is_empty());
    }

    #[test]
    fn invalid_dates_are_refused() {
        assert!(day_bounds("2026-13-45", &utc).is_err());
        assert!(day_bounds("hier", &utc).is_err());
        assert!(day_bounds("", &utc).is_err());
    }

    #[test]
    fn format_duration_examples() {
        assert_eq!(format_duration(7800), "2 h 10");
        assert_eq!(format_duration(2580), "0 h 43");
        assert_eq!(format_duration(0), "0 h 00");
        assert_eq!(format_duration(12 * 3600 + 5 * 60 + 59), "12 h 05");
        assert_eq!(format_duration(-30), "0 h 00");
    }
}
