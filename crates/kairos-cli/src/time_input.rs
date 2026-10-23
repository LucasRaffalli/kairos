//! Les heures saisies en ligne de commande : `13:30` (heure locale du jour ou de `--date`),
//! `AAAA-MM-JJTHH:MM` (heure locale) ou RFC 3339 (avec son propre décalage).

use chrono::{DateTime, NaiveDateTime};
use kairos_core::error::{Error, Result};
use kairos_core::timeline;

/// Convertit la saisie en secondes depuis l'epoch. `date` est le jour de référence d'un `HH:MM`.
pub fn parse(input: &str, date: &str, offset: &dyn Fn(i64) -> i32) -> Result<i64> {
    let text = input.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Ok(dt.timestamp());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M") {
        let day = dt.format("%Y-%m-%d").to_string();
        let seconds = i64::from(chrono::Timelike::num_seconds_from_midnight(&dt.time()));
        return Ok(timeline::day_bounds(&day, offset)?.start + seconds);
    }
    let invalid = || Error::InvalidTime(input.to_string());
    let (h, m) = text.split_once(':').ok_or_else(invalid)?;
    let valid_part = |s: &str, max: i64| {
        (1..=2).contains(&s.len())
            && s.bytes().all(|b| b.is_ascii_digit())
            && s.parse::<i64>().is_ok_and(|n| n <= max)
    };
    if !valid_part(h, 23) || !(m.len() == 2 && valid_part(m, 59)) {
        return Err(invalid());
    }
    let seconds = h.parse::<i64>().map_err(|_| invalid())? * 3600
        + m.parse::<i64>().map_err(|_| invalid())? * 60;
    Ok(timeline::day_bounds(date, offset)?.start + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(_: i64) -> i32 {
        0
    }
    fn plus_two(_: i64) -> i32 {
        7200
    }

    const T9: i64 = 1_792_746_000; // 2026-10-23 09:00 UTC

    #[test]
    fn parses_hh_mm_on_the_given_date() {
        assert_eq!(parse("09:00", "2026-10-23", &utc).unwrap(), T9);
        assert_eq!(parse("9:05", "2026-10-23", &utc).unwrap(), T9 + 300);
        assert_eq!(parse("00:00", "2026-10-23", &utc).unwrap(), T9 - 9 * 3600);
    }

    #[test]
    fn hh_mm_is_local_time() {
        assert_eq!(parse("09:00", "2026-10-23", &plus_two).unwrap(), T9 - 7200);
    }

    #[test]
    fn parses_a_full_local_date_and_time() {
        assert_eq!(parse("2026-10-23T09:00", "2000-01-01", &utc).unwrap(), T9);
    }

    #[test]
    fn parses_rfc3339_with_its_own_offset() {
        assert_eq!(
            parse("2026-10-23T11:00:00+02:00", "2000-01-01", &utc).unwrap(),
            T9
        );
        assert_eq!(
            parse("2026-10-23T09:00:00Z", "2000-01-01", &plus_two).unwrap(),
            T9
        );
    }

    #[test]
    fn rejects_impossible_times_and_words() {
        for bad in [
            "25:61",
            "24:00",
            "12:60",
            "9h30",
            "hier",
            "",
            "12",
            "2026-13-01T10:00",
            "10:00:00",
        ] {
            let err = parse(bad, "2026-10-23", &utc).unwrap_err();
            assert_eq!(err.code(), "invalid_time", "{bad}");
        }
    }

    #[test]
    fn an_invalid_reference_date_is_reported_as_such() {
        assert_eq!(
            parse("09:00", "hier", &utc).unwrap_err().code(),
            "invalid_date"
        );
    }
}
