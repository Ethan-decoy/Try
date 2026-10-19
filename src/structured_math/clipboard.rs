//! Normalize written expressions at the editor's existing clipboard seam.
use slint_math_input::{Error, gui::ClipboardAccess};

pub(super) fn expression_latex(text: &str) -> Option<String> {
    if text.chars().count() <= 1 {
        return None;
    }
    r#try::math_answer::recognize_inline(text, false).map(|node| node.tex())
}

pub(super) fn normalize(access: impl ClipboardAccess) -> impl ClipboardAccess {
    FormulaClipboard(access)
}

struct FormulaClipboard<C>(C);

impl<C: ClipboardAccess> ClipboardAccess for FormulaClipboard<C> {
    fn read_text(&mut self) -> Result<String, Error> {
        let text = self.0.read_text()?;
        Ok(expression_latex(&text).unwrap_or(text))
    }

    fn write_text(&mut self, text: String) -> Result<(), Error> {
        self.0.write_text(text)
    }
}

#[derive(Default)]
pub(super) struct SystemClipboard(Option<arboard::Clipboard>);

impl SystemClipboard {
    fn handle(&mut self) -> Result<&mut arboard::Clipboard, Error> {
        if self.0.is_none() {
            self.0 = Some(arboard::Clipboard::new().map_err(clipboard_error)?);
        }
        Ok(self.0.as_mut().expect("clipboard initialized"))
    }
}

fn clipboard_error(error: arboard::Error) -> Error {
    Error {
        message: format!("Clipboard: {error}"),
        offset: None,
    }
}

impl ClipboardAccess for SystemClipboard {
    fn read_text(&mut self) -> Result<String, Error> {
        self.handle()?.get_text().map_err(clipboard_error)
    }

    fn write_text(&mut self, text: String) -> Result<(), Error> {
        self.handle()?.set_text(text).map_err(clipboard_error)
    }
}
