use crate::knowledge::{AnswerContent, Card, KnowledgePack, Library, Settings, TrueFalse};

/// Synthetic rows exercise formulas, aliases, answer modes and scrolling without
/// depending on starter content or a private knowledge pack.
pub fn regression_library() -> Library {
    let mut cards = vec![
        formula_card("1/2", r"\frac{1}{2}", &["0.5"], "√2/2", "√3/2", true),
        formula_card(
            "√2/2",
            r"\frac{\sqrt{2}}{2}",
            &["sqrt(2)/2"],
            "1/2",
            "√3/2",
            false,
        ),
        formula_card(
            "√3/2",
            r"\frac{\sqrt{3}}{2}",
            &["sqrt(3)/2"],
            "1/2",
            "√2/2",
            true,
        ),
    ];
    // Preserve enough distinct rows to cover scrolling and bulk participation.
    for value in 4..=20 {
        cards.push(formula_card(
            &value.to_string(),
            &value.to_string(),
            &[],
            &(value + 1).to_string(),
            &(value + 2).to_string(),
            value % 2 == 0,
        ));
    }
    let pack = KnowledgePack {
        format: "try".into(),
        schema_version: 1,
        id: "test_pack".into(),
        name: "回归测试".into(),
        tag: "测试".into(),
        description: "独立构造的回归测试数据。".into(),
        cards,
    };
    pack.validate()
        .expect("the synthetic fixture must be valid");
    let mut library = Library {
        format: "try_state".into(),
        schema_version: 1,
        settings: Settings {
            enabled_pack_ids: vec![pack.id.clone()],
            ..Settings::default()
        },
        packs: vec![pack],
    };
    library.ensure_card_ids();
    library
}

fn formula_card(
    answer: &str,
    latex: &str,
    aliases: &[&str],
    first_distractor: &str,
    second_distractor: &str,
    is_true: bool,
) -> Card {
    let stated_answer = if is_true { answer } else { first_distractor };
    let statement = format!("{answer} = {stated_answer}");
    let correction = format!("{answer} = {answer}");
    Card {
        prompt_image: None,
        id: None,
        prompt: format!("{answer} = ?"),
        answer: answer.into(),
        prompt_latex: Some(format!("{latex}=?")),
        answer_latex: Some(latex.into()),
        aliases: aliases.iter().map(|alias| (*alias).into()).collect(),
        distractors: [first_distractor, second_distractor]
            .map(|text| AnswerContent {
                text: text.into(),
                latex: Some(
                    crate::math_editor::expression(text)
                        .expect("the distractor must be a valid formula")
                        .tex(),
                ),
            })
            .into(),
        true_false: Some(TrueFalse {
            statement,
            statement_latex: Some(format!(
                "{latex}={}",
                if is_true {
                    latex.to_owned()
                } else {
                    crate::math_editor::expression(first_distractor)
                        .expect("the distractor must be a valid formula")
                        .tex()
                }
            )),
            is_true,
            correction,
            correction_latex: Some(format!("{latex}={latex}")),
        }),
    }
}
