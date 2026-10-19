//! Try's host adapter for slint-math-input. The editor has no native window.
use crate::{MathInputView, MathKey};
use slint::{ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use slint_math_input::gui::MathInputController;
use r#try::{knowledge::AppResult, math_editor, tr};

mod clipboard;

pub struct Session {
    controller: MathInputController,
}

pub enum Seed {
    Latex(String),
    Typing(String),
}

impl Session {
    pub fn new(seed: Seed) -> AppResult<Self> {
        Self::with_clipboard(seed, clipboard::SystemClipboard::default())
    }

    fn with_clipboard(
        seed: Seed,
        access: impl slint_math_input::gui::ClipboardAccess + 'static,
    ) -> AppResult<Self> {
        let mut controller = MathInputController::with_clipboard(clipboard::normalize(access));
        controller.editor.font_size = 22.0;
        match seed {
            Seed::Latex(latex) if !latex.trim().is_empty() => controller
                .editor
                .import_latex(&latex)
                .map_err(|e| e.to_string())?,
            Seed::Typing(text) if !text.is_empty() => {
                insert_text(&mut controller, &text);
                if let Some(error) = &controller.error {
                    return Err(error.to_string());
                }
            }
            _ => {}
        }
        Ok(Self { controller })
    }
    pub fn insert(&mut self, text: &str) {
        insert_text(&mut self.controller, text);
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
    pub fn source(&self) -> String {
        plain(&self.controller.editor.latex_preview())
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

fn insert_text(controller: &mut MathInputController, text: &str) {
    // A whole expression arrives from native editing or an IME commit. Parse
    // its written grouping before inserting: replaying sqrt(3)/2 as shortcut
    // keystrokes would create parentheses inside the root and divide there.
    // Single keystrokes and unfinished structures still use the editor's live
    // structural shortcuts. Paste records one undoable edit at the selection.
    if let Some(latex) = clipboard::expression_latex(text) {
        controller.error = controller.editor.paste(&latex).err();
    } else {
        controller.insert(text);
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
    use slint_math_input::gui::ClipboardAccess;
    use std::{cell::RefCell, rc::Rc};

    struct TestClipboard(Rc<RefCell<String>>);

    impl ClipboardAccess for TestClipboard {
        fn read_text(&mut self) -> Result<String, slint_math_input::Error> {
            Ok(self.0.borrow().clone())
        }
        fn write_text(&mut self, text: String) -> Result<(), slint_math_input::Error> {
            *self.0.borrow_mut() = text;
            Ok(())
        }
    }

    #[test]
    fn host_clipboard_paste_resets_vertical_navigation_column() {
        let clipboard = Rc::new(RefCell::new(r"\frac{1}{a}".to_owned()));
        let mut session =
            Session::with_clipboard(Seed::Typing("1/2".into()), TestClipboard(clipboard)).unwrap();
        let up = char::from(slint::platform::Key::UpArrow).to_string();
        assert!(session.key(&up, false, false, false));
        assert!(session.key("v", true, false, false));
        assert!(session.error().is_empty());
        assert!(session.key(&up, false, false, false));
        assert_eq!(session.controller.editor.cursor().offset, 1);
    }

    #[test]
    fn host_home_and_end_reset_vertical_navigation_column() {
        use slint::platform::Key;
        for end in [false, true] {
            let mut session = Session::new(Seed::Typing("12/34".into())).unwrap();
            if end {
                assert!(session.key(&char::from(Key::Home).to_string(), false, false, false));
            }
            for key in [
                Key::UpArrow,
                if end { Key::End } else { Key::Home },
                Key::DownArrow,
            ] {
                assert!(session.key(&char::from(key).to_string(), false, false, false));
            }
            assert_eq!(
                session.controller.editor.cursor().offset,
                if end { 2 } else { 0 }
            );
        }
    }

    #[test]
    fn clipboard_and_committed_expressions_share_grouping_and_undo() {
        let content = Rc::new(RefCell::new("sqrt(3)/2".to_owned()));
        let mut session =
            Session::with_clipboard(Seed::Typing("x+1".into()), TestClipboard(content.clone()))
                .unwrap();
        session.key("a", true, false, false);
        assert!(session.key("v", true, false, false));
        assert_eq!(session.complete().unwrap(), r"\frac{\sqrt{3}}{2}");
        session.key("z", true, false, false);
        assert_eq!(session.source(), "x+1");
        session.key("y", true, false, false);
        assert_eq!(session.source(), "sqrt(3)/2");
        session.key("a", true, false, false);
        session.key("x", true, false, false);
        assert_eq!(content.borrow().as_str(), r"\frac{\sqrt{3}}{2}");
        assert!(session.complete().is_err());
        session.key("v", true, false, false);
        assert_eq!(session.source(), "sqrt(3)/2");
        *content.borrow_mut() = r"\frac{".into();
        session.key("a", true, false, false);
        session.key("v", true, false, false);
        assert!(!session.error().is_empty());
        assert_eq!(
            session.source(),
            "sqrt(3)/2",
            "a failed paste preserves the formula"
        );
    }

    #[test]
    fn whole_expressions_keep_grouping_when_seeded_and_replaced() {
        let mut session = Session::new(Seed::Typing("sqrt(3)/2".into())).unwrap();
        assert_eq!(session.complete().unwrap(), r"\frac{\sqrt{3}}{2}");
        assert_eq!(session.source(), "sqrt(3)/2");
        session.key("a", true, false, false);
        session.insert("sqrt(5)/4");
        assert_eq!(session.complete().unwrap(), r"\frac{\sqrt{5}}{4}");
        session.key("z", true, false, false);
        assert_eq!(session.complete().unwrap(), r"\frac{\sqrt{3}}{2}");
    }

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
