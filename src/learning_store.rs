//! The caller's Store retains the directory's single-writer lock.
use crate::knowledge::AppResult;
use crate::learning::LearningState;
use crate::storage::{Store, write_atomic};
use crate::tr;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MAX_LEARNING_BYTES: u64 = 32 * 1024 * 1024;

pub struct LearningStore {
    path: PathBuf,
}

impl LearningStore {
    pub fn new(store: &Store) -> Self {
        Self {
            path: store.directory().join("learning_state.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn load(&self) -> AppResult<LearningState> {
        if !self.exists() {
            return Ok(LearningState::default());
        }
        let read = || -> Result<_, Box<dyn std::error::Error>> {
            let mut bytes = Vec::new();
            std::fs::File::open(&self.path)?
                .take(MAX_LEARNING_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_LEARNING_BYTES {
                return Err(tr!("练习记录过大，请保留原文件。").into());
            }
            let state: LearningState = serde_json::from_slice(&bytes)?;
            state.validate()?;
            Ok(state)
        };
        read().map_err(|error| tr!("无法读取练习记录，原文件已保留：{error}", error = error))
    }

    pub fn save(&self, state: &LearningState) -> AppResult<()> {
        state.validate()?;
        let bytes = serde_json::to_vec(state).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_LEARNING_BYTES {
            return Err(tr!("练习记录过大，请保留原文件。").into());
        }
        write_atomic(&self.path, &bytes)
    }
}
