use crate::math_editor::{self, Node};
use crate::tr;
use latex_rust::{Color, Dim, MathFont, SvgOptions};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const MAX_FORMULA_CHARS: usize = 1000;
const MAX_BITMAP_PIXELS: u64 = 1024 * 512;
type PaintBounds = std::collections::HashMap<(u8, u8, u8), (f32, f32, f32, f32)>;

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

pub fn validate(tex: &str) -> Result<(), String> {
    if tex.trim().is_empty() || tex.chars().count() > MAX_FORMULA_CHARS {
        return Err(tr!("公式须为 1 至 1000 个字符。").into());
    }
    latex_rust::parse(tex)
        .map(|_| ())
        .map_err(|error| tr!("公式暂时无法识别：{error}", error = error))
}

/// Typeset only the formula being shown. The window owns the returned pixels;
/// no font instance, SVG, worker, or accumulating cache survives this call.
pub fn render(
    tex: &str,
    font_size_pt: u16,
    scale_factor: f32,
    ink: (u8, u8, u8),
) -> Result<RenderedFormula, String> {
    validate(tex)?;
    let node = math_editor::from_latex(tex)?;
    if node.kind != math_editor::Kind::Raw {
        return render_node(&node, font_size_pt, scale_factor, ink);
    }
    let font = MathFont::stix_two_math().map_err(|error| error.to_string())?;
    let svg = latex_rust::latex_to_svg(
        tex,
        &font,
        &SvgOptions {
            font_size_pt: Dim::from_i64(font_size_pt.into()),
            color: Color::rgb(ink.0, ink.1, ink.2),
            display: true,
        },
    )
    .map_err(|error| error.to_string())?;
    raster_svg(&svg, scale_factor)
}

pub fn render_node(
    node: &Node,
    font_size_pt: u16,
    scale_factor: f32,
    ink: (u8, u8, u8),
) -> Result<RenderedFormula, String> {
    node.validate()?;
    let (marked, leaves) = node.marked_tex();
    let font = MathFont::stix_two_math().map_err(|e| e.to_string())?;
    let mut svg = latex_rust::latex_to_svg(
        &marked,
        &font,
        &SvgOptions {
            font_size_pt: Dim::from_i64(font_size_pt.into()),
            color: Color::rgb(ink.0, ink.1, ink.2),
            display: true,
        },
    )
    .map_err(|e| e.to_string())?;
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let mut bounds = PaintBounds::new();
    fn collect(group: &resvg::usvg::Group, bounds: &mut PaintBounds) {
        for node in group.children() {
            match node {
                resvg::usvg::Node::Group(group) => collect(group, bounds),
                resvg::usvg::Node::Path(path) => {
                    if let Some(fill) = path.fill()
                        && let resvg::usvg::Paint::Color(c) = fill.paint()
                    {
                        let rect = path.abs_bounding_box();
                        let key = (c.red, c.green, c.blue);
                        let new = (rect.left(), rect.top(), rect.right(), rect.bottom());
                        bounds
                            .entry(key)
                            .and_modify(|old| {
                                old.0 = old.0.min(new.0);
                                old.1 = old.1.min(new.1);
                                old.2 = old.2.max(new.2);
                                old.3 = old.3.max(new.3);
                            })
                            .or_insert(new);
                    }
                }
                _ => {}
            }
        }
    }
    collect(tree.root(), &mut bounds);
    let mut texts = vec![];
    let mut regions = vec![];
    let ink_hex = Color::rgb(ink.0, ink.1, ink.2).css_hex();
    for leaf in leaves {
        let hex = Color::rgb(leaf.paint.0, leaf.paint.1, leaf.paint.2).css_hex();
        if let Some((left, top, right, bottom)) = bounds.get(&leaf.paint).copied() {
            regions.push(HitRegion {
                path: leaf.path,
                x: left,
                y: top,
                width: (right - left).max(3.0),
                height: (bottom - top).max(8.0),
            });
            if let Some(text) = &leaf.text {
                // M's ink height is 0.7 em; native text keeps the same baseline
                // and scales with the surrounding fraction/script layout.
                let font_size = (bottom - top) / 0.7;
                texts.push(NativeText {
                    text: text.clone(),
                    x: left,
                    y: top - font_size * 0.15,
                    width: (right - left).max(font_size),
                    height: font_size * 1.2,
                    font_size,
                });
            }
        }
        svg = svg.replace(
            &hex,
            if leaf.text.is_some() {
                "none"
            } else {
                &ink_hex
            },
        );
    }
    let mut rendered = raster_svg(&svg, scale_factor)?;
    rendered.texts = texts;
    rendered.regions = regions;
    Ok(rendered)
}

fn raster_svg(svg: &str, scale_factor: f32) -> Result<RenderedFormula, String> {
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
        .map_err(|error| error.to_string())?;
    let size = tree.size();
    let scale = if scale_factor.is_finite() {
        scale_factor.clamp(1.0, 3.0)
    } else {
        1.0
    };
    let width = (size.width() * scale).ceil() as u32;
    let height = (size.height() * scale).ceil() as u32;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_BITMAP_PIXELS {
        return Err(tr!("这条公式的显示范围过大，请拆成更短的题目。").into());
    }
    let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let mut pixmap =
        resvg::tiny_skia::PixmapMut::from_bytes(pixels.make_mut_bytes(), width, height)
            .ok_or(tr!("无法建立公式图片。"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap,
    );
    Ok(RenderedFormula {
        image: Image::from_rgba8_premultiplied(pixels),
        width: size.width(),
        height: size.height(),
        texts: vec![],
        regions: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
