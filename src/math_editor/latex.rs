//! Import serialized LaTeX without collapsing unsupported structures.
use super::{Kind, Node, node::is_primes};
use latex_rust::{AccentKind, Delimiter, EnvRow, IntegralKind, MathNode, MatrixStyle, TextStyle};

fn delimiter(d: Delimiter) -> String {
    match d {
        Delimiter::Empty => ".".into(),
        Delimiter::Char(c) => c.to_string(),
        Delimiter::Named(s) => format!("\\{s}"),
    }
}

pub fn from_latex(tex: &str) -> Result<Node, String> {
    fn convert(n: MathNode) -> Option<Node> {
        let branch = Node::branch;
        Some(match n {
            MathNode::Atom('′', _) => Node::leaf(Kind::Symbol, "prime"),
            MathNode::Atom(c, _) => Node::leaf(Kind::Literal, c.to_string()),
            MathNode::Symbol(s) => Node::leaf(Kind::Symbol, s),
            MathNode::Operator(s, _) => Node::leaf(Kind::Operator, s),
            MathNode::Text(s, TextStyle::Text) => Node::leaf(Kind::Text, s),
            MathNode::Text(s, style) => Node::leaf(
                Kind::Styled(
                    match style {
                        TextStyle::Rm => "mathrm",
                        TextStyle::Bf => "mathbf",
                        TextStyle::It => "mathit",
                        TextStyle::Sf => "mathsf",
                        TextStyle::Tt => "mathtt",
                        TextStyle::Bb => "mathbb",
                        TextStyle::Cal => "mathcal",
                        TextStyle::Frak => "mathfrak",
                        TextStyle::Scr => "mathscr",
                        TextStyle::Boldsymbol => "boldsymbol",
                        TextStyle::Pmb => "pmb",
                        TextStyle::Text => unreachable!(),
                    }
                    .into(),
                ),
                s,
            ),
            MathNode::Row(row) => {
                let mut nodes: Vec<Node> = vec![];
                for n in row {
                    let n = convert(n)?;
                    if n.kind == Kind::Literal
                        && n.children.is_empty()
                        && !n.value.contains('\\')
                        && n.value.chars().all(|c| c.is_alphanumeric() || c == '.')
                        && let Some(last) = nodes.last_mut()
                        && last.kind == Kind::Literal
                        && last.value.chars().all(|c| c.is_alphanumeric() || c == '.')
                    {
                        last.value.push_str(&n.value);
                    } else {
                        nodes.push(n);
                    }
                }
                if nodes.is_empty() {
                    Node::blank()
                } else if nodes.len() == 1 {
                    nodes.pop()?
                } else {
                    branch(Kind::Row, nodes)
                }
            }
            MathNode::Fraction(a, b) => branch(Kind::Fraction, vec![convert(*a)?, convert(*b)?]),
            MathNode::Radical(degree, body) => branch(
                Kind::Root,
                degree
                    .into_iter()
                    .map(|n| convert(*n))
                    .chain([convert(*body)])
                    .collect::<Option<Vec<_>>>()?,
            ),
            MathNode::Accent(body, accent) => {
                let mut node = branch(Kind::Accent, vec![convert(*body)?]);
                node.value = match accent {
                    AccentKind::Vec => "vec",
                    AccentKind::Dot => "dot",
                    AccentKind::Ddot => "ddot",
                    AccentKind::Bar | AccentKind::Overline => "bar",
                    AccentKind::Hat | AccentKind::WideHat => "hat",
                    _ => return None,
                }
                .into();
                node
            }
            MathNode::Superscript(a, b) => {
                let base = convert(*a)?;
                let exponent = convert(*b)?;
                // A leading apostrophe is parsed as a prime on an empty row,
                // including inside SMI's u^{'} export. That row is an implicit
                // anchor for the mark, rather than an unfilled formula slot.
                if base == Node::blank() && is_primes(&exponent) {
                    exponent
                } else {
                    branch(Kind::Power, vec![base, exponent])
                }
            }
            MathNode::Subscript(a, b) => branch(Kind::Subscript, vec![convert(*a)?, convert(*b)?]),
            MathNode::SubSup(a, b, c) => {
                branch(Kind::SubSup, vec![convert(*a)?, convert(*b)?, convert(*c)?])
            }
            MathNode::Delimited(left, body, right) => branch(
                Kind::Group(delimiter(left), delimiter(right)),
                vec![convert(*body)?],
            ),
            MathNode::Sum(a, b) => {
                let mut node = branch(
                    Kind::BigOperator,
                    vec![
                        a.map(|n| convert(*n))
                            .unwrap_or(Some(Node::leaf(Kind::Space, "")))?,
                        b.map(|n| convert(*n))
                            .unwrap_or(Some(Node::leaf(Kind::Space, "")))?,
                    ],
                );
                node.value = "sum".into();
                node
            }
            MathNode::Integral(kind, a, b) => {
                let mut node = branch(
                    Kind::BigOperator,
                    vec![
                        a.map(|n| convert(*n))
                            .unwrap_or(Some(Node::leaf(Kind::Space, "")))?,
                        b.map(|n| convert(*n))
                            .unwrap_or(Some(Node::leaf(Kind::Space, "")))?,
                    ],
                );
                node.value = match kind {
                    IntegralKind::Int => "int",
                    IntegralKind::Iint => "iint",
                    IntegralKind::Iiint => "iiint",
                    IntegralKind::Oint => "oint",
                    IntegralKind::Oiint => "oiint",
                }
                .into();
                node
            }
            MathNode::Limit(a) => branch(
                Kind::Limit,
                vec![
                    a.map(|n| convert(*n))
                        .unwrap_or(Some(Node::leaf(Kind::Space, "")))?,
                ],
            ),
            MathNode::Matrix(style, _, rows) => {
                let style = match style {
                    MatrixStyle::Matrix => "matrix",
                    MatrixStyle::Pmatrix => "pmatrix",
                    MatrixStyle::Bmatrix => "bmatrix",
                    MatrixStyle::Vmatrix => "vmatrix",
                    MatrixStyle::Cases => "cases",
                    _ => return None,
                };
                let mut columns = 0;
                let mut children = vec![];
                for row in rows {
                    let EnvRow::Cells { cells, .. } = row else {
                        return None;
                    };
                    if columns == 0 {
                        columns = cells.len();
                    }
                    if cells.len() != columns || columns == 0 {
                        return None;
                    }
                    children.extend(cells.into_iter().map(convert).collect::<Option<Vec<_>>>()?);
                }
                branch(
                    Kind::Grid {
                        style: style.into(),
                        columns,
                    },
                    children,
                )
            }
            MathNode::Space(_) => Node::leaf(Kind::Space, r"\,"),
            _ => return None,
        })
    }
    let canonical = crate::math_structure::canonical(tex);
    let compatible = crate::math_latex::compatible_latex(canonical.as_deref().unwrap_or(tex));
    let ast = latex_rust::parse(&compatible).map_err(|e| e.to_string())?;
    let node = convert(ast).unwrap_or_else(|| Node::leaf(Kind::Raw, compatible.as_ref()));
    // Some display-parser nodes erase distinctions (notably binomial versus
    // fraction). Keep a complete original whenever conversion is not lossless.
    if node.kind != Kind::Raw
        && node.complete()
        && canonical.as_ref().is_some_and(|tex| !tex.trim().is_empty())
        && !crate::math_structure::same(&compatible, &node.tex())
    {
        return Ok(Node::leaf(Kind::Raw, compatible.as_ref()));
    }
    Ok(node)
}

/// A reimportable answer spelling, using LaTeX whenever friendly text would
/// lose structure. This is also used when accepting the completed editor.
pub fn answer_input(latex: &str) -> String {
    if latex.trim().is_empty() {
        return String::new();
    }
    from_latex(latex)
        // Quoted text spans retain their role when answer recognition imports
        // this spelling; unsupported friendly forms remain original LaTeX.
        .map(|node| {
            if node.is_text() {
                node.tex()
            } else {
                node.editor_input()
            }
        })
        .unwrap_or_else(|_| latex.to_owned())
}
