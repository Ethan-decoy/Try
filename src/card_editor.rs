use crate::knowledge::{AnswerContent, AnswerMode, AppResult, Card, TrueFalse};
use crate::tr;

/// A quick edit updates the canonical answer while retaining the rest of the card.
pub fn with_answer(card: &Card, input: &str) -> AppResult<Card> {
    let input = input.trim();
    if input.is_empty() || input.chars().count() > 1000 {
        return Err(tr!("答案须为 1 至 1000 个字符。").into());
    }
    if input == card.answer.trim() {
        return Ok(card.clone());
    }
    let node = if input.contains('\\') {
        crate::math_editor::from_latex(input)?
    } else if card.answer_latex.is_some() {
        crate::math_editor::expression(input)?
    } else {
        crate::math_editor::automatic_content(input)?
    };
    if !node.complete() {
        return Err(tr!("还有未填写的公式位置。").into());
    }
    node.validate()?;
    let mut updated = card.clone();
    updated.answer = node.plain();
    updated.answer_latex = (!node.is_text()).then(|| node.tex());
    if updated.answer.trim() != card.answer.trim()
        && !crate::math_answer::matches(
            &updated.answer,
            &card.answer,
            card.answer_latex.as_deref(),
            &[],
        )
    {
        updated.aliases.clear();
    }
    Ok(updated)
}

/// Text fields stay in a draft until the complete card passes library validation.
#[derive(Clone, Debug, Default)]
pub struct CardDraft {
    pub prompt_image: Option<String>,
    pub prompt: String,
    pub answer: String,
    pub aliases: String,
    pub prompt_latex: String,
    pub answer_latex: String,
    pub choice_one: String,
    pub choice_two: String,
    pub choice_one_latex: String,
    pub choice_two_latex: String,
    pub has_judgment: bool,
    pub statement: String,
    pub statement_latex: String,
    pub is_true: bool,
    pub correction: String,
    pub correction_latex: String,
}

impl From<&Card> for CardDraft {
    fn from(card: &Card) -> Self {
        let mut draft = Self {
            prompt_image: card.prompt_image.clone(),
            prompt: card.prompt.clone(),
            answer: card.answer.clone(),
            aliases: card.aliases.join("\n"),
            prompt_latex: card.prompt_latex.clone().unwrap_or_default(),
            answer_latex: card.answer_latex.clone().unwrap_or_default(),
            ..Self::default()
        };
        for (index, option) in card.distractors.iter().enumerate() {
            if index == 0 {
                draft.choice_one = option.text.clone();
                draft.choice_one_latex = option.latex.clone().unwrap_or_default();
            }
            if index == 1 {
                draft.choice_two = option.text.clone();
                draft.choice_two_latex = option.latex.clone().unwrap_or_default();
            }
        }
        if let Some(statement) = &card.true_false {
            draft.has_judgment = true;
            draft.statement = statement.statement.clone();
            draft.statement_latex = statement.statement_latex.clone().unwrap_or_default();
            draft.is_true = statement.is_true;
            draft.correction = statement.correction.clone();
            draft.correction_latex = statement.correction_latex.clone().unwrap_or_default();
        }
        draft
    }
}

impl CardDraft {
    pub fn into_card_for_mode(mut self, mode: AnswerMode, correct_choice: usize) -> Card {
        if mode == AnswerMode::MultipleChoice {
            match correct_choice {
                1 => {
                    std::mem::swap(&mut self.answer, &mut self.choice_one);
                    std::mem::swap(&mut self.answer_latex, &mut self.choice_one_latex);
                    self.aliases.clear();
                }
                2 => {
                    std::mem::swap(&mut self.answer, &mut self.choice_two);
                    std::mem::swap(&mut self.answer_latex, &mut self.choice_two_latex);
                    self.aliases.clear();
                }
                _ => {}
            }
        }
        if mode == AnswerMode::TrueFalse {
            self.has_judgment = true;
            let symbolic_answer = matches!(self.answer.trim(), "" | "✓" | "✕");
            if (self.prompt.trim().is_empty() && self.prompt_image.is_none()) || symbolic_answer {
                self.prompt.clone_from(&self.statement);
                self.prompt_latex.clone_from(&self.statement_latex);
            }
            let symbol = if self.is_true { "✓" } else { "✕" };
            if symbolic_answer {
                self.answer = symbol.into();
                self.answer_latex.clear();
                self.aliases.clear();
            }
            if matches!(self.correction.trim(), "" | "✓" | "✕") {
                self.correction = symbol.into();
                self.correction_latex.clear();
            }
        }
        self.into_card()
    }

    pub fn into_card(self) -> Card {
        let mut distractors = vec![];
        for (text, latex) in [
            (self.choice_one, self.choice_one_latex),
            (self.choice_two, self.choice_two_latex),
        ] {
            if !text.trim().is_empty() || !latex.trim().is_empty() {
                distractors.push(AnswerContent {
                    text,
                    latex: optional(latex),
                });
            }
        }
        let correction = if self.has_judgment && self.is_true && self.correction.trim().is_empty() {
            (self.statement.clone(), self.statement_latex.clone())
        } else {
            (self.correction, self.correction_latex)
        };
        Card {
            prompt_image: self.prompt_image,
            id: None,
            prompt: self.prompt,
            answer: self.answer,
            aliases: self
                .aliases
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            prompt_latex: optional(self.prompt_latex),
            answer_latex: optional(self.answer_latex),
            distractors,
            true_false: self.has_judgment.then_some(TrueFalse {
                statement: self.statement,
                statement_latex: optional(self.statement_latex),
                is_true: self.is_true,
                correction: correction.0,
                correction_latex: optional(correction.1),
            }),
        }
    }
}

fn optional(text: String) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_only_prompts_survive_editing_in_every_mode() {
        let mut card = crate::test_fixtures::regression_library().packs[0].cards[0].clone();
        card.prompt.clear();
        card.prompt_latex = None;
        card.prompt_image = Some("a".repeat(64));
        for mode in AnswerMode::ALL {
            let saved = CardDraft::from(&card).into_card_for_mode(mode, 0);
            assert!(saved.prompt.is_empty());
            assert!(saved.prompt_latex.is_none());
            assert_eq!(saved.prompt_image, card.prompt_image);
            assert_eq!(saved.answer, card.answer);
        }
    }

    #[test]
    fn existing_content_round_trips_through_an_editor_draft() {
        for original in &crate::test_fixtures::regression_library().packs[0].cards {
            let mut rebuilt = CardDraft::from(original).into_card();
            rebuilt.id = original.id.clone();
            assert_eq!(&rebuilt, original);
        }
    }

    #[test]
    fn true_statement_can_reuse_its_own_content_as_feedback() {
        let card = CardDraft {
            prompt: "A fact?".into(),
            answer: "Yes".into(),
            has_judgment: true,
            is_true: true,
            statement: "A fact is true".into(),
            ..Default::default()
        }
        .into_card();
        assert_eq!(card.true_false.unwrap().correction, "A fact is true");
    }

    #[test]
    fn selecting_a_correct_option_moves_its_formula_and_discards_old_answer_aliases() {
        let draft = CardDraft {
            prompt: "1+1=?".into(),
            answer: "3".into(),
            aliases: "three".into(),
            choice_one: "2".into(),
            choice_one_latex: "2".into(),
            choice_two: "4".into(),
            ..Default::default()
        };
        let card = draft.into_card_for_mode(AnswerMode::MultipleChoice, 1);
        assert_eq!(card.answer, "2");
        assert_eq!(card.answer_latex.as_deref(), Some("2"));
        assert!(card.aliases.is_empty());
        assert_eq!(card.distractors[0].text, "3");
        assert!(card.distractors[0].latex.is_none());
    }

    #[test]
    fn judgment_needs_only_a_statement_and_truth_choice() {
        let card = CardDraft {
            statement: "1+1=3".into(),
            is_true: false,
            ..Default::default()
        }
        .into_card_for_mode(AnswerMode::TrueFalse, 0);
        assert_eq!(card.prompt, "1+1=3");
        assert_eq!(card.answer, "✕");
        assert_eq!(card.true_false.as_ref().unwrap().correction, "✕");
        let mut pack = crate::test_fixtures::regression_library().packs[0].clone();
        pack.cards = vec![card.clone()];
        pack.validate().unwrap();

        let mut edited = CardDraft::from(&card);
        edited.statement = "1+1=2".into();
        edited.is_true = true;
        let updated = edited.into_card_for_mode(AnswerMode::TrueFalse, 0);
        assert_eq!(updated.prompt, "1+1=2");
        assert_eq!(updated.answer, "✓");
        assert_eq!(updated.true_false.unwrap().correction, "✓");

        let fact = &crate::test_fixtures::regression_library().packs[0].cards[2];
        let mut edited = CardDraft::from(fact);
        edited.is_true = !edited.is_true;
        let updated = edited.into_card_for_mode(AnswerMode::TrueFalse, 0);
        assert_eq!(updated.prompt, fact.prompt);
        assert_eq!(updated.answer, fact.answer);
    }

    #[test]
    fn quick_answers_recognize_notation_and_retain_unrelated_card_content() {
        let original = &crate::test_fixtures::regression_library().packs[0].cards[2];
        let same = with_answer(original, r"\frac{\sqrt{3}}{2}").unwrap();
        assert!(same.accepts("sqrt(3)/2"));
        assert_eq!(same.aliases, original.aliases);
        assert_eq!(same.prompt, original.prompt);
        assert_eq!(same.distractors, original.distractors);
        assert_eq!(same.true_false, original.true_false);
        assert_eq!(same.id, original.id);

        let changed = with_answer(original, "3/5").unwrap();
        assert!(changed.accepts("3/5"));
        assert!(!changed.accepts(&original.answer));
        assert!(changed.aliases.is_empty());
        assert!(changed.answer_latex.is_some());
        assert_eq!(changed.prompt_latex, original.prompt_latex);
    }

    #[test]
    fn quick_answers_reject_incomplete_input_and_conflicting_choices_atomically() {
        let mut library = crate::test_fixtures::regression_library();
        let card = library.packs[0].cards[2].clone();
        for input in ["", "sqrt(", "sqrt()/2", r"\frac{1}"] {
            assert!(with_answer(&card, input).is_err(), "{input}");
        }
        assert!(with_answer(&card, &"x".repeat(1001)).is_err());
        let before = library.clone();
        let conflicting = with_answer(&card, &card.distractors[0].text).unwrap();
        assert!(
            library
                .save_card_with_mode(
                    "test_pack",
                    card.id.as_deref(),
                    conflicting,
                    AnswerMode::Input,
                )
                .is_err()
        );
        assert_eq!(library, before);
    }

    #[test]
    fn quick_text_answers_remain_text_and_unchanged_answers_keep_their_source() {
        let card = CardDraft {
            prompt: "尝试的英文？".into(),
            answer: "try".into(),
            aliases: "attempt".into(),
            ..Default::default()
        }
        .into_card();
        assert_eq!(with_answer(&card, " try ").unwrap(), card);
        let changed = with_answer(&card, "give it a try").unwrap();
        assert_eq!(changed.answer, "give it a try");
        assert!(changed.answer_latex.is_none());
        assert!(changed.aliases.is_empty());
    }
}
