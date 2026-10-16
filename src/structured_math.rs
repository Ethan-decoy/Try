//! Try's host adapter for slint-math-input. The editor has no native window.
use crate::{MathInputView, MathKey};
use slint::{ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use slint_math_input::gui::MathInputController;
use r#try::{knowledge::AppResult, math_editor, tr};

pub struct Session {
    controller: MathInputController,
}

pub enum Seed {
    Latex(String),
    Typing(String),
}

impl Session {
    pub fn new(seed: Seed) -> AppResult<Self> {
        let mut controller = MathInputController::new();
        controller.editor.font_size = 22.0;
        match seed {
            Seed::Latex(latex) if !latex.trim().is_empty() => controller
                .editor
                .import_latex(&latex)
                .map_err(|e| e.to_string())?,
            Seed::Typing(text) if !text.is_empty() => {
                controller.insert(&text);
                if let Some(error) = &controller.error {
                    return Err(error.to_string());
                }
            }
            _ => {}
        }
        Ok(Self { controller })
    }
    pub fn insert(&mut self, text: &str) {
        self.controller.insert(text);
    }
    pub fn key(&mut self, text: &str, control: bool, shift: bool, alt: bool) -> bool {
        self.controller.key(text, control, shift, alt)
    }
    pub fn pointer(&mut self, x: f32, y: f32, extend: bool) {
        self.controller.pointer(x, y, extend);
    }
    pub fn keypad(&mut self, token: &str, extend: bool) {
        self.controller.keypad(token, extend);
    }
    pub fn complete(&self) -> AppResult<String> {
        complete(&self.controller)
    }
    pub fn error(&self) -> String {
        self.controller
            .error
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default()
    }
    pub fn view(&self, density: f32, dark: bool) -> AppResult<MathInputView> {
        let view = self
            .controller
            .view(density.clamp(0.5, 4.0))
            .map_err(|e| e.to_string())?;
        Ok(MathInputView {
            image: theme_image(&view.image, dark),
            content_width: view.width,
            content_height: view.height,
            caret_x: view.caret.x,
            caret_y: view.caret.y,
            caret_height: view.caret.height,
            font_size: view.font_size,
            accessible_text: view.spoken.into(),
        })
    }
}

fn complete(controller: &MathInputController) -> AppResult<String> {
    let latex = controller
        .editor
        .latex()
        .map_err(|_| tr!("请先填写完整的公式。").to_owned())?;
    if latex.trim().is_empty() {
        return Err(tr!("请先填写完整的公式。").into());
    }
    // Knowledge packs deliberately have tighter limits than the general editor.
    r#try::formula::validate(&latex)?;
    Ok(latex)
}

fn theme_image(image: &slint::Image, dark: bool) -> slint::Image {
    if !dark {
        return image.clone();
    }
    let Some(mut pixels) = image.to_rgba8() else {
        return image.clone();
    };
    for pixel in pixels.make_mut_slice() {
        // Preserve structural selection with enough contrast for light ink.
        if i16::from(pixel.b) - i16::from(pixel.r) >= 48 {
            pixel.r = 53;
            pixel.g = 87;
            pixel.b = 123;
        } else {
            pixel.r = 232;
            pixel.g = 232;
            pixel.b = 232;
        }
    }
    slint::Image::from_rgba8(pixels)
}

pub fn categories() -> ModelRc<slint::SharedString> {
    ModelRc::new(VecModel::from(
        category_names()
            .iter()
            .map(|name| r#try::i18n::text(name).into())
            .collect::<Vec<_>>(),
    ))
}

fn category_names() -> &'static [&'static str] {
    &[
        "基本结构",
        "常用函数与数论",
        "微积分",
        "三角函数",
        "双曲与反双曲函数",
        "统计与列表",
        "概率与分布",
        "矩阵与分段",
        "线性代数函数",
        "希腊字母",
        "关系与集合",
        "箭头与运算",
        "常数与字母集",
        "重音与注记",
        "数学字母样式",
    ]
}

pub fn catalog(index: i32) -> ModelRc<MathKey> {
    let groups = category_names();
    let index = index.clamp(0, groups.len() as i32 - 1);
    let name = groups[index as usize];
    ModelRc::new(VecModel::from(
        slint_math_input::catalog::entries()
            .iter()
            .filter(|entry| entry.category == name)
            .map(|entry| MathKey {
                label: entry.label.clone().into(),
                token: format!("catalog:{}", entry.id).into(),
                hint: entry.description.clone().into(),
                image: catalog_image(entry),
            })
            .collect::<Vec<_>>(),
    ))
}

fn catalog_image(entry: &slint_math_input::catalog::CatalogEntry) -> slint::Image {
    // Descriptive captions need the host's native font fallback. STIX Two Math
    // covers notation, so rasterizing Chinese here would bake .notdef boxes
    // into the image before Slint has a chance to choose a suitable font.
    if entry
        .label
        .chars()
        .any(|c| matches!(c, '\u{2e80}'..='\u{9fff}' | '\u{f900}'..='\u{faff}'))
    {
        return slint::Image::default();
    }
    let mut editor = slint_math_input::Editor::new();
    editor.font_size = 17.0;
    // These Unicode caption shortcuts are not all present in the math font.
    // Draw the equivalent structure using ordinary base characters instead.
    let latex = match entry.id.as_str() {
        "power" => Some(r"a^{b}"),
        "subscript" => Some(r"a_{n}"),
        "indexed-root" => Some(r"\sqrt[n]{a}"),
        "log-base" => Some(r"\log_{a}"),
        "integral" => Some(r"\int_{a}^{b}"),
        "dot" => Some(r"\dot{a}"),
        _ => None,
    };
    let inserted = match latex {
        Some(latex) => editor.import_latex(latex),
        None => editor.apply(slint_math_input::Action::Insert(entry.label.clone())),
    };
    let rendered = inserted.and_then(|()| editor.render(2.0));
    rendered
        .map(|rendered| {
            slint::Image::from_rgba8_premultiplied(
                SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                    &rendered.pixels,
                    rendered.pixel_width,
                    rendered.pixel_height,
                ),
            )
        })
        .unwrap_or_default()
}

pub fn plain(latex: &str) -> String {
    math_editor::from_latex(latex)
        .map(|node| node.plain())
        .unwrap_or_else(|_| latex.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_rejects_incomplete_or_oversized_formulas_and_keeps_draft() {
        let mut controller = MathInputController::new();
        controller.insert("1/");
        let draft = controller.editor.draft().unwrap();
        assert!(complete(&controller).is_err());
        assert_eq!(controller.editor.draft().unwrap(), draft);
        controller.insert("2");
        assert_eq!(complete(&controller).unwrap(), r"\frac{1}{2}");
        controller.keypad("key:clear", false);
        controller.insert(&"x".repeat(1001));
        assert!(complete(&controller).is_err());
    }

    #[test]
    fn host_catalog_covers_the_pinned_modules_groups() {
        let entries = slint_math_input::catalog::entries();
        assert!(entries.len() >= 319);
        for entry in entries {
            assert!(
                category_names().contains(&entry.category.as_str()),
                "{}",
                entry.category
            );
        }
    }

    #[test]
    fn host_catalog_keeps_descriptive_labels_on_the_native_text_surface() {
        use slint::Model;
        let mut checked = 0;
        for group in 0..category_names().len() {
            let model = catalog(group as i32);
            for key in model.iter() {
                if key
                    .label
                    .chars()
                    .any(|c| matches!(c, '\u{2e80}'..='\u{9fff}' | '\u{f900}'..='\u{faff}'))
                {
                    assert_eq!(
                        key.image.size().width,
                        0,
                        "{}: {} must use native text",
                        key.token,
                        key.label
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked >= 30);
    }

    #[test]
    fn host_catalog_renders_unicode_scripts_and_accents_as_math_structure() {
        use slint::Model;
        for (group, id, latex) in [
            (0, "power", r"a^{b}"),
            (0, "subscript", r"a_{n}"),
            (0, "indexed-root", r"\sqrt[n]{a}"),
            (2, "log-base", r"\log_{a}"),
            (2, "integral", r"\int_{a}^{b}"),
            (13, "dot", r"\dot{a}"),
        ] {
            let model = catalog(group);
            let key = model
                .iter()
                .find(|key| key.token == format!("catalog:{id}"))
                .unwrap();
            let mut expected = slint_math_input::Editor::new();
            expected.font_size = 17.0;
            expected.import_latex(latex).unwrap();
            let rendered = expected.render(2.0).unwrap();
            let expected = slint::Image::from_rgba8_premultiplied(
                SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                    &rendered.pixels,
                    rendered.pixel_width,
                    rendered.pixel_height,
                ),
            )
            .to_rgba8()
            .unwrap();
            let actual = key.image.to_rgba8().unwrap();
            assert_eq!(
                (actual.width(), actual.height()),
                (expected.width(), expected.height()),
                "{id}"
            );
            assert_eq!(actual.as_bytes(), expected.as_bytes(), "{id}");
        }
    }
}
