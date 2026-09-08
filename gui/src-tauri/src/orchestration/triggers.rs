use chrono::{Datelike, Duration, LocalResult, TimeZone, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Trigger {
    Now,
    Once {
        at: i64,
    },
    Interval {
        seconds: u64,
        anchor: i64,
    },
    Calendar {
        timezone: String,
        hour: u32,
        minute: u32,
        weekdays: Vec<u32>,
    },
}
impl Trigger {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Once { at } if chrono::DateTime::from_timestamp(*at, 0).is_none() => {
                Err("Invalid start time".into())
            }
            Self::Interval { seconds, anchor }
                if !(60..=31_536_000).contains(seconds)
                    || chrono::DateTime::from_timestamp(*anchor, 0).is_none() =>
            {
                Err("Interval must be between one minute and one year".into())
            }
            Self::Calendar {
                timezone,
                hour,
                minute,
                weekdays,
            } => {
                timezone
                    .parse::<chrono_tz::Tz>()
                    .map_err(|_| "Choose an IANA time zone")?;
                if *hour > 23
                    || *minute > 59
                    || weekdays.is_empty()
                    || weekdays.len() > 7
                    || weekdays.iter().any(|v| *v > 6)
                {
                    return Err("Invalid calendar schedule (Monday = 0)".into());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub fn recurring(&self) -> bool {
        matches!(self, Self::Interval { .. } | Self::Calendar { .. })
    }
    /// First occurrence strictly after `after`. Gaps are skipped; folds run once, earliest.
    pub fn next(&self, after: i64) -> Result<Option<i64>, String> {
        self.validate()?;
        match self {
            Self::Now => Ok(None),
            Self::Once { at } => Ok((*at > after).then_some(*at)),
            Self::Interval { seconds, anchor } => {
                let interval = *seconds as i64;
                let delta = after.checked_sub(*anchor).ok_or("Schedule time overflow")?;
                let n = if delta < 0 { 0 } else { delta / interval + 1 };
                Ok(Some(
                    n.checked_mul(interval)
                        .and_then(|v| anchor.checked_add(v))
                        .ok_or("Schedule time overflow")?,
                ))
            }
            Self::Calendar {
                timezone,
                hour,
                minute,
                weekdays,
            } => {
                let zone = timezone
                    .parse::<chrono_tz::Tz>()
                    .map_err(|e| e.to_string())?;
                let utc = Utc
                    .timestamp_opt(after, 0)
                    .single()
                    .ok_or("Invalid timestamp")?;
                let date = utc.with_timezone(&zone).date_naive();
                for day in 0..=370 {
                    let date = date
                        .checked_add_signed(Duration::days(day))
                        .ok_or("Date overflow")?;
                    if !weekdays.contains(&date.weekday().num_days_from_monday()) {
                        continue;
                    }
                    let local = date
                        .and_hms_opt(*hour, *minute, 0)
                        .ok_or("Invalid clock time")?;
                    let candidate = match zone.from_local_datetime(&local) {
                        LocalResult::None => None,
                        LocalResult::Single(v) => Some(v),
                        LocalResult::Ambiguous(a, b) => Some(a.min(b)),
                    };
                    if let Some(v) = candidate {
                        if v.timestamp() > after {
                            return Ok(Some(v.timestamp()));
                        }
                    }
                }
                Err("No calendar occurrence found within a year".into())
            }
        }
    }
}
