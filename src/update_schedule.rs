//! Persisted daily release-check budget and the last successful result.

use crate::knowledge::AppResult;
use crate::storage::write_atomic;
use crate::update_check::{CheckOutcome, Release};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const STARTUP_DELAY: Duration = Duration::from_secs(30);
const INTERVAL_SECONDS: i64 = 24 * 60 * 60;
const MAX_CACHE_BYTES: usize = 4096;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SavedOutcome {
    Available { tag: String },
    UpToDate { checked_version: String },
    NoPublicRelease,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema_version: u32,
    attempted_at: Option<i64>,
    outcome: Option<SavedOutcome>,
}

impl Default for Record {
    fn default() -> Self {
        Self {
            schema_version: 1,
            attempted_at: None,
            outcome: None,
        }
    }
}

pub struct UpdateSchedule {
    path: PathBuf,
    current_version: String,
    record: Record,
}

impl UpdateSchedule {
    pub fn open(directory: &Path, current_version: &str, now: i64) -> Self {
        let path = directory.join("update_check_state.json");
        let record = match Self::read(&path) {
            Ok(record) => record,
            Err(error) => {
                log::warn!(target: "try::updates", "event=cache_read_failed reason={error}");
                Record::default()
            }
        };
        let mut schedule = Self {
            path,
            current_version: current_version.into(),
            record,
        };
        schedule.delay(now);
        schedule
    }

    fn read(path: &Path) -> AppResult<Record> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Record::default());
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut bytes = Vec::new();
        file.take((MAX_CACHE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_CACHE_BYTES {
            return Err("update cache exceeds size limit".into());
        }
        let record: Record = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let outcome_valid = match &record.outcome {
            Some(SavedOutcome::Available { tag }) => {
                tag.len() <= 256 && Release::from_tag(tag).is_ok()
            }
            Some(SavedOutcome::UpToDate { checked_version }) => {
                checked_version.len() <= 80 && Version::parse(checked_version).is_ok()
            }
            _ => true,
        };
        if record.schema_version != 1
            || record.attempted_at.is_some_and(|time| time < 0)
            || !outcome_valid
        {
            return Err("invalid update cache".into());
        }
        Ok(record)
    }

    /// Clock rollback rebases the budget once, rather than postponing it indefinitely.
    pub fn delay(&mut self, now: i64) -> Duration {
        let Some(last) = self.record.attempted_at else {
            return Duration::ZERO;
        };
        if now < last {
            self.record.attempted_at = Some(now.max(0));
            if let Err(error) = self.save() {
                log::warn!(target: "try::updates", "event=cache_clock_save_failed reason={error}");
            }
            return Duration::from_secs(INTERVAL_SECONDS as u64);
        }
        Duration::from_secs(
            INTERVAL_SECONDS
                .saturating_sub(now.saturating_sub(last))
                .max(0) as u64,
        )
    }

    pub fn cached_outcome(&self) -> Option<CheckOutcome> {
        match self.record.outcome.as_ref()? {
            SavedOutcome::Available { tag } => {
                let release = Release::from_tag(tag).ok()?;
                let current = Version::parse(&self.current_version).ok()?;
                Some(if release.version > current {
                    CheckOutcome::Available(release)
                } else {
                    CheckOutcome::UpToDate
                })
            }
            SavedOutcome::UpToDate { checked_version }
                if checked_version == &self.current_version =>
            {
                Some(CheckOutcome::UpToDate)
            }
            SavedOutcome::NoPublicRelease => Some(CheckOutcome::NoPublicRelease),
            _ => None,
        }
    }

    /// Failed requests consume the same budget, including across restarts.
    pub fn record_attempt(&mut self, now: i64) -> AppResult<()> {
        self.record.attempted_at = Some(now.max(0));
        self.save()
    }

    pub fn record_result(&mut self, outcome: &CheckOutcome) -> AppResult<()> {
        self.record.outcome = Some(match outcome {
            CheckOutcome::Available(release) => SavedOutcome::Available {
                tag: release.tag().into(),
            },
            CheckOutcome::UpToDate => SavedOutcome::UpToDate {
                checked_version: self.current_version.clone(),
            },
            CheckOutcome::NoPublicRelease => SavedOutcome::NoPublicRelease,
        });
        self.save()
    }

    fn save(&self) -> AppResult<()> {
        let bytes = serde_json::to_vec(&self.record).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_CACHE_BYTES {
            return Err("update cache exceeds size limit".into());
        }
        write_atomic(&self.path, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_budget_survives_restart_and_manual_attempts() {
        let dir = tempfile::tempdir().unwrap();
        let mut schedule = UpdateSchedule::open(dir.path(), "0.1.0", 100);
        assert_eq!(schedule.delay(100), Duration::ZERO);
        schedule.record_attempt(100).unwrap();
        let mut restarted = UpdateSchedule::open(dir.path(), "0.1.0", 110);
        assert_eq!(restarted.delay(110), Duration::from_secs(86390));
        assert_eq!(restarted.delay(86499), Duration::from_secs(1));
        assert_eq!(restarted.delay(86500), Duration::ZERO);
        restarted.record_attempt(86500).unwrap();
        assert_eq!(restarted.delay(86500), Duration::from_secs(86400));
    }

    #[test]
    fn successful_hint_survives_failure_restart_and_installing_the_update() {
        let dir = tempfile::tempdir().unwrap();
        let mut schedule = UpdateSchedule::open(dir.path(), "0.1.0", 100);
        let available = CheckOutcome::Available(Release::from_tag("v0.2.0").unwrap());
        schedule.record_attempt(100).unwrap();
        schedule.record_result(&available).unwrap();
        // A later failed request records only its attempt, retaining the successful hint.
        schedule.record_attempt(101).unwrap();
        assert_eq!(
            UpdateSchedule::open(dir.path(), "0.1.0", 102).cached_outcome(),
            Some(available)
        );
        assert_eq!(
            UpdateSchedule::open(dir.path(), "0.2.0", 102).cached_outcome(),
            Some(CheckOutcome::UpToDate)
        );
    }

    #[test]
    fn current_and_no_release_results_are_cached_without_false_upgrade_claims() {
        let dir = tempfile::tempdir().unwrap();
        let mut schedule = UpdateSchedule::open(dir.path(), "0.1.0", 100);
        schedule.record_result(&CheckOutcome::UpToDate).unwrap();
        assert_eq!(
            UpdateSchedule::open(dir.path(), "0.1.0", 100).cached_outcome(),
            Some(CheckOutcome::UpToDate)
        );
        assert_eq!(
            UpdateSchedule::open(dir.path(), "0.0.9", 100).cached_outcome(),
            None
        );
        schedule
            .record_result(&CheckOutcome::NoPublicRelease)
            .unwrap();
        assert_eq!(
            UpdateSchedule::open(dir.path(), "0.1.0", 100).cached_outcome(),
            Some(CheckOutcome::NoPublicRelease)
        );
    }

    #[test]
    fn backward_clock_rebases_once_and_recovers_after_one_day() {
        let dir = tempfile::tempdir().unwrap();
        let mut schedule = UpdateSchedule::open(dir.path(), "0.1.0", 200);
        schedule.record_attempt(200).unwrap();
        let mut restarted = UpdateSchedule::open(dir.path(), "0.1.0", 100);
        assert_eq!(restarted.delay(100), Duration::from_secs(86400));
        let mut again = UpdateSchedule::open(dir.path(), "0.1.0", 101);
        assert_eq!(again.delay(86500), Duration::ZERO);
    }

    #[test]
    fn invalid_or_oversized_cache_cannot_supply_download_hints() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update_check_state.json");
        for bytes in [b"broken".to_vec(), vec![b' '; MAX_CACHE_BYTES + 1], br#"{"schema_version":1,"attempted_at":100,"outcome":{"kind":"available","tag":"https://evil.example"}}"#.to_vec()] {
            std::fs::write(&path, bytes).unwrap();
            let mut schedule = UpdateSchedule::open(dir.path(), "0.1.0", 100);
            assert_eq!(schedule.cached_outcome(), None);
            assert_eq!(schedule.delay(100), Duration::ZERO);
        }
    }

    #[test]
    fn unwritable_cache_retains_in_memory_budget_without_touching_user_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update_check_state.json");
        std::fs::create_dir(&path).unwrap();
        let mut schedule = UpdateSchedule::open(dir.path(), "0.1.0", 100);
        assert!(schedule.record_attempt(100).is_err());
        assert_eq!(schedule.delay(100), Duration::from_secs(86400));
        assert!(path.is_dir());
        assert!(!dir.path().join("state.json").exists());
    }
}
