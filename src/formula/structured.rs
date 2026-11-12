use super::{NativeText, RenderedFormula, output};
use crate::tr;
use serde_json::Value;
use slint::{Rgba8Pixel, SharedPixelBuffer};
use slint_math_input::Editor;

pub(super) fn render(
    mut editor: slint_math_input::Editor,
    font_size_pt: u16,
    scale_factor: f32,
    ink: (u8, u8, u8),
) -> Result<RenderedFormula, String> {
    let scale = output::scale(scale_factor);
    editor.font_size = f32::from(font_size_pt) * 96.0 / 72.0;
    let (editor, spans) =
        native_text_layout(editor).ok_or_else(|| tr!("无法建立公式图片。").to_owned())?;
    let layout = editor.layout();
    output::check_size(
        (layout.width * scale).ceil() as u32,
        (layout.height * scale).ceil() as u32,
    )?;
    let mut texts = vec![];
    for span in spans {
        let rect = layout
            .range_rects(span.row, span.index, span.index + 1)
            .into_iter()
            .next()
            .ok_or_else(|| tr!("无法建立公式图片。").to_owned())?;
        let caret = layout
            .caret(slint_math_input::Position {
                row: span.row,
                offset: span.index,
            })
            .ok_or_else(|| tr!("无法建立公式图片。").to_owned())?;
        let font_size = caret.rect.height;
        texts.push(NativeText {
            text: span.text,
            x: rect.x,
            y: caret.rect.y - font_size * 0.1,
            width: rect.width,
            height: font_size * 1.2,
            font_size,
        });
    }
    let rendered = editor.render(scale).map_err(|error| error.to_string())?;
    let mut pixels =
        SharedPixelBuffer::<Rgba8Pixel>::new(rendered.pixel_width, rendered.pixel_height);
    for (target, source) in pixels
        .make_mut_bytes()
        .chunks_exact_mut(4)
        .zip(rendered.pixels.chunks_exact(4))
    {
        let alpha = source[3];
        target.copy_from_slice(&[
            (u16::from(ink.0) * u16::from(alpha) / 255) as u8,
            (u16::from(ink.1) * u16::from(alpha) / 255) as u8,
            (u16::from(ink.2) * u16::from(alpha) / 255) as u8,
            alpha,
        ]);
    }
    output::finish(
        pixels,
        scale,
        (layout.width, layout.height),
        texts,
        vec![],
        true,
    )
}

struct NativeTextSpan {
    pub row: u64,
    pub index: usize,
    pub text: String,
}

/// Make a render-only document with space reserved for native text. Spaces
/// have no ink, so decorations and neighboring glyphs never need to be erased.
/// The original formula and editor history are not rewritten.
fn native_text_layout(editor: Editor) -> Option<(Editor, Vec<NativeTextSpan>)> {
    // Math-only documents keep the existing rendering path, including draft
    // structures that do not need to be inspected for native text.
    if !editor.latex_preview().contains(r"\text{") {
        return Some((editor, vec![]));
    }
    fn space_advance() -> Option<f32> {
        let mut measure = Editor::new();
        measure.font_size = 32.0;
        measure.import_latex(r"\text{ }").ok()?;
        let one = measure.layout().width;
        measure.import_latex(r"\text{  }").ok()?;
        let advance = (measure.layout().width - one) / measure.font_size;
        (advance.is_finite() && advance > 0.0).then_some(advance)
    }
    fn visit(
        value: &mut Value,
        spans: &mut Vec<NativeTextSpan>,
        advance: &mut Option<f32>,
    ) -> Option<()> {
        match value {
            Value::Object(object) => {
                if let Some(row) = object.get("id").and_then(Value::as_u64)
                    && let Some(Value::Array(nodes)) = object.get_mut("nodes")
                {
                    for (index, node) in nodes.iter_mut().enumerate() {
                        if node.get("kind").and_then(Value::as_str) == Some("text")
                            && let Some(text) = node.get("text").and_then(Value::as_str)
                            && !text.trim().is_empty()
                        {
                            let text = text.to_owned();
                            let em = text
                                .chars()
                                .map(|ch| {
                                    if ch.is_whitespace() {
                                        0.33
                                    } else if ch.is_ascii()
                                        && !matches!(ch, 'm' | 'w' | 'M' | 'W' | '@' | '%')
                                    {
                                        0.6
                                    } else {
                                        1.0
                                    }
                                })
                                .sum::<f32>();
                            let space = match *advance {
                                Some(space) => space,
                                None => {
                                    let space = space_advance()?;
                                    *advance = Some(space);
                                    space
                                }
                            };
                            node["text"] =
                                Value::String(" ".repeat((em / space).ceil().max(1.0) as usize));
                            spans.push(NativeTextSpan { row, index, text });
                        } else {
                            visit(node, spans, advance)?;
                        }
                    }
                } else {
                    for child in object.values_mut() {
                        visit(child, spans, advance)?;
                    }
                }
            }
            Value::Array(array) => {
                for child in array {
                    visit(child, spans, advance)?;
                }
            }
            _ => {}
        }
        Some(())
    }
    let mut draft = crate::math_structure::editor_draft(&editor)?;
    let mut spans = vec![];
    visit(
        draft.get_mut("state")?.get_mut("root")?,
        &mut spans,
        &mut None,
    )?;
    if spans.is_empty() {
        return Some((editor, spans));
    }
    let mut prepared = Editor::from_draft(&draft.to_string()).ok()?;
    prepared.font_size = editor.font_size;
    Some((prepared, spans))
}
