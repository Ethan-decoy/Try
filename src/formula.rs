//! Formula display. Every backend returns the same logical geometry, native
//! text overlays and hit regions. Rendering never changes serialized content.
use crate::math_editor::{self, Node};
use slint::Image;

// Preserve the existing entry points while keeping parsing independent of display.
pub use crate::math_latex::{compatible_latex, validate};

mod output;
mod structured;
mod svg;

/// Complete display projection. All geometry is in logical pixels; `image`
/// contains premultiplied physical pixels at the requested (bounded) scale.
/// Native text and hit regions lie inside `width`/`height` and share its origin.
pub struct RenderedFormula {
    pub image: Image,
    pub width: f32,
    pub height: f32,
    pub texts: Vec<NativeText>,
    pub regions: Vec<HitRegion>,
}

#[derive(Clone, Debug)]
pub struct NativeText {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
}

#[derive(Clone, Debug)]
pub struct HitRegion {
    pub path: Vec<usize>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Typeset only the formula being shown. The caller owns the returned pixels;
/// no font instance, SVG, worker or accumulating cache survives this call.
/// Valid syntax can still return a display error; the UI then uses its label.
pub fn render(
    tex: &str,
    font_size_pt: u16,
    scale_factor: f32,
    ink: (u8, u8, u8),
) -> Result<RenderedFormula, String> {
    let compatible = compatible_latex(tex);
    let tex = compatible.as_ref();
    validate(tex)?;
    let node = math_editor::from_latex(tex)?;
    if node.kind != math_editor::Kind::Raw {
        return render_node(&node, font_size_pt, scale_factor, ink);
    }
    // Retain structures the content tree cannot represent faithfully (binomials,
    // for example). Only use the display parser when SMI cannot import them.
    let mut editor = slint_math_input::Editor::new();
    if editor.import_latex(tex).is_ok() {
        return structured::render(editor, font_size_pt, scale_factor, ink);
    }
    svg::preserved(tex, font_size_pt, scale_factor, ink)
}

pub fn render_node(
    node: &Node,
    font_size_pt: u16,
    scale_factor: f32,
    ink: (u8, u8, u8),
) -> Result<RenderedFormula, String> {
    node.validate()?;
    fn preserved(node: &Node) -> bool {
        node.kind == math_editor::Kind::Raw || node.children.iter().any(preserved)
    }
    if preserved(node) {
        return render(&node.tex(), font_size_pt, scale_factor, ink);
    }
    svg::node(node, font_size_pt, scale_factor, ink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxed_native_text_keeps_its_line_box_inside_the_decoration() {
        for scale in [1.0, 1.5, 2.0] {
            let rendered = render(r"\boxed{\text{（中文说明）}}", 18, scale, (30, 30, 30)).unwrap();
            let pixels = rendered.image.to_rgba8().unwrap();
            let rows: Vec<_> = pixels
                .as_slice()
                .chunks_exact(pixels.width() as usize)
                .enumerate()
                .filter(|(_, row)| row.iter().any(|pixel| pixel.a > 0))
                .map(|(index, _)| index)
                .collect();
            let top = *rows.first().unwrap() as f32 / scale;
            let bottom = (*rows.last().unwrap() + 1) as f32 / scale;
            let text = &rendered.texts[0];
            assert!(text.y >= top, "native line starts above the box: {text:?}");
            assert!(
                text.y + text.height <= bottom,
                "native line extends below the box: {text:?}"
            );
        }
    }

    #[test]
    fn preserved_text_keeps_existing_font_colors_and_phantom_visibility() {
        let colored = render(r"\color{red}{\text{label}}", 18, 1.0, (30, 30, 30)).unwrap();
        assert!(
            colored.texts.is_empty(),
            "supported styled text stays in its original AST"
        );
        let pixels = colored.image.to_rgba8().unwrap();
        assert!(pixels.as_slice().iter().any(|p| p.r > p.g && p.a > 0));

        let phantom = render(r"\phantom{\text{中文}}", 18, 1.0, (30, 30, 30)).unwrap();
        assert!(
            phantom.texts.is_empty(),
            "a hidden placeholder must not become visible text"
        );
        assert!(
            phantom
                .image
                .to_rgba8()
                .unwrap()
                .as_slice()
                .iter()
                .all(|p| p.a == 0)
        );
    }

    #[test]
    fn rendering_paths_share_native_text_bounds_and_hit_regions() {
        for tex in [
            r"x\text{（中文说明）}",
            r"x\quad\text{（中文说明）}",
            r"\binom{4}{2}\text{（中文说明）}",
            r"\boxed{\text{（中文说明）}}",
            r"\color{red}{\text{（中文说明）}}",
        ] {
            let node = math_editor::from_latex(tex).unwrap();
            for scale in [1.0, 1.5, 2.0] {
                for rendered in [
                    render(tex, 18, scale, (30, 30, 30)).unwrap(),
                    render_node(&node, 18, scale, (232, 232, 232)).unwrap(),
                ] {
                    assert!(!rendered.regions.is_empty(), "hit regions: {tex}");
                    assert_eq!(rendered.texts.len(), 1, "native caption: {tex}");
                    for text in &rendered.texts {
                        assert_eq!(text.text, "（中文说明）");
                        assert!(text.x >= 0.0 && text.y >= 0.0, "{tex}: {text:?}");
                        assert!(
                            text.x + text.width <= rendered.width + 0.001,
                            "{tex}: {text:?}"
                        );
                        assert!(
                            text.y + text.height <= rendered.height + 0.001,
                            "{tex}: {text:?}"
                        );
                    }
                    for region in rendered.regions {
                        assert!(node.at(&region.path).is_some(), "editable path: {tex}");
                        assert!(region.x >= 0.0 && region.y >= 0.0);
                        assert!(region.x + region.width <= rendered.width + 0.001);
                        assert!(region.y + region.height <= rendered.height + 0.001);
                    }
                }
            }
        }
    }

    #[test]
    fn mixed_text_keeps_native_font_fallback_in_preserved_formulas() {
        for tex in [
            r"x\quad\text{（中文说明）}",
            r"\binom{4}{2}\text{（中文说明）}",
        ] {
            for (scale, ink) in [(1.0, (30, 30, 30)), (2.0, (232, 232, 232))] {
                let rendered = render(tex, 18, scale, ink).unwrap();
                let text = rendered
                    .texts
                    .iter()
                    .find(|run| run.text == "（中文说明）")
                    .unwrap_or_else(|| panic!("native text missing: {tex}"));
                assert!(text.x >= 0.0 && text.y >= 0.0);
                assert!(text.x + text.width <= rendered.width);
                assert!(text.y + text.height <= rendered.height);
                let pixels = rendered.image.to_rgba8().unwrap();
                // The math font supplies no text glyphs here. Chinese is
                // painted by FormulaLabel's native text layer instead.
                let left = (text.x * scale).ceil() as u32;
                let right = ((text.x + text.width) * scale).floor() as u32;
                let top = (text.y * scale).ceil() as u32;
                let bottom = ((text.y + text.height) * scale).floor() as u32;
                assert!(
                    (top..bottom).all(|y| (left..right)
                        .all(|x| pixels.as_slice()[(y * pixels.width() + x) as usize].a == 0)),
                    "text must not be baked as missing-glyph boxes: {tex}"
                );
            }
        }
    }

    #[test]
    fn mixed_text_in_nested_fields_keeps_its_own_size_and_bounds() {
        let rendered = render(
            r"\binom{\text{上标}}{2}\quad\text{说明}",
            22,
            2.0,
            (30, 30, 30),
        )
        .unwrap();
        let upper = rendered
            .texts
            .iter()
            .find(|run| run.text == "上标")
            .unwrap();
        let outer = rendered
            .texts
            .iter()
            .find(|run| run.text == "说明")
            .unwrap();
        assert!(upper.font_size < outer.font_size);
        assert!(upper.x + upper.width < outer.x);
        for text in &rendered.texts {
            assert!(text.x >= 0.0 && text.y >= 0.0);
            assert!(text.x + text.width <= rendered.width);
            assert!(text.y + text.height <= rendered.height);
        }
    }

    #[test]
    fn mixed_text_only_formula_can_use_a_transparent_math_bitmap() {
        let rendered = render(r"\quad\text{中文}", 18, 1.0, (30, 30, 30)).unwrap();
        assert_eq!(rendered.texts.len(), 1);
        assert_eq!(rendered.texts[0].text, "中文");
        assert!(rendered.width > 0.0 && rendered.height > 0.0);
        assert!(
            rendered
                .image
                .to_rgba8()
                .unwrap()
                .as_slice()
                .iter()
                .all(|pixel| pixel.a == 0)
        );
    }

    #[test]
    fn bridge_renders_a_binomial_differently_from_a_fraction() {
        let choose = render(r"\binom{5}{2}", 22, 1.0, (30, 30, 30)).unwrap();
        let fraction = render(r"\left(\frac{5}{2}\right)", 22, 1.0, (30, 30, 30)).unwrap();
        assert!(
            choose.width != fraction.width
                || choose.height != fraction.height
                || choose.image.to_rgba8().unwrap().as_slice()
                    != fraction.image.to_rgba8().unwrap().as_slice(),
            "binomial coefficients must not gain a fraction bar"
        );
    }

    #[test]
    fn bridge_preserved_structures_render_the_same_inside_authoring_fields() {
        let mut field = math_editor::Field::new("", r"\frac{x}{2}");
        field.select(vec![0]);
        field.replace_formula(r"\binom{5}{2}").unwrap();
        let (_, latex) = field.content().unwrap();
        for (scale, ink) in [(1.0, (30, 30, 30)), (2.0, (232, 232, 232))] {
            let authoring = render_node(&field.root, 22, scale, ink).unwrap();
            let reopened = render(&latex, 22, scale, ink).unwrap();
            assert_eq!(
                authoring.image.to_rgba8().unwrap().as_slice(),
                reopened.image.to_rgba8().unwrap().as_slice()
            );
        }
    }

    #[test]
    fn primed_formulas_render_like_explicit_derivative_marks() {
        for (shorthand, explicit) in [
            (
                r"u^{'}f^{'}\left(u\right)",
                r"u^{\prime}f^{\prime}\left(u\right)",
            ),
            ("f^{''}(x)", r"f^{\prime\prime}(x)"),
            ("f^{'''}(x)", r"f^{\prime\prime\prime}(x)"),
        ] {
            for (scale, ink) in [(1.0, (30, 30, 30)), (2.0, (232, 232, 232))] {
                let actual = render(shorthand, 22, scale, ink).unwrap();
                let expected = render(explicit, 22, scale, ink).unwrap();
                assert_eq!(actual.width, expected.width, "{shorthand}");
                assert_eq!(actual.height, expected.height, "{shorthand}");
                assert_eq!(
                    actual.image.to_rgba8().unwrap().as_slice(),
                    expected.image.to_rgba8().unwrap().as_slice(),
                    "derivative display must not add an empty base: {shorthand}"
                );
            }
        }
    }

    #[test]
    fn component_notation_can_be_saved_and_rendered_without_changing_its_meaning() {
        for (source, expected) in [
            (r"30\degree", "30°"),
            (r"A\supsetneq B", "A⊋ B"),
            (r"A\uplus B", "A⊎ B"),
            (r"a\coloneqq b", "a≔ b"),
            (r"\hom(x)", r"\operatorname{hom}(x)"),
        ] {
            assert_eq!(compatible_latex(source), expected);
            validate(source).unwrap();
            math_editor::from_latex(source).unwrap().validate().unwrap();
            let image = render(source, 18, 1.0, (30, 30, 30)).unwrap();
            assert!(
                image
                    .image
                    .to_rgba8()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .any(|p| p.a > 0),
                "{source}"
            );
        }
    }

    #[test]
    fn common_math_renders_to_visible_pixels_at_display_scales() {
        for tex in [
            r"\sin\left(\frac{\pi}{3}\right)=?",
            r"\frac{\sqrt{3}}{2}",
            r"a^2+b^2=c^2",
            r"\frac{-b\pm\sqrt{b^2-4ac}}{2a}",
            r"\sum_{k=1}^{n}k=\frac{n(n+1)}{2}",
            r"\int_0^1 x^2\,\mathrm{d}x=?",
        ] {
            let original = render(tex, 21, 1.0, (36, 58, 49)).unwrap();
            for scale in [1.0, 1.5, 2.0] {
                let formula = render(tex, 21, scale, (36, 58, 49)).unwrap();
                assert_eq!(
                    (formula.width, formula.height),
                    (original.width, original.height)
                );
                let pixels = formula.image.to_rgba8().unwrap();
                assert_eq!(pixels.width(), (formula.width * scale).ceil() as u32);
                assert_eq!(pixels.height(), (formula.height * scale).ceil() as u32);
                assert!(pixels.as_slice().iter().any(|pixel| pixel.a > 0));
            }
        }
    }

    #[test]
    fn malformed_or_excessive_math_returns_an_error() {
        assert!(validate("").is_err());
        assert!(validate(r"\tryunknown{x}").is_err());
        assert!(validate(r"\frac{1}").is_err());
        assert!(validate(&("{".repeat(40) + "x" + &"}".repeat(40))).is_err());
        assert!(validate(&"x".repeat(1001)).is_err());
        assert!(render(r"\rule{10000pt}{10000pt}", 21, 2.0, (0, 0, 0)).is_err());
    }
}
