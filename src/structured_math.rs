//! Try's host adapter for slint-math-input. The editor has no native window.
use crate::MathInputView;
use slint_math_input::gui::MathInputController;
use r#try::{knowledge::AppResult, math_editor, tr};

mod clipboard;
mod keypad;
pub use keypad::{catalog, categories, common_keys};

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
        controller.editor.structured_typing = true;
        match seed {
            Seed::Latex(latex) if !latex.trim().is_empty() => controller
                .editor
                .import_latex(&latex)
                .map_err(|e| e.to_string())?,
            Seed::Typing(text) if !text.is_empty() => {
                if clipboard::expression_latex(&text).is_some() {
                    insert_text(&mut controller, &text);
                } else {
                    // Reproduce the native prefix as typing so the command's
                    // literal form and earlier characters remain undoable.
                    for ch in text.chars() {
                        controller.insert(&ch.to_string());
                    }
                }
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
            can_undo: self.controller.editor.can_undo(),
            can_redo: self.controller.editor.can_redo(),
            has_content: !view.latex.is_empty(),
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
    let latex = r#try::formula::compatible_latex(&latex).into_owned();
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

pub fn plain(latex: &str) -> String {
    math_editor::from_latex(latex)
        .map(|node| node.plain())
        .unwrap_or_else(|_| latex.to_owned())
}

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;

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
    fn direct_typing_enters_log_base_before_it_is_filled() {
        use slint::platform::Key;
        for hint in [false, true] {
            let mut text = String::new();
            let mut session: Option<Session> = None;
            for ch in "log_".chars() {
                if let Some(session) = &mut session {
                    session.insert(&ch.to_string());
                } else {
                    text.push(ch);
                    if r#try::math_answer::starts_structured_edit(&text, hint) {
                        session = Some(Session::new(Seed::Typing(text.clone())).unwrap());
                    }
                }
            }
            let mut session = session.expect("log_ opens its base before the next character");
            assert!(session.complete().is_err(), "the base is still empty");
            let base = session.controller.editor.cursor().row;
            for ch in "n+1".chars() {
                session.insert(&ch.to_string());
            }
            assert_eq!(session.controller.editor.cursor().row, base);
            session.key(&char::from(Key::Tab).to_string(), false, false, false);
            for ch in "(x)".chars() {
                session.insert(&ch.to_string());
            }
            assert_eq!(session.complete().unwrap(), r"\log_{n+1}\left(x\right)");
        }
    }

    #[test]
    fn typed_commands_create_fields_and_preserve_written_function_calls() {
        use slint::platform::Key;
        let mut session = Session::new(Seed::Typing("log".into())).unwrap();
        assert!(session.complete().is_err());
        session.insert("2");
        session.key(&char::from(Key::Tab).to_string(), false, false, false);
        session.insert("x");
        assert_eq!(session.complete().unwrap(), r"\log_{2}\left(x\right)");
        for input in ["sqrt(3)/2", "log_2(x)", "log(x)", "sqrt(sqrt(3))/2"] {
            let mut session = Session::new(Seed::Typing(String::new())).unwrap();
            for ch in input.chars() {
                session.insert(&ch.to_string());
            }
            let latex = session.complete().unwrap();
            assert!(
                r#try::math_answer::matches(input, "", Some(&latex), &[]),
                "{input}: {latex}"
            );
        }
        let mut session = Session::new(Seed::Typing("log".into())).unwrap();
        session.key("z", true, false, false);
        assert_eq!(session.source(), "log");
        session.key("z", true, false, false);
        assert_eq!(session.source(), "lo");
    }

    #[test]
    fn practice_pointer_preserves_formula_boundaries() {
        use crate::{MathInputState, PracticeWindow};
        use slint::platform::{Key, WindowEvent};
        use slint::{ComponentHandle, PhysicalSize};
        use test_support::{click, key, paint, software_window};

        fn refresh(window: &PracticeWindow, session: &Session) {
            window.global::<MathInputState>().set_formula(
                session
                    .view(window.get_pixel_scale(), window.get_dark_theme())
                    .unwrap(),
            );
        }

        let adapter = software_window();
        let window = PracticeWindow::new().unwrap();
        let session = Rc::new(RefCell::new(
            Session::with_clipboard(Seed::Typing(String::new()), TestClipboard(Rc::default()))
                .unwrap(),
        ));
        let ui = window.global::<MathInputState>();
        {
            let state = session.clone();
            let weak = window.as_weak();
            ui.on_seek(move |x, y, extend| {
                let mut state = state.borrow_mut();
                state.pointer(x, y, extend);
                refresh(&weak.unwrap(), &state);
            });
        }
        {
            let state = session.clone();
            let weak = window.as_weak();
            ui.on_committed(move |text| {
                let mut state = state.borrow_mut();
                state.insert(&text);
                refresh(&weak.unwrap(), &state);
            });
        }
        {
            let state = session.clone();
            let weak = window.as_weak();
            ui.on_key(move |text, control, shift, alt| {
                let mut state = state.borrow_mut();
                let handled = state.key(&text, control, shift, alt);
                refresh(&weak.unwrap(), &state);
                handled
            });
        }
        {
            let state = session.clone();
            let weak = window.as_weak();
            ui.on_keypad(move |token, extend| {
                state.borrow_mut().keypad(&token, extend);
                refresh(&weak.unwrap(), &state.borrow());
            });
        }
        ui.set_practice(true);
        ui.set_common_keys(common_keys());
        ui.set_open(true);
        window.set_prompt("Formula".into());
        window.show().unwrap();
        let type_text = |text: &str| {
            for ch in text.chars() {
                let text: slint::SharedString = ch.to_string().into();
                window
                    .window()
                    .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
                window
                    .window()
                    .dispatch_event(WindowEvent::KeyReleased { text });
                paint(&adapter);
            }
        };

        for (density, dark, expanded) in [(1.0, false, false), (2.0, true, true)] {
            window
                .window()
                .dispatch_event(WindowEvent::ScaleFactorChanged {
                    scale_factor: density,
                });
            adapter.set_size(PhysicalSize::new(
                (320.0 * density) as u32,
                (560.0 * density) as u32,
            ));
            window.set_dark_theme(dark);
            ui.set_keypad_expanded(expanded);
            for (source, at_end, dx, expected) in [
                (
                    r"\log_{b}\left(x\right)",
                    false,
                    0.6,
                    r"y\log_{b}\left(x\right)",
                ),
                ("x^{2}", false, 0.6, "yx^{2}"),
                (r"\mathbf{x}_{n}", false, 0.6, r"y\mathbf{x}_{n}"),
                (r"\sqrt[n]{x}", false, -0.6, r"y\sqrt[n]{x}"),
                (r"\sqrt[n]{x}", true, 0.6, r"\sqrt[n]{x}y"),
            ] {
                *session.borrow_mut() = Session::with_clipboard(
                    Seed::Latex(source.into()),
                    TestClipboard(Rc::default()),
                )
                .unwrap();
                refresh(&window, &session.borrow());
                window.invoke_focus_answer();
                key(&window, &adapter, Key::End);
                let caret = {
                    let state = session.borrow();
                    let editor = &state.controller.editor;
                    let mut target = editor.cursor();
                    if !at_end {
                        target.offset = 0;
                    }
                    editor.layout().caret(target).unwrap().rect
                };
                paint(&adapter);
                click(
                    &window,
                    &adapter,
                    window.get_input_canvas_left() + caret.x + dx,
                    window.get_input_canvas_top() + caret.y + caret.height * 0.25,
                );
                type_text("y");
                paint(&adapter);
                assert_eq!(
                    session.borrow().complete().unwrap(),
                    expected,
                    "{source}, density={density}, expanded={expanded}"
                );
                assert!(session.borrow().error().is_empty());
            }
            for (base, expected) in [
                ("b", r"\log_{b}\left(x\right)"),
                ("b^2", r"\log_{b}^{2}\left(x\right)"),
            ] {
                for navigation in [Key::RightArrow, Key::Tab] {
                    *session.borrow_mut() = Session::with_clipboard(
                        Seed::Typing(String::new()),
                        TestClipboard(Rc::default()),
                    )
                    .unwrap();
                    if expanded {
                        refresh(&window, &session.borrow());
                        paint(&adapter);
                        click(
                            &window,
                            &adapter,
                            ui.get_keypad_left() + ui.get_keypad_width() * 3.5 / 4.0,
                            ui.get_keypad_top() + 18.0,
                        );
                    } else {
                        session.borrow_mut().keypad("catalog:log-base", false);
                    }
                    refresh(&window, &session.borrow());
                    window.invoke_focus_answer();
                    type_text(base);
                    let base_cursor = session.borrow().controller.editor.cursor();
                    key(&window, &adapter, navigation);
                    type_text("x");
                    assert_eq!(session.borrow().complete().unwrap(), expected);

                    let caret = {
                        let state = session.borrow();
                        let editor = &state.controller.editor;
                        editor.layout().caret(editor.cursor()).unwrap().rect
                    };
                    session
                        .borrow_mut()
                        .controller
                        .editor
                        .set_position(base_cursor, false)
                        .unwrap();
                    refresh(&window, &session.borrow());
                    paint(&adapter);
                    click(
                        &window,
                        &adapter,
                        window.get_input_canvas_left() + caret.x,
                        window.get_input_canvas_top() + caret.y + caret.height * 0.5,
                    );
                    type_text("y");
                    assert_eq!(
                        session.borrow().complete().unwrap(),
                        expected.replace("(x", "(xy"),
                        "{base}, navigation={navigation:?}, density={density}, expanded={expanded}"
                    );
                    assert!(session.borrow().error().is_empty());
                }
            }
        }
        // Physical Shift selection and Shift-click remain available without
        // onscreen modifier or navigation buttons.
        *session.borrow_mut() =
            Session::with_clipboard(Seed::Latex("x+y".into()), TestClipboard(Rc::default()))
                .unwrap();
        refresh(&window, &session.borrow());
        ui.set_keypad_expanded(true);
        window.invoke_focus_answer();
        key(&window, &adapter, Key::End);
        window.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
        key(&window, &adapter, Key::LeftArrow);
        window.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
        assert!(session.borrow().controller.editor.has_selection());
        type_text("z");
        assert_eq!(session.borrow().complete().unwrap(), "x+z");
        key(&window, &adapter, Key::End);
        let caret = {
            let state = session.borrow();
            let editor = &state.controller.editor;
            let mut before_last = editor.cursor();
            before_last.offset -= 1;
            editor.layout().caret(before_last).unwrap().rect
        };
        window.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
        click(
            &window,
            &adapter,
            window.get_input_canvas_left() + caret.x + 0.6,
            window.get_input_canvas_top() + caret.y + caret.height * 0.5,
        );
        window.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
        assert!(
            session.borrow().controller.editor.has_selection(),
            "Shift-click in the formula extends selection"
        );
        ui.set_keypad_expanded(false);
        paint(&adapter);
        assert!(
            session.borrow().controller.editor.has_selection(),
            "collapsing keeps the selected formula range"
        );
        window.hide().unwrap();
    }

    #[test]
    fn native_practice_typing_hands_over_to_the_same_collapsed_or_expanded_editor() {
        use crate::{MathInputState, PracticeWindow};
        use slint::{
            ComponentHandle, PhysicalSize,
            platform::{Key, WindowEvent},
        };
        use test_support::{key, paint, software_window};
        let adapter = software_window();
        let window = PracticeWindow::new().unwrap();
        let active: Rc<RefCell<Option<Session>>> = Rc::default();
        let ui = window.global::<MathInputState>();
        let refresh = |window: &PracticeWindow, session: &Session| {
            window
                .global::<MathInputState>()
                .set_formula(session.view(1.0, true).unwrap());
        };
        {
            let weak = window.as_weak();
            let active = active.clone();
            window.on_answer_edited(move |text| {
                let window = weak.unwrap();
                window.set_answer_text(text.clone());
                if r#try::math_answer::starts_structured_edit(&text, true) {
                    let session = Session::with_clipboard(
                        Seed::Typing(text.into()),
                        TestClipboard(Rc::default()),
                    )
                    .unwrap();
                    refresh(&window, &session);
                    *active.borrow_mut() = Some(session);
                    let ui = window.global::<MathInputState>();
                    ui.set_open(true);
                    ui.set_focus_request(ui.get_focus_request() + 1);
                }
            });
        }
        {
            let weak = window.as_weak();
            let active = active.clone();
            ui.on_committed(move |text| {
                let mut active = active.borrow_mut();
                let session = active.as_mut().unwrap();
                session.insert(&text);
                refresh(&weak.unwrap(), session);
            });
        }
        {
            let weak = window.as_weak();
            let active = active.clone();
            ui.on_key(move |text, control, shift, alt| {
                let mut active = active.borrow_mut();
                let session = active.as_mut().unwrap();
                let handled = session.key(&text, control, shift, alt);
                refresh(&weak.unwrap(), session);
                handled
            });
        }
        ui.set_practice(true);
        ui.set_common_keys(common_keys());
        window.show().unwrap();
        adapter.set_size(PhysicalSize::new(320, 560));
        let type_text = |text: &str| {
            for ch in text.chars() {
                let text: slint::SharedString = ch.to_string().into();
                window
                    .window()
                    .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
                window
                    .window()
                    .dispatch_event(WindowEvent::KeyReleased { text });
                paint(&adapter);
            }
        };
        for expanded in [false, true] {
            for (typed, expected) in [
                ("sqrt(3)/2", r"\frac{\sqrt{3}}{2}"),
                ("P^n+m", "P^{n+m}"),
                ("log_2(x)", r"\log_{2}\left(x\right)"),
            ] {
                window.set_answer_text("".into());
                ui.set_open(false);
                *active.borrow_mut() = None;
                ui.set_keypad_expanded(expanded);
                window.invoke_focus_answer();
                paint(&adapter);
                type_text(typed);
                assert_eq!(
                    active.borrow().as_ref().unwrap().complete().unwrap(),
                    expected,
                    "{typed}, expanded={expanded}"
                );
            }
            window.set_answer_text("".into());
            ui.set_open(false);
            *active.borrow_mut() = None;
            ui.set_keypad_expanded(expanded);
            window.invoke_focus_answer();
            paint(&adapter);
            type_text("log2");
            let cursor = active.borrow().as_ref().unwrap().controller.editor.cursor();
            ui.set_keypad_expanded(!expanded);
            paint(&adapter);
            assert_eq!(
                active.borrow().as_ref().unwrap().controller.editor.cursor(),
                cursor
            );
            key(&window, &adapter, Key::Tab);
            type_text("x");
            assert_eq!(
                active.borrow().as_ref().unwrap().complete().unwrap(),
                r"\log_{2}\left(x\right)"
            );
        }
        window.hide().unwrap();
    }

    #[test]
    fn host_keyboard_and_keypad_operators_stay_in_the_exponent() {
        for keypad in [false, true] {
            let mut session = Session::new(Seed::Typing(String::new())).unwrap();
            for text in ["P", if keypad { "key:power" } else { "^" }, "n"] {
                if keypad {
                    session.keypad(text, false);
                } else {
                    session.insert(text);
                }
            }
            let inside = session.controller.editor.cursor().row;
            if keypad {
                session.keypad("+", false);
                session.keypad("m", false);
            } else {
                session.insert("+");
                session.insert("m");
            }
            assert_eq!(session.controller.editor.cursor().row, inside);
            assert_eq!(session.complete().unwrap(), "P^{n+m}");
            session.keypad("key:next", false);
            session.insert("+");
            session.insert("q");
            assert_eq!(session.complete().unwrap(), "P^{n+m}+q");
        }
    }

    #[test]
    fn view_reports_edit_history_and_clearing_preserves_undo() {
        let mut session = Session::new(Seed::Typing(String::new())).unwrap();
        let view = session.view(1.0, false).unwrap();
        assert!(!view.can_undo && !view.can_redo && !view.has_content);
        session.keypad("x", false);
        assert!(session.view(1.0, false).unwrap().can_undo);
        session.keypad("key:clear", false);
        assert!(!session.view(1.0, false).unwrap().has_content);
        session.keypad("key:undo", false);
        assert_eq!(session.complete().unwrap(), "x");
        assert!(session.view(1.0, false).unwrap().can_redo);
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
}
