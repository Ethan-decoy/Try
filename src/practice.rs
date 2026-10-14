use crate::knowledge::{AnswerContent, AnswerMode, AppResult, Card, KnowledgePack, Library};
use crate::tr;
use rand::{Rng, seq::SliceRandom};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionKey {
    pub pack_id: String,
    pub card_id: String,
}

#[derive(Clone, Debug)]
pub struct Question {
    pub key: QuestionKey,
    pub card: Card,
}

/// Sample participating cards uniformly, excluding the last card when another
/// is available. This policy has no UI, timers, persisted state or answer history.
pub fn select_question(
    library: &Library,
    previous: Option<&QuestionKey>,
    rng: &mut impl Rng,
) -> Option<Question> {
    let differs = |pack: &KnowledgePack, card: &Card| {
        previous.is_none_or(|key| {
            key.pack_id != pack.id || Some(key.card_id.as_str()) != card.id.as_deref()
        })
    };
    let available = library
        .participating_cards()
        .filter(|(pack, card)| differs(pack, card))
        .count();
    let exclude_previous = available > 0;
    let count = if exclude_previous {
        available
    } else {
        library.participating_cards().count()
    };
    if count == 0 {
        return None;
    }
    let choice = rng.random_range(0..count);
    library
        .participating_cards()
        .filter(|(pack, card)| !exclude_previous || differs(pack, card))
        .nth(choice)
        .map(|(pack, card)| Question {
            key: QuestionKey {
                pack_id: pack.id.clone(),
                card_id: card
                    .id
                    .clone()
                    .expect("local cards have stable identifiers"),
            },
            card: card.clone(),
        })
}

/// Prepared once per card. DPI changes only rasterize these contents again.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedQuestion {
    pub mode: AnswerMode,
    pub knowledge_only: bool,
    pub prompt: AnswerContent,
    pub answer: AnswerContent,
    pub options: Vec<AnswerContent>,
    correct_index: Option<usize>,
}

impl PreparedQuestion {
    pub fn new(card: &Card, mode: AnswerMode, rng: &mut impl Rng) -> AppResult<Self> {
        let mut result = Self {
            mode,
            knowledge_only: false,
            prompt: AnswerContent {
                text: card.prompt.clone(),
                latex: card.prompt_latex.clone(),
            },
            answer: AnswerContent {
                text: card.answer.clone(),
                latex: card.answer_latex.clone(),
            },
            options: vec![],
            correct_index: None,
        };
        match mode {
            AnswerMode::Input => {}
            AnswerMode::MultipleChoice => {
                if card.distractors.len() != 2 {
                    return Err(tr!("选择题需要两个明确的干扰项。").into());
                }
                let mut choices = vec![(true, result.answer.clone())];
                choices.extend(
                    card.distractors
                        .iter()
                        .cloned()
                        .map(|option| (false, option)),
                );
                choices.shuffle(rng);
                result.correct_index = choices.iter().position(|(correct, _)| *correct);
                result.options = choices.into_iter().map(|(_, option)| option).collect();
            }
            AnswerMode::TrueFalse => {
                let statement = card
                    .true_false
                    .as_ref()
                    .ok_or(tr!("判断题需要一个明确的陈述。"))?;
                result.prompt = AnswerContent {
                    text: statement.statement.clone(),
                    latex: statement.statement_latex.clone(),
                };
                result.answer = AnswerContent {
                    text: if statement.is_true { "✓" } else { "✕" }.into(),
                    latex: None,
                };
                result.options = [tr!("正确"), tr!("不正确")]
                    .map(|text| AnswerContent {
                        text: text.into(),
                        latex: None,
                    })
                    .into();
                result.correct_index = Some(usize::from(!statement.is_true));
            }
        }
        Ok(result)
    }

    /// Display the canonical fact. Judgment-only cards pair their statement with ✓ / ✕.
    /// Only explicit equation slots ('= ?' or LaTeX '{?}') are replaced.
    /// Other prompts keep a separate answer rather than guessing a new sentence.
    pub fn knowledge(card: &Card) -> Self {
        let mut result = Self {
            mode: AnswerMode::Input,
            knowledge_only: true,
            prompt: AnswerContent {
                text: card.prompt.clone(),
                latex: card.prompt_latex.clone(),
            },
            answer: AnswerContent {
                text: card.answer.clone(),
                latex: card.answer_latex.clone(),
            },
            options: vec![],
            correct_index: None,
        };
        let equation = |text: &str| {
            text.trim_end()
                .strip_suffix('?')
                .map(str::trim_end)
                .filter(|prefix| prefix.ends_with('='))
                .map(str::to_owned)
        };
        let merged = match (&card.prompt_latex, &card.answer_latex) {
            (None, None) => {
                equation(&card.prompt).map(|prefix| (format!("{prefix} {}", card.answer), None))
            }
            (Some(prompt), Some(answer))
                if card.prompt.contains('=')
                    && card.prompt.matches('?').count() == 1
                    && prompt.matches('?').count() == 1
                    && (equation(prompt).is_some() || prompt.contains("{?}")) =>
            {
                Some((
                    card.prompt.replace('?', &card.answer),
                    Some(prompt.replace('?', &format!("{{{answer}}}"))),
                ))
            }
            _ => None,
        };
        if let Some((text, latex)) = merged {
            result.prompt = AnswerContent { text, latex };
            result.answer = AnswerContent {
                text: String::new(),
                latex: None,
            };
        }
        result
    }

    pub fn accepts_choice(&self, index: usize) -> Option<bool> {
        (index < self.options.len()).then(|| self.correct_index == Some(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};

    #[test]
    fn every_starter_domain_can_be_practiced_independently() {
        let mut library = Library::default();
        let pack_ids: Vec<_> = library.packs.iter().map(|p| p.id.clone()).collect();
        let mut rng = StdRng::seed_from_u64(41);
        for id in pack_ids {
            library.settings.enabled_pack_ids = vec![id.clone()];
            for _ in 0..12 {
                let question = select_question(&library, None, &mut rng).unwrap();
                assert_eq!(question.key.pack_id, id);
                let mode = library.card_answer_mode(&question.key.pack_id, &question.key.card_id);
                let prepared = PreparedQuestion::new(&question.card, mode, &mut rng).unwrap();
                assert!(question.card.accepts(&prepared.answer.text));
            }
        }
    }

    #[test]
    fn selection_respects_participation_and_avoids_immediate_repeat() {
        let mut library = crate::test_fixtures::regression_library();
        let mut rng = StdRng::seed_from_u64(41);
        let first = select_question(&library, None, &mut rng).unwrap();
        for _ in 0..30 {
            let next = select_question(&library, Some(&first.key), &mut rng).unwrap();
            assert_ne!(next.key, first.key);
            assert_eq!(next.key.pack_id, "test_pack");
        }
        library.set_pack_enabled("test_pack", false).unwrap();
        assert!(select_question(&library, None, &mut rng).is_none());
    }

    #[test]
    fn selection_can_repeat_the_only_participating_card() {
        let mut library = crate::test_fixtures::regression_library();
        let pack_id = library.packs[0].id.clone();
        let card_id = library.packs[0].cards[0].id.clone().unwrap();
        library.set_pack_enabled(&pack_id, false).unwrap();
        library.set_card_enabled(&pack_id, &card_id, true).unwrap();
        let mut rng = StdRng::seed_from_u64(41);
        let first = select_question(&library, None, &mut rng).unwrap();
        assert_eq!(
            select_question(&library, Some(&first.key), &mut rng)
                .unwrap()
                .key,
            first.key
        );
    }

    #[test]
    fn selection_qualifies_card_identity_with_the_pack() {
        let mut library = crate::test_fixtures::regression_library();
        library.packs[0].cards.truncate(1);
        let mut other = library.packs[0].clone();
        other.id = "other_pack".into();
        library.import(other.clone(), false).unwrap();
        let mut rng = StdRng::seed_from_u64(41);
        let first = select_question(&library, None, &mut rng).unwrap();
        let next = select_question(&library, Some(&first.key), &mut rng).unwrap();
        assert_ne!(next.key.pack_id, first.key.pack_id);
        assert_eq!(next.key.card_id, first.key.card_id);
    }

    #[test]
    fn choices_have_one_correct_answer_and_preserve_formula_pairing() {
        let library = crate::test_fixtures::regression_library();
        let mut rng = StdRng::seed_from_u64(41);
        let mut correct_positions = std::collections::HashSet::new();
        for card in &library.packs[0].cards {
            let prepared =
                PreparedQuestion::new(card, AnswerMode::MultipleChoice, &mut rng).unwrap();
            assert_eq!(prepared.options.len(), 3);
            let correct: Vec<_> = (0..3)
                .filter(|index| prepared.accepts_choice(*index) == Some(true))
                .collect();
            assert_eq!(correct.len(), 1);
            correct_positions.insert(correct[0]);
            assert_eq!(prepared.options[correct[0]].text, card.answer);
            assert_eq!(prepared.options[correct[0]].latex, card.answer_latex);
            assert_eq!(prepared.accepts_choice(3), None);
            let snapshot = prepared.clone();
            prepared.accepts_choice(correct[0]);
            assert_eq!(prepared, snapshot, "Judging never changes option order");
        }
        assert_eq!(
            correct_positions.len(),
            3,
            "Correct answers can occur in every slot"
        );
    }

    #[test]
    fn true_false_uses_only_declared_truth_and_symbol_feedback() {
        let library = crate::test_fixtures::regression_library();
        let mut rng = StdRng::seed_from_u64(41);
        let mut truths = std::collections::HashSet::new();
        for card in &library.packs[0].cards {
            let prepared = PreparedQuestion::new(card, AnswerMode::TrueFalse, &mut rng).unwrap();
            let statement = card.true_false.as_ref().unwrap();
            truths.insert(statement.is_true);
            assert_eq!(prepared.prompt.text, statement.statement);
            assert_eq!(
                prepared.answer.text,
                if statement.is_true { "✓" } else { "✕" }
            );
            assert!(prepared.answer.latex.is_none());
            assert_eq!(prepared.accepts_choice(0), Some(statement.is_true));
            assert_eq!(prepared.accepts_choice(1), Some(!statement.is_true));
            assert_eq!(prepared.options.len(), 2);
        }
        assert_eq!(truths.len(), 2);
    }

    #[test]
    fn knowledge_shows_the_fact_without_false_statements_or_answer_options() {
        let library = crate::test_fixtures::regression_library();
        for card in &library.packs[0].cards {
            let original = card.clone();
            let shown = PreparedQuestion::knowledge(card);
            assert!(shown.knowledge_only);
            assert!(shown.options.is_empty());
            assert_eq!(shown.accepts_choice(0), None);
            assert!(!shown.prompt.text.contains('?'));
            assert!(shown.prompt.text.ends_with(&card.answer));
            crate::formula::validate(shown.prompt.latex.as_ref().unwrap()).unwrap();
            assert!(shown.answer.text.is_empty());
            assert_eq!(*card, original);
        }
        let mut card = library.packs[0].cards[0].clone();
        card.prompt = "尝试的英文是什么？".into();
        card.prompt_latex = None;
        card.answer = "try".into();
        card.answer_latex = None;
        let shown = PreparedQuestion::knowledge(&card);
        assert_eq!(shown.prompt.text, card.prompt);
        assert_eq!(shown.answer.text, card.answer);
    }
}
