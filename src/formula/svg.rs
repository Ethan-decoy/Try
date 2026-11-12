use super::{HitRegion, NativeText, RenderedFormula, compatible_latex, output};
use crate::math_editor::{Leaf, Node};
use crate::tr;
use latex_rust::{Color, Dim, MathFont, SvgOptions};
use slint::{Rgba8Pixel, SharedPixelBuffer};

type PaintBounds = std::collections::HashMap<(u8, u8, u8), (f32, f32, f32, f32)>;

/// Preserve the display parser's full AST when the editable tree and SMI cannot
/// import it. Only text missing from the math font needs a native overlay;
/// supported text keeps its original color/style. The AST is never rebuilt
/// from a human-readable label.
pub(super) fn preserved(
    tex: &str,
    font_size_pt: u16,
    scale_factor: f32,
    ink: (u8, u8, u8),
) -> Result<RenderedFormula, String> {
    use latex_rust::{MathNode, TextStyle};
    let mut ast = latex_rust::parse(tex).map_err(|error| error.to_string())?;
    let font = MathFont::stix_two_math().map_err(|error| error.to_string())?;
    let mut used = std::collections::HashSet::from([ink]);
    visit(&mut ast, &mut |node| {
        let mut remember = |color: &Color| {
            used.insert((color.r, color.g, color.b));
        };
        match node {
            MathNode::Color(color, _)
            | MathNode::TextColor(color, _)
            | MathNode::ColorBox(color, _) => remember(color),
            MathNode::FColorBox(border, fill, _) => {
                remember(border);
                remember(fill);
            }
            _ => {}
        }
    });
    let mut leaves = vec![];
    let mut next = 0;
    visit(&mut ast, &mut |node| {
        if let MathNode::Text(text, TextStyle::Text) = node {
            if text
                .chars()
                .all(|ch| ch.is_whitespace() || font.face().glyph_index(ch).is_some())
            {
                return;
            }
            while used.contains(&Leaf::paint(next)) {
                next += 1;
            }
            let paint = Leaf::paint(next);
            next += 1;
            let measured = Leaf::text_placeholder(text);
            leaves.push(Leaf {
                path: vec![],
                paint,
                text: Some(text.clone()),
            });
            *node = native_line_box(MathNode::Color(
                Color::rgb(paint.0, paint.1, paint.2),
                Box::new(MathNode::Text(measured, TextStyle::Text)),
            ));
        }
    });
    let svg = typeset(&ast, &font, font_size_pt, ink)?;
    // Raw content is edited as one preserved formula, rather than invented paths
    // into a structure the content tree does not understand.
    let mut rendered = marked_svg(svg, leaves, scale_factor, ink)?;
    rendered.regions = vec![HitRegion {
        path: vec![],
        x: 0.0,
        y: 0.0,
        width: rendered.width,
        height: rendered.height,
    }];
    Ok(rendered)
}

/// Visit children before their parent so a render-only replacement is not
/// recursively visited again. Keep this exhaustive over the pinned parser AST.
fn visit(node: &mut latex_rust::MathNode, apply: &mut impl FnMut(&mut latex_rust::MathNode)) {
    use latex_rust::{EnvRow, EqNumber, MathNode::*};
    match node {
        Fraction(a, b) | Superscript(a, b) | Subscript(a, b) | CancelTo(a, b) => {
            visit(a, apply);
            visit(b, apply);
        }
        SubSup(a, b, c) => {
            visit(a, apply);
            visit(b, apply);
            visit(c, apply);
        }
        Radical(a, b) => {
            if let Some(a) = a {
                visit(a, apply);
            }
            visit(b, apply);
        }
        Delimited(_, body, _)
        | Accent(body, _)
        | Intertext(body)
        | Tag { body, .. }
        | Color(_, body)
        | TextColor(_, body)
        | ColorBox(_, body)
        | FColorBox(_, _, body)
        | Phantom(_, body) => visit(body, apply),
        Row(nodes) | Substack(nodes) => {
            for node in nodes {
                visit(node, apply);
            }
        }
        Matrix(_, _, rows) => {
            for row in rows {
                match row {
                    EnvRow::Cells { cells, number, .. } => {
                        for cell in cells {
                            visit(cell, apply);
                        }
                        if let EqNumber::Tag { body, .. } = number {
                            visit(body, apply);
                        }
                    }
                    EnvRow::Intertext(body) => visit(body, apply),
                    EnvRow::Hline => {}
                }
            }
        }
        Sum(a, b) | Integral(_, a, b) | Product(a, b) => {
            if let Some(a) = a {
                visit(a, apply);
            }
            if let Some(b) = b {
                visit(b, apply);
            }
        }
        Limit(body) => {
            if let Some(body) = body {
                visit(body, apply);
            }
        }
        OverUnder(base, over, under) => {
            visit(base, apply);
            if let Some(over) = over {
                visit(over, apply);
            }
            if let Some(under) = under {
                visit(under, apply);
            }
        }
        Atom(..) | SizedDelim(..) | Ref(_) | Label(_) | NoNumber | Hline | Text(..) | Space(_)
        | Operator(..) | Symbol(_) | Strut(..) => {}
    }
    apply(node);
}

pub(super) fn node(
    node: &Node,
    font_size_pt: u16,
    scale_factor: f32,
    ink: (u8, u8, u8),
) -> Result<RenderedFormula, String> {
    let (marked, leaves) = node.marked_tex();
    let mut ast =
        latex_rust::parse(&compatible_latex(&marked)).map_err(|error| error.to_string())?;
    let native_paints = leaves
        .iter()
        .filter(|leaf| {
            leaf.text
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty())
        })
        .map(|leaf| leaf.paint)
        .collect::<std::collections::HashSet<_>>();
    if !native_paints.is_empty() {
        visit(&mut ast, &mut |node| {
            if let latex_rust::MathNode::Color(paint, _) = node
                && native_paints.contains(&(paint.r, paint.g, paint.b))
            {
                let body = std::mem::replace(node, latex_rust::MathNode::Row(vec![]));
                *node = native_line_box(body);
            }
        });
    }
    let font = MathFont::stix_two_math().map_err(|e| e.to_string())?;
    let svg = typeset(&ast, &font, font_size_pt, ink)?;
    marked_svg(svg, leaves, scale_factor, ink)
}

fn native_line_box(body: latex_rust::MathNode) -> latex_rust::MathNode {
    // The native overlay has a full line box, rather than M's cap height.
    // Reserve its ascent/descent before surrounding decorations are laid out.
    latex_rust::MathNode::Row(vec![
        latex_rust::MathNode::Strut(Dim::ratio(85, 100), Dim::ratio(35, 100)),
        body,
    ])
}

fn typeset(
    ast: &latex_rust::MathNode,
    font: &MathFont,
    font_size_pt: u16,
    ink: (u8, u8, u8),
) -> Result<String, String> {
    let options = SvgOptions {
        font_size_pt: Dim::from_i64(font_size_pt.into()),
        color: Color::rgb(ink.0, ink.1, ink.2),
        display: true,
    };
    let layout = latex_rust::layout_with_em_size_pt(
        ast,
        font,
        latex_rust::MathStyle::Display,
        &options.font_size_pt,
    )
    .map_err(|error| error.to_string())?;
    latex_rust::render_svg(&layout, font, &options).map_err(|error| error.to_string())
}

fn marked_svg(
    mut svg: String,
    leaves: Vec<Leaf>,
    scale_factor: f32,
    ink: (u8, u8, u8),
) -> Result<RenderedFormula, String> {
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
    raster_svg(&svg, scale_factor, texts, regions)
}

fn raster_svg(
    svg: &str,
    scale_factor: f32,
    texts: Vec<NativeText>,
    regions: Vec<HitRegion>,
) -> Result<RenderedFormula, String> {
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
        .map_err(|error| error.to_string())?;
    let size = tree.size();
    let scale = output::scale(scale_factor);
    let width = (size.width() * scale).ceil() as u32;
    let height = (size.height() * scale).ceil() as u32;
    output::check_size(width, height)?;
    let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let mut pixmap =
        resvg::tiny_skia::PixmapMut::from_bytes(pixels.make_mut_bytes(), width, height)
            .ok_or(tr!("无法建立公式图片。"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap,
    );
    output::finish(
        pixels,
        scale,
        (size.width(), size.height()),
        texts,
        regions,
        false,
    )
}
