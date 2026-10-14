//! Personal memory and soft selection, independent of windows and timers.
//! All clock and random inputs are supplied by the caller. Knowledge-only
//! cards must use practice::select_question and never enter this module.

use crate::fsrs6::{self, Memory};
use crate::knowledge::{AnswerMode, AppResult, Card, Library};
use crate::learning_store::LearningStore;
use crate::practice::{self, Question, QuestionKey};
use crate::storage::Store;
use crate::tr;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};

const TIME_ADAPTER: &str = "elapsed_whole_days_v1";
const MAX_REVIEWS: usize = 4096;
const RECENT_WINDOW: usize = 8;
const COVERAGE_EVERY: u64 = 5;
const WEIGHT_FLOOR: f64 = 0.05;
const URGENCY_CAP: f64 = 4.0;
const FAILURE_BONUS: f64 = 1.0;
const MAX_TIMESTAMP: i64 = 253402300799;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Manual,
    Reminder,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Progress {
    fingerprint: String,
    memory: Memory,
    last_review_at: i64,
    last_review_sequence: u64,
    correct: bool,
    successes: u32,
    failures: u32,
    delayed_reviews: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CardProgress {
    modes: [Option<Progress>; 3],
    last_shown: Option<Presentation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Presentation {
    key: QuestionKey,
    content: String,
    at: i64,
    sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    key: QuestionKey,
    mode: AnswerMode,
    fingerprint: String,
    at: i64,
    sequence: u64,
    correct: bool,
    origin: Origin,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningState {
    format: String,
    schema_version: u32,
    model_version: String,
    time_adapter: String,
    presentations: u64,
    packs: BTreeMap<String, BTreeMap<String, CardProgress>>,
    recent: VecDeque<Presentation>,
    reviews: VecDeque<Review>,
}

impl Default for LearningState {
    fn default() -> Self {
        Self {
            format: "try_learning".into(),
            schema_version: 1,
            model_version: fsrs6::MODEL_VERSION.into(),
            time_adapter: TIME_ADAPTER.into(),
            presentations: 0,
            packs: BTreeMap::new(),
            recent: VecDeque::new(),
            reviews: VecDeque::new(),
        }
    }
}

/// Model evidence, not a calibrated confidence interval or public rank.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackSummary {
    pub participating: usize,
    pub assessed: usize,
    pub familiarity: Option<f64>,
    pub current_retrievability: Option<f64>,
    pub weakest_familiarity: Option<f64>,
    pub effective_reviews: u64,
    pub delayed_reviews: u64,
}

/// A capability for the actual card that was presented. It carries no input text.
#[derive(Clone, Debug)]
pub struct ReviewToken {
    key: QuestionKey,
    mode: AnswerMode,
    fingerprint: String,
    at: i64,
    sequence: u64,
    origin: Origin,
}

/// Lazily loads only on answering practice. Failed loads preserve the original
/// file and fall back to uniform practice until restart; they never overwrite it.
pub struct Learner {
    store: LearningStore,
    state: Option<LearningState>,
    load_failed: bool,
}

struct Candidate<'a> {
    pack: &'a str,
    card: &'a Card,
    progress: Option<&'a Progress>,
    content: String,
    last_sequence: u64,
    weight: f64,
}

fn digest<T: Serialize>(content: &T) -> String {
    let bytes = serde_json::to_vec(content).expect("fingerprints contain only fixed JSON types");
    format!("{:x}", Sha256::digest(bytes))
}

fn fingerprint(card: &Card, mode: AnswerMode) -> String {
    match mode {
        AnswerMode::Input => digest(&(
            1,
            mode,
            &card.prompt,
            &card.prompt_latex,
            &card.answer,
            &card.answer_latex,
            &card.aliases,
        )),
        AnswerMode::MultipleChoice => digest(&(
            1,
            mode,
            &card.prompt,
            &card.prompt_latex,
            &card.answer,
            &card.answer_latex,
            &card.distractors,
        )),
        AnswerMode::TrueFalse => digest(&(
            1,
            mode,
            card.true_false
                .as_ref()
                .map(|s| (&s.statement, &s.statement_latex, s.is_true)),
        )),
    }
}

fn content_stamp(card: &Card) -> String {
    digest(&(1, card))
}

fn key(pack: &str, card: &Card) -> QuestionKey {
    QuestionKey {
        pack_id: pack.into(),
        card_id: card
            .id
            .clone()
            .expect("local cards have stable identifiers"),
    }
}

fn valid_timestamp(at: i64) -> bool {
    (0..=MAX_TIMESTAMP).contains(&at)
}

fn valid_key(key: &QuestionKey) -> bool {
    [&key.pack_id, &key.card_id].iter().all(|s| {
        !s.is_empty()
            && s.len() <= 80
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    })
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl LearningState {
    pub(crate) fn validate(&self) -> AppResult<()> {
        let invalid = || tr!("练习记录格式无效，请保留原文件。").to_string();
        if self.format != "try_learning"
            || self.schema_version != 1
            || self.model_version != fsrs6::MODEL_VERSION
            || self.time_adapter != TIME_ADAPTER
            || self.presentations == u64::MAX
            || self.packs.len() > 64
            || self.packs.values().map(BTreeMap::len).sum::<usize>() > 20_000
            || self.recent.len() > RECENT_WINDOW
            || self.reviews.len() > MAX_REVIEWS
        {
            return Err(invalid());
        }
        for (pack_id, cards) in &self.packs {
            for (card_id, card) in cards {
                let key = QuestionKey {
                    pack_id: pack_id.clone(),
                    card_id: card_id.clone(),
                };
                if !valid_key(&key) {
                    return Err(invalid());
                }
                for progress in card.modes.iter().flatten() {
                    let count = u64::from(progress.successes) + u64::from(progress.failures);
                    if !progress.memory.valid()
                        || !valid_digest(&progress.fingerprint)
                        || !valid_timestamp(progress.last_review_at)
                        || progress.last_review_sequence == 0
                        || progress.last_review_sequence > self.presentations
                        || count == 0
                        || u64::from(progress.delayed_reviews) >= count
                    {
                        return Err(invalid());
                    }
                }
                if let Some(shown) = &card.last_shown
                    && (shown.key != key || !self.valid_presentation(shown))
                {
                    return Err(invalid());
                }
            }
        }
        let mut last = 0;
        for shown in &self.recent {
            if !self.valid_presentation(shown) || shown.sequence <= last {
                return Err(invalid());
            }
            last = shown.sequence;
        }
        last = 0;
        for review in &self.reviews {
            if !valid_key(&review.key)
                || !valid_digest(&review.fingerprint)
                || !valid_timestamp(review.at)
                || review.sequence <= last
                || review.sequence > self.presentations
            {
                return Err(invalid());
            }
            last = review.sequence;
        }
        Ok(())
    }

    fn valid_presentation(&self, shown: &Presentation) -> bool {
        valid_key(&shown.key)
            && valid_digest(&shown.content)
            && valid_timestamp(shown.at)
            && shown.sequence > 0
            && shown.sequence <= self.presentations
    }

    fn card(&self, key: &QuestionKey) -> Option<&CardProgress> {
        self.packs.get(&key.pack_id)?.get(&key.card_id)
    }

    fn progress(&self, key: &QuestionKey, card: &Card, mode: AnswerMode) -> Option<&Progress> {
        self.card(key)?.modes[mode.index() as usize]
            .as_ref()
            .filter(|p| p.fingerprint == fingerprint(card, mode))
    }

    fn reconcile(&mut self, library: &Library) {
        let all: BTreeMap<_, _> = library
            .packs
            .iter()
            .flat_map(|p| {
                p.cards
                    .iter()
                    .filter_map(move |c| c.id.as_deref().map(|id| ((p.id.as_str(), id), c)))
            })
            .collect();
        let lookup = |key: &QuestionKey| {
            all.get(&(key.pack_id.as_str(), key.card_id.as_str()))
                .copied()
        };
        self.packs.retain(|pack_id, cards| {
            let Some(pack) = library.packs.iter().find(|p| &p.id == pack_id) else {
                return false;
            };
            // One lookup per pack rather than a quadratic scan of large packs.
            let content: BTreeMap<_, _> = pack
                .cards
                .iter()
                .filter_map(|c| c.id.as_ref().map(|id| (id, c)))
                .collect();
            cards.retain(|card_id, progress| {
                let Some(card) = content.get(card_id) else {
                    return false;
                };
                for mode in AnswerMode::ALL {
                    let item = &mut progress.modes[mode.index() as usize];
                    if item
                        .as_ref()
                        .is_some_and(|p| p.fingerprint != fingerprint(card, mode))
                    {
                        *item = None;
                    }
                }
                if progress
                    .last_shown
                    .as_ref()
                    .is_some_and(|p| p.content != content_stamp(card))
                {
                    progress.last_shown = None;
                }
                true
            });
            !cards.is_empty()
        });
        self.recent
            .retain(|p| lookup(&p.key).is_some_and(|c| p.content == content_stamp(c)));
        self.reviews.retain(|r| lookup(&r.key).is_some());
    }

    pub fn pack_summary(&self, library: &Library, pack_id: &str, now: i64) -> PackSummary {
        let mut summary = PackSummary::default();
        let mut familiarity = 0.0;
        let mut retrieval = 0.0;
        for (pack, card) in library
            .participating_cards()
            .filter(|(p, _)| p.id == pack_id)
        {
            summary.participating += 1;
            let key = key(&pack.id, card);
            let mode = library.card_answer_mode(&key.pack_id, &key.card_id);
            if let Some(progress) = self.progress(&key, card, mode) {
                let f = f64::from(progress.memory.stability).ln_1p();
                summary.assessed += 1;
                familiarity += f;
                retrieval += fsrs6::retrievability(
                    progress.memory,
                    now.saturating_sub(progress.last_review_at),
                );
                summary.weakest_familiarity =
                    Some(summary.weakest_familiarity.map_or(f, |old| old.min(f)));
                summary.effective_reviews +=
                    u64::from(progress.successes) + u64::from(progress.failures);
                summary.delayed_reviews += u64::from(progress.delayed_reviews);
            }
        }
        if summary.assessed != 0 {
            summary.familiarity = Some(familiarity / summary.assessed as f64);
            summary.current_retrievability = Some(retrieval / summary.assessed as f64);
        }
        summary
    }

    fn select(
        &self,
        library: &Library,
        previous: Option<&QuestionKey>,
        now: i64,
        rng: &mut impl Rng,
    ) -> Option<Question> {
        let mut candidates: Vec<_> = library
            .participating_cards()
            .map(|(pack, card)| {
                let key = key(&pack.id, card);
                let mode = library.card_answer_mode(&key.pack_id, &key.card_id);
                let progress = self.progress(&key, card, mode);
                let content = content_stamp(card);
                let last_sequence = self
                    .card(&key)
                    .and_then(|p| p.last_shown.as_ref())
                    .filter(|s| s.content == content)
                    .map_or(0, |s| s.sequence);
                let weight = progress.map_or(1.0, |p| {
                    let days = now.saturating_sub(p.last_review_at).max(0) as f64 / 86400.0;
                    WEIGHT_FLOOR
                        + (days / fsrs6::interval_days(p.memory)).min(URGENCY_CAP)
                        + if p.correct { 0.0 } else { FAILURE_BONUS }
                });
                Candidate {
                    pack: &pack.id,
                    card,
                    progress,
                    content,
                    last_sequence,
                    weight,
                }
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let next = self.presentations.saturating_add(1);
        let previous_differs = |c: &Candidate<'_>| {
            previous.is_none_or(|p| {
                p.pack_id != c.pack || Some(p.card_id.as_str()) != c.card.id.as_deref()
            })
        };
        if candidates.iter().any(previous_differs) {
            candidates.retain(previous_differs);
        }
        // Failed items wait for two intervening cards and occur at most twice
        // in the last eight presentations where other candidates permit it.
        let safe = |c: &Candidate<'_>| {
            c.progress.is_none_or(|p| p.correct)
                || (next.saturating_sub(c.last_sequence) >= 3
                    && self
                        .recent
                        .iter()
                        .filter(|p| {
                            p.key.pack_id == c.pack
                                && Some(p.key.card_id.as_str()) == c.card.id.as_deref()
                                && p.content == c.content
                        })
                        .count()
                        < 2)
        };
        if candidates.iter().any(safe) {
            candidates.retain(safe);
        }
        // Counted, alternating coverage lanes. Uniform tie-breaking happens
        // through the same summed pack decomposition, without domain quotas.
        if next.is_multiple_of(COVERAGE_EVERY) {
            let explore = (next / COVERAGE_EVERY) % 2 == 1;
            if explore && candidates.iter().any(|c| c.progress.is_none()) {
                candidates.retain(|c| c.progress.is_none());
            } else if !explore && candidates.iter().any(|c| c.last_sequence != 0) {
                candidates.retain(|c| c.last_sequence != 0);
            }
            let oldest = candidates.iter().map(|c| c.last_sequence).min().unwrap();
            candidates.retain(|c| c.last_sequence == oldest);
            for c in &mut candidates {
                c.weight = 1.0;
            }
        }
        let mut packs = BTreeMap::<&str, f64>::new();
        for c in &candidates {
            *packs.entry(c.pack).or_default() += c.weight;
        }
        let total: f64 = packs.values().sum();
        let mut roll = rng.random_range(0.0..total);
        let mut chosen_pack = *packs.last_key_value().unwrap().0;
        for (&pack, &weight) in &packs {
            if roll < weight {
                chosen_pack = pack;
                break;
            }
            roll -= weight;
        }
        let mut roll = rng.random_range(0.0..packs[chosen_pack]);
        let mut chosen = None;
        for c in candidates.iter().filter(|c| c.pack == chosen_pack) {
            chosen = Some(c);
            if roll < c.weight {
                break;
            }
            roll -= c.weight;
        }
        let chosen = chosen?;
        Some(Question {
            key: key(chosen.pack, chosen.card),
            card: chosen.card.clone(),
        })
    }
}

impl Learner {
    /// Diagnostic status; querying it does not initialize the personal profile.
    pub fn is_loaded(&self) -> bool {
        self.state.is_some()
    }

    pub fn new(store: &Store) -> Self {
        Self {
            store: LearningStore::new(store),
            state: None,
            load_failed: false,
        }
    }

    pub fn choose(
        &mut self,
        library: &Library,
        previous: Option<&QuestionKey>,
        now: i64,
        rng: &mut impl Rng,
    ) -> AppResult<Option<Question>> {
        if self.load_failed {
            return Ok(practice::select_question(library, previous, rng));
        }
        if self.state.is_none() {
            match self.store.load() {
                Ok(mut state) => {
                    state.reconcile(library);
                    self.state = Some(state);
                }
                Err(error) => {
                    self.load_failed = true;
                    return Err(error);
                }
            }
        }
        Ok(self
            .state
            .as_ref()
            .unwrap()
            .select(library, previous, now, rng))
    }

    /// Presentation cadence is in-memory until a valid answer is committed.
    /// Dismissing a card never enters the memory update or writes a result.
    pub fn presented(
        &mut self,
        question: &Question,
        mode: AnswerMode,
        now: i64,
        origin: Origin,
    ) -> Option<ReviewToken> {
        if !valid_timestamp(now) {
            return None;
        }
        let state = self.state.as_mut()?;
        let sequence = state.presentations.checked_add(1)?;
        let shown = Presentation {
            key: question.key.clone(),
            content: content_stamp(&question.card),
            at: now,
            sequence,
        };
        state.presentations = sequence;
        state
            .packs
            .entry(question.key.pack_id.clone())
            .or_default()
            .entry(question.key.card_id.clone())
            .or_default()
            .last_shown = Some(shown.clone());
        state.recent.push_back(shown);
        if state.recent.len() > RECENT_WINDOW {
            state.recent.pop_front();
        }
        Some(ReviewToken {
            key: question.key.clone(),
            mode,
            fingerprint: fingerprint(&question.card, mode),
            at: now,
            sequence,
            origin,
        })
    }

    pub fn answer(
        &mut self,
        library: &Library,
        token: &ReviewToken,
        correct: bool,
        now: i64,
    ) -> AppResult<bool> {
        let Some(state) = &self.state else {
            return Ok(false);
        };
        if token.sequence != state.presentations {
            return Ok(false);
        }
        let Some(card) = library
            .packs
            .iter()
            .find(|p| p.id == token.key.pack_id)
            .and_then(|p| {
                p.cards
                    .iter()
                    .find(|c| c.id.as_deref() == Some(&token.key.card_id))
            })
        else {
            return Ok(false);
        };
        if fingerprint(card, token.mode) != token.fingerprint {
            return Ok(false);
        }
        let previous = state.progress(&token.key, card, token.mode);
        if previous.is_some_and(|p| p.last_review_sequence >= token.sequence) {
            return Ok(false);
        }
        if !valid_timestamp(now)
            || now < token.at
            || previous.is_some_and(|p| now < p.last_review_at)
        {
            return Err(tr!("系统时间早于上次作答，练习记录未更新。").into());
        }
        let delta = previous.map_or(0, |p| now - p.last_review_at);
        let days = u32::try_from(delta / 86400).unwrap_or(u32::MAX);
        let progress = Progress {
            fingerprint: token.fingerprint.clone(),
            memory: fsrs6::reviewed(previous.map(|p| p.memory), days, correct),
            last_review_at: now,
            last_review_sequence: token.sequence,
            correct,
            successes: previous
                .map_or(0, |p| p.successes)
                .saturating_add(u32::from(correct)),
            failures: previous
                .map_or(0, |p| p.failures)
                .saturating_add(u32::from(!correct)),
            delayed_reviews: previous
                .map_or(0, |p| p.delayed_reviews)
                .saturating_add(u32::from(days > 0)),
        };
        let mut candidate = state.clone();
        candidate.reconcile(library);
        candidate
            .packs
            .entry(token.key.pack_id.clone())
            .or_default()
            .entry(token.key.card_id.clone())
            .or_default()
            .modes[token.mode.index() as usize] = Some(progress);
        candidate.reviews.push_back(Review {
            key: token.key.clone(),
            mode: token.mode,
            fingerprint: token.fingerprint.clone(),
            at: now,
            sequence: token.sequence,
            correct,
            origin: token.origin,
        });
        if candidate.reviews.len() > MAX_REVIEWS {
            candidate.reviews.pop_front();
        }
        self.store.save(&candidate)?;
        self.state = Some(candidate);
        Ok(true)
    }

    /// Only an already loaded answer profile can be reconciled. Content remains
    /// authoritative even if cleanup fails, because every read checks its digest.
    pub fn reconcile(&mut self, library: &Library) -> AppResult<()> {
        let Some(state) = &self.state else {
            return Ok(());
        };
        let mut candidate = state.clone();
        candidate.reconcile(library);
        if candidate != *state {
            if self.store.exists() {
                self.store.save(&candidate)?;
            }
            self.state = Some(candidate);
        }
        Ok(())
    }

    pub fn pack_summary(&self, library: &Library, pack_id: &str, now: i64) -> PackSummary {
        self.state.as_ref().map_or_else(PackSummary::default, |s| {
            s.pack_summary(library, pack_id, now)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{AnswerContent, KnowledgePack, Settings, TrueFalse};
    use rand::{SeedableRng, rngs::StdRng};

    const NOW: i64 = 1_790_000_000;

    fn library(sizes: &[usize]) -> Library {
        let packs: Vec<_> = sizes
            .iter()
            .enumerate()
            .map(|(p, &size)| KnowledgePack {
                format: "try".into(),
                schema_version: 1,
                id: format!("pack_{p}"),
                name: format!("Pack {p}"),
                tag: String::new(),
                description: String::new(),
                cards: (0..size)
                    .map(|c| Card {
                        id: Some(format!("q{c}")),
                        prompt: format!("cue{c}"),
                        answer: format!("answer{c}"),
                        prompt_latex: None,
                        answer_latex: None,
                        aliases: vec![],
                        distractors: vec![
                            AnswerContent {
                                text: "other".into(),
                                latex: None,
                            },
                            AnswerContent {
                                text: "different".into(),
                                latex: None,
                            },
                        ],
                        true_false: Some(TrueFalse {
                            statement: format!("cue{c}"),
                            statement_latex: None,
                            is_true: true,
                            correction: format!("answer{c}"),
                            correction_latex: None,
                        }),
                    })
                    .collect(),
            })
            .collect();
        let library = Library {
            format: "try_state".into(),
            schema_version: 1,
            settings: Settings {
                enabled_pack_ids: packs.iter().map(|p| p.id.clone()).collect(),
                ..Settings::default()
            },
            packs,
        };
        library.validate().unwrap();
        library
    }

    fn question(library: &Library, card: usize) -> Question {
        let pack = &library.packs[0];
        Question {
            key: key(&pack.id, &pack.cards[card]),
            card: pack.cards[card].clone(),
        }
    }

    fn loaded(store: &Store, library: &Library) -> Learner {
        let mut learner = Learner::new(store);
        learner
            .choose(library, None, NOW, &mut StdRng::seed_from_u64(7))
            .unwrap();
        learner
    }

    fn review(
        learner: &mut Learner,
        library: &Library,
        card: usize,
        mode: AnswerMode,
        correct: bool,
        now: i64,
    ) -> ReviewToken {
        let token = learner
            .presented(&question(library, card), mode, now, Origin::Manual)
            .unwrap();
        assert!(learner.answer(library, &token, correct, now).unwrap());
        token
    }

    #[test]
    fn binary_result_commits_once_and_restarts_without_changing_content_state() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let library = library(&[4]);
        store.save(&library).unwrap();
        let before = std::fs::read(directory.path().join("state.json")).unwrap();
        let mut learner = loaded(&store, &library);
        assert!(!directory.path().join("learning_state.json").exists());
        let token = review(&mut learner, &library, 0, AnswerMode::Input, true, NOW);
        let saved = std::fs::read(directory.path().join("learning_state.json")).unwrap();
        assert!(!learner.answer(&library, &token, false, NOW + 1).unwrap());
        assert_eq!(
            saved,
            std::fs::read(directory.path().join("learning_state.json")).unwrap()
        );
        let restart = loaded(&store, &library);
        let summary = restart.pack_summary(&library, "pack_0", NOW + 86400 * 7);
        assert_eq!(summary.participating, 4);
        assert_eq!(summary.assessed, 1);
        assert_eq!(summary.effective_reviews, 1);
        assert!(summary.current_retrievability.unwrap() < 0.9);
        assert_eq!(
            before,
            std::fs::read(directory.path().join("state.json")).unwrap()
        );
    }

    #[test]
    fn modes_are_separate_and_content_changes_guard_summary_and_updates() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let mut library = library(&[3]);
        let mut learner = loaded(&store, &library);
        review(&mut learner, &library, 0, AnswerMode::Input, true, NOW);
        library
            .set_card_answer_mode("pack_0", "q0", AnswerMode::TrueFalse)
            .unwrap();
        assert_eq!(learner.pack_summary(&library, "pack_0", NOW).assessed, 0);
        review(
            &mut learner,
            &library,
            0,
            AnswerMode::TrueFalse,
            false,
            NOW + 60,
        );
        library
            .set_card_answer_mode("pack_0", "q0", AnswerMode::Input)
            .unwrap();
        assert_eq!(
            learner
                .pack_summary(&library, "pack_0", NOW)
                .effective_reviews,
            1
        );
        let token = learner
            .presented(
                &question(&library, 0),
                AnswerMode::Input,
                NOW + 86400,
                Origin::Reminder,
            )
            .unwrap();
        library.packs[0].name = "Renamed".into();
        library.packs[0].tag = "New tag".into();
        library.packs[0].cards.reverse();
        assert_eq!(learner.pack_summary(&library, "pack_0", NOW).assessed, 1);
        let card = library.packs[0]
            .cards
            .iter_mut()
            .find(|c| c.id.as_deref() == Some("q0"))
            .unwrap();
        card.answer_latex = Some("x".into());
        assert_eq!(learner.pack_summary(&library, "pack_0", NOW).assessed, 0);
        assert!(!learner.answer(&library, &token, true, NOW + 86400).unwrap());
        learner.reconcile(&library).unwrap();
        let restart = loaded(&store, &library);
        assert_eq!(restart.pack_summary(&library, "pack_0", NOW).assessed, 0);
        library
            .set_card_answer_mode("pack_0", "q0", AnswerMode::TrueFalse)
            .unwrap();
        assert_eq!(restart.pack_summary(&library, "pack_0", NOW).assessed, 1);
    }

    #[test]
    fn every_math_and_judgment_field_has_a_stable_mode_specific_guard() {
        let library = library(&[1]);
        let original = &library.packs[0].cards[0];
        let mut edited = original.clone();
        edited.prompt_latex = Some("x".into());
        assert_ne!(
            fingerprint(original, AnswerMode::Input),
            fingerprint(&edited, AnswerMode::Input)
        );
        assert_ne!(
            fingerprint(original, AnswerMode::MultipleChoice),
            fingerprint(&edited, AnswerMode::MultipleChoice)
        );
        assert_eq!(
            fingerprint(original, AnswerMode::TrueFalse),
            fingerprint(&edited, AnswerMode::TrueFalse)
        );
        edited = original.clone();
        edited.distractors[0].latex = Some("y".into());
        assert_ne!(
            fingerprint(original, AnswerMode::MultipleChoice),
            fingerprint(&edited, AnswerMode::MultipleChoice)
        );
        edited = original.clone();
        edited.true_false.as_mut().unwrap().statement_latex = Some("z".into());
        assert_ne!(
            fingerprint(original, AnswerMode::TrueFalse),
            fingerprint(&edited, AnswerMode::TrueFalse)
        );
        edited = original.clone();
        edited.true_false.as_mut().unwrap().is_true = false;
        assert_ne!(
            fingerprint(original, AnswerMode::TrueFalse),
            fingerprint(&edited, AnswerMode::TrueFalse)
        );
    }

    #[test]
    fn removal_cleans_records_and_disabling_retains_them() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let mut library = library(&[2]);
        let mut learner = loaded(&store, &library);
        review(&mut learner, &library, 0, AnswerMode::Input, true, NOW);
        library.set_card_enabled("pack_0", "q0", false).unwrap();
        learner.reconcile(&library).unwrap();
        assert_eq!(learner.pack_summary(&library, "pack_0", NOW).assessed, 0);
        library.set_card_enabled("pack_0", "q0", true).unwrap();
        assert_eq!(learner.pack_summary(&library, "pack_0", NOW).assessed, 1);
        library.delete_card("pack_0", "q0").unwrap();
        learner.reconcile(&library).unwrap();
        let data = std::fs::read_to_string(directory.path().join("learning_state.json")).unwrap();
        assert!(!data.contains("\"q0\""));
    }

    #[test]
    fn corrupt_or_future_profiles_are_preserved_and_fallback_is_usable() {
        for bytes in [
            b"not json".as_slice(),
            br#"{"format":"try_learning","schema_version":99}"#.as_slice(),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let store = Store::open(directory.path().into()).unwrap();
            let path = directory.path().join("learning_state.json");
            std::fs::write(&path, bytes).unwrap();
            let library = library(&[2]);
            let mut learner = Learner::new(&store);
            let mut rng = StdRng::seed_from_u64(3);
            assert!(learner.choose(&library, None, NOW, &mut rng).is_err());
            let selected = learner
                .choose(&library, None, NOW, &mut rng)
                .unwrap()
                .unwrap();
            assert!(
                learner
                    .presented(&selected, AnswerMode::Input, NOW, Origin::Manual)
                    .is_none()
            );
            assert_eq!(bytes, std::fs::read(&path).unwrap());
        }
    }

    #[test]
    fn failed_save_does_not_commit_memory_and_can_recover() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let library = library(&[1]);
        let mut learner = loaded(&store, &library);
        let token = learner
            .presented(
                &question(&library, 0),
                AnswerMode::Input,
                NOW,
                Origin::Manual,
            )
            .unwrap();
        let path = directory.path().join("learning_state.json");
        std::fs::create_dir(&path).unwrap(); // Cannot atomically replace a directory with a file.
        assert!(learner.answer(&library, &token, true, NOW).is_err());
        assert_eq!(learner.pack_summary(&library, "pack_0", NOW).assessed, 0);
        std::fs::remove_dir(&path).unwrap();
        assert!(learner.answer(&library, &token, true, NOW).unwrap());
        assert_eq!(
            loaded(&store, &library)
                .pack_summary(&library, "pack_0", NOW)
                .effective_reviews,
            1
        );
    }

    #[test]
    fn clock_reversal_and_stale_session_do_not_create_false_results() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let library = library(&[2]);
        let mut learner = loaded(&store, &library);
        let older = learner
            .presented(
                &question(&library, 0),
                AnswerMode::Input,
                NOW,
                Origin::Manual,
            )
            .unwrap();
        let current = learner
            .presented(
                &question(&library, 1),
                AnswerMode::Input,
                NOW,
                Origin::Manual,
            )
            .unwrap();
        assert!(!learner.answer(&library, &older, true, NOW).unwrap());
        assert!(learner.answer(&library, &current, true, NOW - 1).is_err());
        assert!(!directory.path().join("learning_state.json").exists());
    }

    #[test]
    fn crossing_midnight_does_not_invent_a_day_and_delayed_success_does_strengthen() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let library = library(&[1]);
        let mut learner = loaded(&store, &library);
        let before_midnight = 86400 * 20000 - 60;
        review(
            &mut learner,
            &library,
            0,
            AnswerMode::Input,
            true,
            before_midnight,
        );
        let initial = learner.pack_summary(&library, "pack_0", before_midnight);
        review(
            &mut learner,
            &library,
            0,
            AnswerMode::Input,
            true,
            before_midnight + 120,
        );
        let after = learner.pack_summary(&library, "pack_0", before_midnight + 120);
        assert_eq!(initial.familiarity, after.familiarity);
        assert_eq!(after.delayed_reviews, 0);
        review(
            &mut learner,
            &library,
            0,
            AnswerMode::Input,
            true,
            before_midnight + 120 + 86400,
        );
        let after_day = learner.pack_summary(&library, "pack_0", before_midnight + 120 + 86400);
        assert!(after_day.familiarity > initial.familiarity);
        assert_eq!(after_day.delayed_reviews, 1);
    }

    #[test]
    fn sum_selection_keeps_equal_items_equal_across_different_pack_sizes() {
        let library = library(&[2, 20]);
        let state = LearningState::default();
        let mut rng = StdRng::seed_from_u64(20261013);
        let small = (0..6000)
            .filter(|_| {
                state
                    .select(&library, None, NOW, &mut rng)
                    .unwrap()
                    .key
                    .pack_id
                    == "pack_0"
            })
            .count();
        assert!((450..=650).contains(&small), "{small}"); // Expected 6000 * 2/22, not 3000.
    }

    #[test]
    fn wrong_item_gets_more_opportunities_without_becoming_a_domain_bonus() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let library = library(&[20]);
        let mut learner = loaded(&store, &library);
        for c in 0..20 {
            review(&mut learner, &library, c, AnswerMode::Input, c != 0, NOW);
        }
        let mut rng = StdRng::seed_from_u64(10);
        let wrong = (0..3000)
            .filter(|_| {
                learner
                    .choose(&library, None, NOW + 86400, &mut rng)
                    .unwrap()
                    .unwrap()
                    .key
                    .card_id
                    == "q0"
            })
            .count();
        assert!((800..=1300).contains(&wrong), "{wrong}"); // Uniform baseline is 150.
    }

    #[test]
    fn coverage_reaches_new_items_and_old_items_survive_continuous_new_content() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let mut library = library(&[20]);
        let mut learner = loaded(&store, &library);
        let mut rng = StdRng::seed_from_u64(20);
        let mut seen = std::collections::BTreeSet::new();
        let mut previous = None;
        for i in 0..100 {
            let chosen = learner
                .choose(&library, previous.as_ref(), NOW + i * 1800, &mut rng)
                .unwrap()
                .unwrap();
            assert_ne!(previous.as_ref(), Some(&chosen.key));
            seen.insert(chosen.key.card_id.clone());
            learner
                .presented(&chosen, AnswerMode::Input, NOW + i * 1800, Origin::Reminder)
                .unwrap();
            previous = Some(chosen.key);
        }
        assert_eq!(seen.len(), 20);
        let mut old_coverage = 0;
        for i in 100..200 {
            let mut new = library.packs[0].cards[0].clone();
            new.id = Some(format!("new{i}"));
            library.packs[0].cards.push(new);
            let chosen = learner
                .choose(&library, previous.as_ref(), NOW + i * 1800, &mut rng)
                .unwrap()
                .unwrap();
            if chosen.key.card_id.starts_with('q') {
                old_coverage += 1;
            }
            learner
                .presented(&chosen, AnswerMode::Input, NOW + i * 1800, Origin::Reminder)
                .unwrap();
            previous = Some(chosen.key);
        }
        assert!(old_coverage >= 8, "{old_coverage}");
    }

    #[test]
    fn one_and_two_item_pools_remain_usable_and_failures_are_interleaved() {
        for size in [1, 2, 10] {
            let directory = tempfile::tempdir().unwrap();
            let store = Store::open(directory.path().into()).unwrap();
            let library = library(&[size]);
            let mut learner = loaded(&store, &library);
            let mut rng = StdRng::seed_from_u64(11);
            let mut previous = None;
            let mut failed_at = Vec::new();
            for i in 0..50 {
                let now = NOW + i * 1800;
                let chosen = learner
                    .choose(&library, previous.as_ref(), now, &mut rng)
                    .unwrap()
                    .unwrap();
                if size != 1 {
                    assert_ne!(previous.as_ref(), Some(&chosen.key));
                }
                let token = learner
                    .presented(&chosen, AnswerMode::Input, now, Origin::Reminder)
                    .unwrap();
                let failed = chosen.key.card_id == "q0";
                if failed {
                    failed_at.push(i);
                }
                learner.answer(&library, &token, !failed, now).unwrap();
                previous = Some(chosen.key);
            }
            if size == 1 {
                assert_eq!(failed_at.len(), 50);
            }
            if size == 2 {
                assert_eq!(failed_at.len(), 25);
            }
            if size == 10 {
                assert!(failed_at.len() <= 14, "{failed_at:?}");
                assert!(
                    failed_at.windows(2).all(|w| w[1] - w[0] >= 3),
                    "{failed_at:?}"
                );
            }
        }
    }

    #[test]
    #[ignore = "measures the largest supported profile in isolated temporary storage"]
    fn maximum_profile_resource_check() {
        let mut library = library(&[2000; 10]);
        for (p, pack) in library.packs.iter_mut().enumerate() {
            pack.id = format!("p{p:0>79}");
            for (c, card) in pack.cards.iter_mut().enumerate() {
                card.id = Some(format!("q{c:0>79}"));
            }
        }
        library.settings.enabled_pack_ids = library.packs.iter().map(|p| p.id.clone()).collect();
        library.validate().unwrap();
        let mut state = LearningState::default();
        for pack in &library.packs {
            for card in &pack.cards {
                let key = key(&pack.id, card);
                let mut progress = CardProgress::default();
                for mode in AnswerMode::ALL {
                    state.presentations += 1;
                    progress.modes[mode.index() as usize] = Some(Progress {
                        fingerprint: fingerprint(card, mode),
                        memory: fsrs6::reviewed(None, 0, true),
                        last_review_at: NOW,
                        last_review_sequence: state.presentations,
                        correct: true,
                        successes: 1,
                        failures: 0,
                        delayed_reviews: 0,
                    });
                }
                let shown = Presentation {
                    key: key.clone(),
                    content: content_stamp(card),
                    at: NOW,
                    sequence: state.presentations,
                };
                progress.last_shown = Some(shown.clone());
                state.recent.push_back(shown);
                if state.recent.len() > RECENT_WINDOW {
                    state.recent.pop_front();
                }
                state.reviews.push_back(Review {
                    key,
                    mode: AnswerMode::TrueFalse,
                    fingerprint: fingerprint(card, AnswerMode::TrueFalse),
                    at: NOW,
                    sequence: state.presentations,
                    correct: true,
                    origin: Origin::Reminder,
                });
                if state.reviews.len() > MAX_REVIEWS {
                    state.reviews.pop_front();
                }
                state
                    .packs
                    .entry(pack.id.clone())
                    .or_default()
                    .insert(card.id.clone().unwrap(), progress);
            }
        }
        state.validate().unwrap();
        let start = std::time::Instant::now();
        assert!(
            state
                .select(&library, None, NOW + 86400, &mut StdRng::seed_from_u64(1))
                .is_some()
        );
        let selection_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = std::time::Instant::now();
        state.reconcile(&library);
        let reconciliation_ms = start.elapsed().as_secs_f64() * 1000.0;
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let persistence = LearningStore::new(&store);
        let start = std::time::Instant::now();
        persistence.save(&state).unwrap();
        let save_ms = start.elapsed().as_secs_f64() * 1000.0;
        let bytes = std::fs::metadata(persistence.path()).unwrap().len();
        assert!(bytes < crate::learning_store::MAX_LEARNING_BYTES);
        let start = std::time::Instant::now();
        assert_eq!(persistence.load().unwrap().packs.len(), 10);
        let load_ms = start.elapsed().as_secs_f64() * 1000.0;
        let result = serde_json::json!({ "cards":20000, "mode_states":60000, "recent_reviews":MAX_REVIEWS,
            "bytes":bytes, "selection_ms":selection_ms, "reconciliation_ms":reconciliation_ms,
            "save_ms":save_ms, "load_ms":load_ms });
        let output = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local_artifacts/adaptive_practice_build/scale.json");
        std::fs::write(output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        println!("{result}");
    }
}
