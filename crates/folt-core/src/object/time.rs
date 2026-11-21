//! Validates and formats canonical commit times; object body encoding lives elsewhere.

use std::fmt;
use std::str::FromStr;

use super::ObjectError;

/// Minimum commit time in Unix seconds (year 0000).
const COMMIT_TIME_MIN_SECONDS: i64 = -62_167_219_200;
/// Maximum commit time in Unix seconds (year 9999).
const COMMIT_TIME_MAX_SECONDS: i64 = 253_402_300_799;
/// Canonical commit time length in ASCII bytes.
const COMMIT_TIME_TEXT_LEN: usize = 20;

/// A UTC second in the proleptic Gregorian calendar, years 0000 through 9999.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitTime(i64);

impl CommitTime {
    pub fn from_unix_seconds(seconds: i64) -> Result<Self, ObjectError> {
        // These bounds keep both the arithmetic and the four-digit year canonical.
        if !(COMMIT_TIME_MIN_SECONDS..=COMMIT_TIME_MAX_SECONDS).contains(&seconds) {
            return Err(ObjectError::InvalidObject);
        }
        Ok(Self(seconds))
    }

    pub fn unix_seconds(self) -> i64 {
        self.0
    }
}

// Civil dates are decomposed into Gregorian 400-year eras, starting in March.
fn civil_date(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day = days.rem_euclid(146_097);
    let year = (day - day / 1_460 + day / 36_524 - day / 146_096) / 365;
    let ordinal = day - (365 * year + year / 4 - year / 100);
    let month = (5 * ordinal + 2) / 153;
    let date = ordinal - (153 * month + 2) / 5 + 1;
    let month = month + if month < 10 { 3 } else { -9 };
    (year + era * 400 + i64::from(month <= 2), month, date)
}

impl fmt::Display for CommitTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (year, month, day) = civil_date(self.0.div_euclid(86_400));
        let second = self.0.rem_euclid(86_400);
        write!(
            f,
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
            second / 3_600,
            second / 60 % 60,
            second % 60
        )
    }
}

impl FromStr for CommitTime {
    type Err = ObjectError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != COMMIT_TIME_TEXT_LEN || !value.is_ascii() {
            return Err(ObjectError::InvalidObject);
        }
        let number = |range: std::ops::Range<usize>| -> Result<i64, ObjectError> {
            let text = &value[range];
            if !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ObjectError::InvalidObject);
            }
            text.parse().map_err(|_| ObjectError::InvalidObject)
        };
        let year = number(0..4)?;
        let month = number(5..7)?;
        let day = number(8..10)?;
        let hour = number(11..13)?;
        let minute = number(14..16)?;
        let second = number(17..19)?;
        if !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return Err(ObjectError::InvalidObject);
        }
        let year = year - i64::from(month <= 2);
        let era = year.div_euclid(400);
        let year = year.rem_euclid(400);
        let month = month + if month > 2 { -3 } else { 9 };
        let ordinal = (153 * month + 2) / 5 + day - 1;
        let days = era * 146_097 + year * 365 + year / 4 - year / 100 + ordinal - 719_468;
        let time = Self::from_unix_seconds(days * 86_400 + hour * 3_600 + minute * 60 + second)?;
        if time.to_string() != value {
            return Err(ObjectError::InvalidObject);
        }
        Ok(time)
    }
}
