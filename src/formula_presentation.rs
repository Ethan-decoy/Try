//! Adapt formula pixels and native text into the shared Slint display model.
//! The caller owns each result; this module retains no images or window state.

use crate::{AnswerOption, NativeText, PracticeWindow};
use slint::{ModelRc, SharedString, VecModel};
use r#try::formula::{self, RenderedFormula};
use r#try::knowledge::AnswerContent;
use r#try::practice::PreparedQuestion;

pub(crate) fn option(label: SharedString, rendered: RenderedFormula) -> AnswerOption {
    AnswerOption {
        label,
        formula: rendered.image,
        formula_width: rendered.width,
        formula_height: rendered.height,
        texts: ModelRc::new(VecModel::from(
            rendered
                .texts
                .into_iter()
                .map(|text| NativeText {
                    text: text.text.into(),
                    x: text.x,
                    y: text.y,
                    width: text.width,
                    height: text.height,
                    font_size: text.font_size,
                })
                .collect::<Vec<_>>(),
        )),
    }
}

pub(crate) fn content(content: &AnswerContent, size: u16, scale: f32) -> AnswerOption {
    let mut result = AnswerOption {
        label: content.text.clone().into(),
        ..Default::default()
    };
    if let Some(tex) = &content.latex {
        match formula::render(tex, size, scale, (232, 232, 232)) {
            Ok(rendered) => result = option(result.label, rendered),
            Err(error) => log::warn!(target: "try::app",
                "event=formula_text_fallback size={size} scale={scale} reason={error}"
            ),
        }
    }
    result
}

/// Rasterizing at a new display scale never prepares or reshuffles a question.
pub(crate) fn apply_practice(window: &PracticeWindow, prepared: &PreparedQuestion, scale: f32) {
    let prompt = content(&prepared.prompt, 18, scale);
    window.set_prompt_formula(prompt.formula);
    window.set_prompt_formula_width(prompt.formula_width);
    window.set_prompt_formula_height(prompt.formula_height);
    window.set_prompt_texts(prompt.texts);
    let answer = content(&prepared.answer, 15, scale);
    window.set_answer_formula(answer.formula);
    window.set_answer_formula_width(answer.formula_width);
    window.set_answer_formula_height(answer.formula_height);
    window.set_answer_texts(answer.texts);
    let options: Vec<_> = prepared
        .options
        .iter()
        .map(|option| content(option, 15, scale))
        .collect();
    window.set_option_formula_height(
        options
            .iter()
            .map(|option| option.formula_height)
            .fold(0.0, f32::max),
    );
    window.set_options(ModelRc::new(VecModel::from(options)));
}
