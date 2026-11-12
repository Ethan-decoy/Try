//! Content tree, serialization and lossless written projections.
use super::{MAX_DEPTH, MAX_INPUT, MAX_NODES, expression};
use crate::tr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Literal,
    Symbol,
    Operator,
    Styled(String),
    Accent,
    Row,
    Fraction,
    Root,
    Power,
    Subscript,
    SubSup,
    Group(String, String),
    BigOperator,
    Limit,
    Grid { style: String, columns: usize },
    Space,
    Raw,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub kind: Kind,
    pub value: String,
    pub children: Vec<Node>,
}

impl Node {
    pub fn leaf(kind: Kind, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
            children: vec![],
        }
    }

    pub(super) fn branch(kind: Kind, children: Vec<Self>) -> Self {
        Self {
            kind,
            value: String::new(),
            children,
        }
    }

    pub fn blank() -> Self {
        Self::leaf(Kind::Literal, "")
    }

    pub fn is_text(&self) -> bool {
        self.kind == Kind::Text
    }
    pub fn accepts_prose(&self) -> bool {
        self.is_text() || (self.kind == Kind::Row && self.children.iter().any(Self::is_text))
    }

    pub fn at(&self, path: &[usize]) -> Option<&Self> {
        match path.split_first() {
            None => Some(self),
            Some((index, rest)) => self.children.get(*index)?.at(rest),
        }
    }

    pub(super) fn at_mut(&mut self, path: &[usize]) -> Option<&mut Self> {
        match path.split_first() {
            None => Some(self),
            Some((index, rest)) => self.children.get_mut(*index)?.at_mut(rest),
        }
    }

    pub fn tex(&self) -> String {
        self.serialize(None, &mut 0, &mut vec![])
    }

    /// Each editable leaf gets a distinct paint. The renderer derives hit
    /// regions from the actual glyph bounds, rather than an approximate grid.
    pub fn marked_tex(&self) -> (String, Vec<Leaf>) {
        let mut leaves = vec![];
        let tex = self.serialize(Some(&mut leaves), &mut 0, &mut vec![]);
        (tex, leaves)
    }

    fn serialize(
        &self,
        mut marks: Option<&mut Vec<Leaf>>,
        next: &mut usize,
        path: &mut Vec<usize>,
    ) -> String {
        if self.children.is_empty() {
            let raw = match &self.kind {
                Kind::Text => format!("\\text{{{}}}", escape_text(&self.value)),
                Kind::Symbol => format!("\\{} ", self.value),
                Kind::Operator => {
                    if latex_rust::parse(&format!("\\{}", self.value)).is_ok() {
                        format!("\\{} ", self.value)
                    } else {
                        format!("\\operatorname{{{}}}", self.value)
                    }
                }
                Kind::Styled(style) => format!("\\{style}{{{}}}", self.value),
                Kind::Space => self.value.clone(),
                Kind::Raw => self.value.clone(),
                _ if self.value.is_empty() => "\\square".into(),
                _ => self.value.clone(),
            };
            if let Some(leaves) = marks.as_mut() {
                let index = *next;
                *next += 1;
                let paint = Leaf::paint(index);
                // Native UI text supplies CJK glyphs. An ASCII measuring run
                // reserves space in the math layout, then becomes transparent.
                let measured = if self.is_text() {
                    format!("\\text{{{}}}", Leaf::text_placeholder(&self.value))
                } else {
                    raw
                };
                leaves.push(Leaf {
                    path: path.clone(),
                    paint,
                    text: self.is_text().then(|| self.value.clone()),
                });
                return format!(
                    "{{\\color[RGB]{{{},{},{}}}{{{measured}}}}}",
                    paint.0, paint.1, paint.2
                );
            }
            return raw;
        }
        let mut children = Vec::with_capacity(self.children.len());
        for (i, child) in self.children.iter().enumerate() {
            path.push(i);
            children.push(child.serialize(marks.as_deref_mut(), next, path));
            path.pop();
        }
        match &self.kind {
            Kind::Row => children.join(""),
            Kind::Fraction => format!("\\frac{{{}}}{{{}}}", children[0], children[1]),
            Kind::Root if children.len() == 2 => {
                format!("\\sqrt[{}]{{{}}}", children[0], children[1])
            }
            Kind::Root => format!("\\sqrt{{{}}}", children[0]),
            Kind::Accent => format!("\\{}{{{}}}", self.value, children[0]),
            Kind::Power => format!("{{{}}}^{{{}}}", children[0], children[1]),
            Kind::Subscript => format!("{{{}}}_{{{}}}", children[0], children[1]),
            Kind::SubSup => format!(
                "{{{}}}_{{{}}}^{{{}}}",
                children[0], children[1], children[2]
            ),
            Kind::Group(left, right) if left.starts_with('\\') || right.starts_with('\\') => {
                format!("\\left{left} {}\\right{right} ", children[0])
            }
            Kind::Group(left, right) => format!("\\left{left}{}\\right{right}", children[0]),
            Kind::BigOperator => {
                format!("\\{}_{{{}}}^{{{}}}", self.value, children[0], children[1])
            }
            Kind::Limit => format!("\\lim_{{{}}}", children[0]),
            Kind::Grid { style, columns } => format!(
                "\\begin{{{style}}}{}\\end{{{style}}}",
                children
                    .chunks(*columns)
                    .map(|row| row.join(" & "))
                    .collect::<Vec<_>>()
                    .join(r" \\ ")
            ),
            _ => children.join(""),
        }
    }

    /// Human-readable content label. Text spans retain their authored spacing.
    /// Use `editor_input` when importing a formula back into the expression parser.
    pub fn plain(&self) -> String {
        self.lossless_written(false)
    }
    /// Reimportable spelling: quote literal text and retain LaTeX whenever a
    /// friendly expression cannot preserve the editor-authoritative structure.
    pub fn editor_input(&self) -> String {
        self.lossless_written(true)
    }
    fn lossless_written(&self, quoted_text: bool) -> String {
        let written = self.written(quoted_text);
        if self.accepts_prose() || !self.complete() || self.kind == Kind::Raw {
            return written;
        }
        // Friendly text is useful only when reimporting it retains the formula.
        // Otherwise keep LaTeX, including styles, bounds and postfix scope.
        if expression(&written)
            .is_ok_and(|node| crate::math_structure::same(&self.tex(), &node.tex()))
        {
            written
        } else {
            self.tex()
        }
    }
    fn written(&self, quoted_text: bool) -> String {
        let c = self
            .children
            .iter()
            .map(|node| node.written(quoted_text))
            .collect::<Vec<_>>();
        match &self.kind {
            Kind::Literal if quoted_text && self.value.is_empty() => "□".into(),
            Kind::Text if quoted_text => format!(
                "\"{}\"",
                self.value.replace('\\', "\\\\").replace('"', "\\\"")
            ),
            Kind::Symbol => latex_rust::lookup(&format!("\\{}", self.value))
                .map(|s| s.glyph.to_string())
                .unwrap_or_else(|| self.value.clone()),
            Kind::Space => if self.value.is_empty() { "" } else { " " }.into(),
            Kind::Row => {
                let mut result = String::new();
                let mut index = 0;
                while index < self.children.len() {
                    let op = &self.children[index];
                    // Keep neighboring math words separate: a^u followed by
                    // ln u must not round-trip as a^(ulnu). Text spans retain
                    // their authored spacing and concatenation.
                    if index > 0
                        && !op.is_text()
                        && !self.children[index - 1].is_text()
                        && result
                            .chars()
                            .next_back()
                            .zip(c[index].chars().next())
                            .is_some_and(|(left, right)| {
                                left.is_ascii_alphabetic() && right.is_ascii_alphabetic()
                                    || (left.is_ascii_digit() || left == '.')
                                        && (right.is_ascii_digit() || right == '.')
                            })
                    {
                        result.push(' ');
                    }
                    if matches!(op.kind, Kind::BigOperator | Kind::Limit)
                        && let Some(body) = self.children.get(index + 1)
                        && !(body.kind == Kind::Literal
                            && ["+", "-", "=", "<", ">", ",", ";"].contains(&body.value.as_str()))
                    {
                        let args = op
                            .children
                            .iter()
                            .map(|node| node.written(quoted_text))
                            .collect::<Vec<_>>();
                        let name = if op.kind == Kind::Limit {
                            "lim"
                        } else {
                            &op.value
                        };
                        if op.kind == Kind::BigOperator && args.iter().all(String::is_empty) {
                            result.push_str(&format!("{name}({})", body.written(quoted_text)));
                        } else {
                            result.push_str(&format!(
                                "{name}({},{})",
                                args.join(","),
                                body.written(quoted_text)
                            ));
                        }
                        index += 2;
                    } else {
                        result.push_str(&c[index]);
                        index += 1;
                    }
                }
                result
            }
            Kind::Fraction => format!(
                "{}/{}",
                grouped(&self.children[0], quoted_text),
                grouped(&self.children[1], quoted_text)
            ),
            Kind::Root if c.len() == 2 => format!("root({},{})", c[0], c[1]),
            Kind::Root => format!("sqrt({})", c[0]),
            Kind::Accent => format!("{}({})", self.value, c[0]),
            Kind::Power => format!(
                "{}^{}",
                grouped(&self.children[0], quoted_text),
                grouped(&self.children[1], quoted_text)
            ),
            Kind::Subscript => format!(
                "{}_{}",
                grouped(&self.children[0], quoted_text),
                grouped(&self.children[1], quoted_text)
            ),
            Kind::SubSup => format!(
                "{}_{}^{}",
                grouped(&self.children[0], quoted_text),
                grouped(&self.children[1], quoted_text),
                grouped(&self.children[2], quoted_text)
            ),
            Kind::Group(left, right) => {
                if left.ends_with("Vert") && right.ends_with("Vert") {
                    format!("norm({})", c[0])
                } else if left == "|" && right == "|" {
                    format!("abs({})", c[0])
                } else {
                    format!("{}{}{}", friendly_delim(left), c[0], friendly_delim(right))
                }
            }
            Kind::Styled(style) if style == "mathbb" => match self.value.as_str() {
                "R" => "ℝ",
                "Z" => "ℤ",
                "N" => "ℕ",
                "Q" => "ℚ",
                "C" => "ℂ",
                _ => &self.value,
            }
            .into(),
            Kind::BigOperator => format!("{}({},{})", self.value, c[0], c[1]),
            Kind::Limit => format!("lim({})", c[0]),
            Kind::Grid { style, columns } => {
                if style == "cases" {
                    format!("cases({})", c.join(","))
                } else {
                    format!(
                        "[{}]",
                        c.chunks(*columns)
                            .map(|r| format!("[{}]", r.join(",")))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                }
            }
            _ => self.value.clone(),
        }
    }

    pub fn complete(&self) -> bool {
        if self.kind == Kind::Raw {
            return crate::math_structure::complete(&self.value);
        }
        !(self.kind == Kind::Literal && self.value.is_empty())
            && self.children.iter().all(Self::complete)
    }

    pub fn validate(&self) -> Result<(), String> {
        fn visit(node: &Node, depth: usize, count: &mut usize) -> bool {
            *count += 1;
            depth <= MAX_DEPTH
                && *count <= MAX_NODES
                && node.children.iter().all(|n| visit(n, depth + 1, count))
        }
        if !visit(self, 0, &mut 0) || self.tex().chars().count() > MAX_INPUT {
            return Err(tr!("内容过长或结构过深，请拆成更短的题目。").into());
        }
        crate::math_latex::validate(&self.tex())
    }
}

fn grouped(node: &Node, quoted_text: bool) -> String {
    let s = node.written(quoted_text);
    if (node.kind == Kind::Row && node.children.len() > 1)
        || matches!(
            node.kind,
            Kind::Fraction | Kind::Power | Kind::Subscript | Kind::SubSup
        )
    {
        format!("({s})")
    } else {
        s
    }
}
fn friendly_delim(s: &str) -> &str {
    match s {
        r"\{" => "{",
        r"\}" => "}",
        "." => "",
        _ => s,
    }
}
fn escape_text(s: &str) -> String {
    s.replace('\\', r"\\")
        .replace('{', r"\{")
        .replace('}', r"\}")
        .replace('%', r"\%")
        .replace('&', r"\&")
        .replace('#', r"\#")
        .replace('_', r"\_")
        .replace('$', r"\$")
}
#[derive(Clone, Debug)]
pub struct Leaf {
    pub path: Vec<usize>,
    pub paint: (u8, u8, u8),
    pub text: Option<String>,
}

impl Leaf {
    pub(crate) fn paint(index: usize) -> (u8, u8, u8) {
        (13, ((index >> 8) & 255) as u8, (index & 255) as u8)
    }

    /// ASCII ink reserves native text in the vector layout. Its actual bounds
    /// supply both hit testing and the native font's baseline; the ink is removed.
    pub(crate) fn text_placeholder(text: &str) -> String {
        if !text.is_empty() && text.trim().is_empty() {
            return text.into();
        }
        let units = text
            .chars()
            .map(|c| if c.is_ascii() { 0.55 } else { 1.0 })
            .sum::<f32>();
        "M".repeat((units / 0.9).ceil().max(1.0) as usize)
    }
}

pub(super) fn is_primes(node: &Node) -> bool {
    node.kind == Kind::Symbol && node.value == "prime"
        || node.kind == Kind::Row && node.children.iter().all(is_primes)
}
