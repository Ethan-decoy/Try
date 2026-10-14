use crate::knowledge::AppResult;
use crate::tr;
use chrono::{DateTime, Duration, TimeZone, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PracticePause {
    #[default]
    None,
    Timed {
        until: i64,
        minutes: u32,
    },
    UntilResumed,
}

impl PracticePause {
    pub fn from_minutes(minutes: i32, now: i64) -> AppResult<Self> {
        let pause = match minutes {
            0 => Self::None,
            -1 => Self::UntilResumed,
            1..=1440 => Self::Timed {
                until: now
                    .checked_add(i64::from(minutes) * 60)
                    .ok_or(tr!("暂停结束时间超出范围。"))?,
                minutes: minutes as u32,
            },
            _ => return Err(tr!("暂停时间须为 1 至 1440 分钟。").into()),
        };
        pause.validate()?;
        Ok(pause)
    }

    pub fn validate(&self) -> AppResult<()> {
        if let Self::Timed { until, minutes } = self
            && (!(1..=1440).contains(minutes)
                || chrono::DateTime::from_timestamp(*until, 0).is_none())
        {
            return Err(tr!("暂停时间无效，请保留原配置。").into());
        }
        Ok(())
    }

    pub fn active(&self, now: i64) -> bool {
        match self {
            Self::None => false,
            Self::UntilResumed => true,
            Self::Timed { until, .. } => now < *until,
        }
    }

    pub fn minutes(&self, now: i64) -> i32 {
        if !self.active(now) {
            return 0;
        }
        match self {
            Self::Timed { minutes, .. } => *minutes as i32,
            Self::UntilResumed => -1,
            Self::None => 0,
        }
    }

    pub fn expired(&self, now: i64) -> bool {
        matches!(self, Self::Timed { until, .. } if now >= *until)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct QuietHours {
    pub enabled: bool,
    pub start_minute: u16,
    pub end_minute: u16,
}

impl Default for QuietHours {
    fn default() -> Self {
        Self {
            enabled: false,
            start_minute: 22 * 60,
            end_minute: 8 * 60,
        }
    }
}

impl QuietHours {
    pub fn validate(&self) -> AppResult<()> {
        if self.start_minute >= 1440 || self.end_minute >= 1440 {
            return Err(tr!("请勿打扰时间须在 00:00 至 23:59 之间。").into());
        }
        if self.start_minute == self.end_minute {
            return Err(tr!("请勿打扰的开始与结束时间须不同。").into());
        }
        Ok(())
    }

    pub fn from_text(enabled: bool, start: &str, end: &str) -> AppResult<Self> {
        let hours = Self {
            enabled,
            start_minute: parse_time(start)?,
            end_minute: parse_time(end)?,
        };
        hours.validate()?;
        Ok(hours)
    }

    pub fn contains<Tz: TimeZone>(&self, time: &DateTime<Tz>) -> bool {
        let minute = (time.hour() * 60 + time.minute()) as u16;
        self.enabled
            && if self.start_minute < self.end_minute {
                minute >= self.start_minute && minute < self.end_minute
            } else {
                minute >= self.start_minute || minute < self.end_minute
            }
    }

    /// A missed reminder waits a full interval after quiet hours. If that
    /// interval is longer than the daily active window, wait until the next
    /// allowed time without restarting the interval forever.
    pub fn next_reminder<Tz: TimeZone>(
        &self,
        now: DateTime<Tz>,
        interval_minutes: u32,
    ) -> DateTime<Tz> {
        let interval = Duration::minutes(i64::from(interval_minutes));
        if !self.enabled {
            return now + interval;
        }
        let was_quiet = self.contains(&now);
        let base = if was_quiet { self.end_after(&now) } else { now };
        let due = base + interval;
        if !self.contains(&due) {
            return due;
        }
        let end = self.end_after(&due);
        if was_quiet {
            return end;
        }
        let delayed = end + interval;
        if self.contains(&delayed) {
            self.end_after(&delayed)
        } else {
            delayed
        }
    }

    fn end_after<Tz: TimeZone>(&self, time: &DateTime<Tz>) -> DateTime<Tz> {
        let mut end = time
            .date_naive()
            .and_hms_opt(
                u32::from(self.end_minute / 60),
                u32::from(self.end_minute % 60),
                0,
            )
            .expect("quiet hours are validated when loading or saving settings");
        if end <= time.naive_local() {
            end += Duration::days(1);
        }
        // A DST jump can remove the configured minute. Pick the first valid
        // local minute afterwards; during a repeated hour use its later end.
        for _ in 0..=1440 {
            if let Some(end) = time.timezone().from_local_datetime(&end).latest() {
                return end;
            }
            end += Duration::minutes(1);
        }
        time.clone() + Duration::days(2)
    }
}

pub fn format_time(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

fn parse_time(text: &str) -> AppResult<u16> {
    let bytes = text.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || ![0, 1, 3, 4].into_iter().all(|i| bytes[i].is_ascii_digit())
    {
        return Err(tr!("请输入 HH:mm 格式的时间。").into());
    }
    let hour = u16::from(bytes[0] - b'0') * 10 + u16::from(bytes[1] - b'0');
    let minute = u16::from(bytes[3] - b'0') * 10 + u16::from(bytes[4] - b'0');
    if hour >= 24 || minute >= 60 {
        return Err(tr!("时间须在 00:00 至 23:59 之间。").into());
    }
    Ok(hour * 60 + minute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn pause_is_bounded_and_expires_at_its_original_deadline() {
        let now = at(10, 12, 0).timestamp();
        let pause = PracticePause::from_minutes(180, now).unwrap();
        assert!(pause.active(now + 180 * 60 - 1));
        assert!(!pause.active(now + 180 * 60));
        assert!(pause.expired(now + 180 * 60));
        assert_eq!(pause.minutes(now + 60), 180);
        assert_eq!(pause.minutes(now + 180 * 60), 0);
        assert!(PracticePause::UntilResumed.active(now + 10 * 86400));
        assert!(!PracticePause::None.active(now));
        for minutes in [-2, 1441] {
            assert!(PracticePause::from_minutes(minutes, now).is_err());
        }
        assert!(PracticePause::from_minutes(1, now).is_ok());
        assert!(PracticePause::from_minutes(1440, now).is_ok());
        assert!(PracticePause::from_minutes(1, i64::MAX).is_err());
        assert!(
            PracticePause::Timed {
                until: i64::MAX,
                minutes: 60
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn quiet_ranges_include_start_exclude_end_and_can_cross_midnight() {
        for (start, end, times) in [
            (
                "22:00",
                "08:00",
                vec![
                    (21, 59, false),
                    (22, 0, true),
                    (0, 0, true),
                    (7, 59, true),
                    (8, 0, false),
                ],
            ),
            (
                "09:30",
                "18:15",
                vec![
                    (9, 29, false),
                    (9, 30, true),
                    (12, 0, true),
                    (18, 14, true),
                    (18, 15, false),
                    (23, 0, false),
                ],
            ),
        ] {
            let quiet = QuietHours::from_text(true, start, end).unwrap();
            for (hour, minute, expected) in times {
                assert_eq!(quiet.contains(&at(10, hour, minute)), expected);
            }
        }
    }

    #[test]
    fn disabled_hours_keep_normal_interval() {
        let quiet = QuietHours::default();
        assert!(!quiet.contains(&at(10, 23, 0)));
        assert_eq!(quiet.next_reminder(at(10, 23, 0), 30), at(10, 23, 30));
    }

    #[test]
    fn reminders_due_during_quiet_wait_a_full_interval_afterwards() {
        let quiet = QuietHours::from_text(true, "22:00", "08:00").unwrap();
        for (now, expected) in [
            (at(10, 12, 0), at(10, 12, 30)),
            (at(10, 21, 30), at(11, 8, 30)),
            (at(10, 23, 0), at(11, 8, 30)),
            (at(10, 7, 59), at(10, 8, 30)),
            (at(10, 8, 0), at(10, 8, 30)),
        ] {
            assert_eq!(quiet.next_reminder(now, 30), expected);
        }
        let day = QuietHours::from_text(true, "09:00", "18:00").unwrap();
        assert_eq!(day.next_reminder(at(10, 8, 45), 30), at(10, 18, 30));
    }

    #[test]
    fn long_intervals_do_not_starve_reminders() {
        let quiet = QuietHours::from_text(true, "22:00", "08:00").unwrap();
        for now in [at(10, 21, 0), at(10, 23, 0), at(10, 8, 0)] {
            for interval in [1, 30, 840, 900, 1439, 1440] {
                let due = quiet.next_reminder(now, interval);
                assert!(!quiet.contains(&due));
                assert!(due >= now + Duration::minutes(i64::from(interval)));
                assert!(due <= now + Duration::days(3));
            }
        }
    }

    #[test]
    fn invalid_ranges_and_partial_edits_cannot_be_saved() {
        for (start, end) in [
            ("8:00", "09:00"),
            ("08:0", "09:00"),
            ("24:00", "09:00"),
            ("08:60", "09:00"),
            ("08:00", "08:00"),
            ("ab:cd", "09:00"),
        ] {
            assert!(QuietHours::from_text(true, start, end).is_err());
        }
        assert_eq!(format_time(5), "00:05");
        assert_eq!(format_time(1439), "23:59");
    }
}
