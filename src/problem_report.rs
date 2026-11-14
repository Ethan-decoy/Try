//! User-requested, local reproduction snapshots, independent of learning state.
use crate::knowledge::{AnswerContent, AnswerMode, AppResult, Card};
use crate::storage::{Store, write_atomic};
use crate::tr;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnswerAttempt {
    pub input: String,
    pub math_latex: Option<String>,
    pub keypad_expanded: bool,
}

/// Only the visible question and its input; no whole profile or log collection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProblemContext {
    pub card: Card,
    pub mode: AnswerMode,
    pub knowledge_only: bool,
    pub displayed_prompt: AnswerContent,
    pub displayed_answer: AnswerContent,
    pub options: Vec<AnswerContent>,
    pub selected_option: Option<usize>,
    pub outcome: Outcome,
    pub current_input: String,
    pub current_math_latex: Option<String>,
    pub keypad_expanded: bool,
    pub last_attempt: Option<AnswerAttempt>,
    pub input_error: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Unanswered,
    Correct,
    Incorrect,
    Knowledge,
}

/// The same version and reproduction case share a file, even after restarting.
/// Recording changes no card, setting, grade or learning record.
pub fn save(store: &Store, context: &ProblemContext) -> AppResult<PathBuf> {
    let runtime = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "git_revision": env!("TRY_GIT_REVISION"),
        "git_state": env!("TRY_GIT_STATE"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    });
    let mut report = serde_json::json!({
        "format": "try_problem_report",
        "schema_version": 1,
        "runtime": runtime,
        "context": context,
    });
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&report).map_err(|error| error.to_string())?)
    );
    report["recorded_at"] = chrono::Utc::now().to_rfc3339().into();
    let bytes = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    let directory = store.directory().join("bug_reports");
    std::fs::create_dir_all(&directory)
        .map_err(|error| tr!("无法创建问题记录目录：{error}", error = error))?;
    let path = directory.join(format!("problem_{fingerprint}.json"));
    write_atomic(&path, &bytes)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::{Learner, Origin};
    use crate::practice::PreparedQuestion;

    #[test]
    fn local_snapshot_preserves_the_case_without_touching_profile_or_progress() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().join("profile")).unwrap();
        let library = store.load().unwrap();
        store.save(&library).unwrap();
        let state = std::fs::read(store.directory().join("state.json")).unwrap();
        let mut learner = Learner::new(&store);
        let question = learner
            .choose(&library, None, 1_800_000_000, &mut rand::rng())
            .unwrap()
            .unwrap();
        let token = learner
            .presented(&question, AnswerMode::Input, 1_800_000_000, Origin::Manual)
            .unwrap();
        learner
            .answer(&library, &token, false, 1_800_000_001)
            .unwrap();
        let progress = std::fs::read(store.directory().join("learning_state.json")).unwrap();
        let prepared =
            PreparedQuestion::new(&question.card, AnswerMode::Input, &mut rand::rng()).unwrap();
        let mut context = ProblemContext {
            card: question.card,
            mode: prepared.mode,
            knowledge_only: false,
            displayed_prompt: prepared.prompt,
            displayed_answer: prepared.answer,
            options: prepared.options,
            selected_option: None,
            outcome: Outcome::Incorrect,
            current_input: "yln(x)".into(),
            current_math_latex: None,
            keypad_expanded: false,
            last_attempt: Some(AnswerAttempt {
                input: "yln(x)".into(),
                math_latex: Some(r"y\ln\left(x\right)".into()),
                keypad_expanded: true,
            }),
            input_error: String::new(),
        };
        let path = save(&store, &context).unwrap();
        assert_eq!(
            path.parent().unwrap(),
            store.directory().join("bug_reports")
        );
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["format"], "try_problem_report");
        assert_eq!(report["runtime"]["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(report["context"], serde_json::to_value(&context).unwrap());
        assert!(report["recorded_at"].as_str().is_some());
        assert_eq!(
            save(&store, &context).unwrap(),
            path,
            "Repeated clicks reuse the same case file"
        );
        context.current_input = "changed".into();
        assert_ne!(save(&store, &context).unwrap(), path);
        assert_eq!(
            std::fs::read(store.directory().join("state.json")).unwrap(),
            state
        );
        assert_eq!(
            std::fs::read(store.directory().join("learning_state.json")).unwrap(),
            progress
        );
        assert_eq!(
            std::fs::read_dir(store.directory().join("bug_reports"))
                .unwrap()
                .count(),
            2
        );
    }

    #[test]
    fn a_save_failure_leaves_existing_records_and_profile_intact() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_path_buf()).unwrap();
        let library = store.load().unwrap();
        store.save(&library).unwrap();
        let state = std::fs::read(store.directory().join("state.json")).unwrap();
        std::fs::write(store.directory().join("bug_reports"), b"occupied").unwrap();
        let card = library.packs[0].cards[0].clone();
        let prepared = PreparedQuestion::new(&card, AnswerMode::Input, &mut rand::rng()).unwrap();
        let context = ProblemContext {
            card,
            mode: prepared.mode,
            knowledge_only: false,
            displayed_prompt: prepared.prompt,
            displayed_answer: prepared.answer,
            options: prepared.options,
            selected_option: None,
            outcome: Outcome::Unanswered,
            current_input: String::new(),
            current_math_latex: Some(r"\frac{1}{}".into()),
            keypad_expanded: false,
            last_attempt: None,
            input_error: "incomplete".into(),
        };
        assert!(save(&store, &context).is_err());
        assert_eq!(
            std::fs::read(store.directory().join("bug_reports")).unwrap(),
            b"occupied"
        );
        assert_eq!(
            std::fs::read(store.directory().join("state.json")).unwrap(),
            state
        );
    }
}
