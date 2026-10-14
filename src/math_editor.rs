//! On-demand content authoring. Ordinary expressions and visual structures
//! share this tree; supported LaTeX pastes are decoded to the same structure.
use crate::math_vocabulary as vocabulary;
use crate::tr;
use latex_rust::{AccentKind, Delimiter, EnvRow, IntegralKind, MathNode, MatrixStyle, TextStyle};

const MAX_INPUT: usize = 1000;
const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 400;

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

    fn branch(kind: Kind, children: Vec<Self>) -> Self {
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

    fn at_mut(&mut self, path: &[usize]) -> Option<&mut Self> {
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
                let paint = marker(index);
                // Native UI text supplies CJK glyphs. An ASCII measuring run
                // reserves space in the math layout, then becomes transparent.
                let measured =
                    if self.is_text() && !self.value.is_empty() && self.value.trim().is_empty() {
                        format!("\\text{{{}}}", self.value)
                    } else if self.is_text() {
                        let units = self
                            .value
                            .chars()
                            .map(|c| if c.is_ascii() { 0.55 } else { 1.0 })
                            .sum::<f32>();
                        format!(
                            "\\text{{{}}}",
                            "M".repeat((units / 0.9).ceil().max(1.0) as usize)
                        )
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

    pub fn plain(&self) -> String {
        self.written(false)
    }
    pub fn editor_input(&self) -> String {
        self.written(true)
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
            Kind::Raw => "".into(),
            _ => self.value.clone(),
        }
    }

    pub fn complete(&self) -> bool {
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
        crate::formula::validate(&self.tex())
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
fn marker(index: usize) -> (u8, u8, u8) {
    (13, ((index >> 8) & 255) as u8, (index & 255) as u8)
}

#[derive(Clone, Debug)]
pub struct Leaf {
    pub path: Vec<usize>,
    pub paint: (u8, u8, u8),
    pub text: Option<String>,
}

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
                    AccentKind::Bar => "bar",
                    AccentKind::Hat => "hat",
                    _ => return None,
                }
                .into();
                node
            }
            MathNode::Superscript(a, b) => branch(Kind::Power, vec![convert(*a)?, convert(*b)?]),
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
    let ast = latex_rust::parse(tex).map_err(|e| e.to_string())?;
    Ok(convert(ast).unwrap_or_else(|| Node::leaf(Kind::Raw, tex)))
}

/// UTF-8 offsets, matching Slint's text input selection. A range and a node
/// selection are separate: moving the text caret never selects a whole formula.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: usize,
    pub focus: usize,
}

#[derive(Clone)]
struct Draft {
    input: String,
    error: Option<String>,
}

#[derive(Clone)]
struct Snapshot {
    root: Node,
    selection: Vec<usize>,
    draft: Option<Draft>,
    caret: TextSelection,
}

#[derive(Clone)]
pub struct Field {
    pub root: Node,
    pub selection: Vec<usize>,
    original: (String, String),
    original_root: Node,
    dirty: bool,
    draft: Option<Draft>,
    caret: TextSelection,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Field {
    pub fn new(text: &str, latex: &str) -> Self {
        let root = if latex.trim().is_empty() {
            Node::leaf(Kind::Text, text)
        } else {
            from_latex(latex).unwrap_or_else(|_| Node::leaf(Kind::Raw, latex))
        };
        let mut field = Self {
            original_root: root.clone(),
            root,
            selection: vec![],
            original: (text.into(), latex.into()),
            dirty: false,
            draft: None,
            caret: TextSelection::default(),
            undo: vec![],
            redo: vec![],
        };
        field.caret_to_end();
        field
    }
    pub fn selected(&self) -> &Node {
        self.root.at(&self.selection).unwrap_or(&self.root)
    }
    pub fn input(&self) -> String {
        if let Some(draft) = &self.draft {
            return draft.input.clone();
        }
        if *self.selected() == Node::blank() {
            return String::new();
        }
        if self.selected().kind == Kind::Raw {
            self.original.0.clone()
        } else if self.selected().accepts_prose() {
            self.selected().plain()
        } else {
            self.selected().editor_input()
        }
    }
    pub fn error(&self) -> Option<&str> {
        self.draft.as_ref().and_then(|draft| draft.error.as_deref())
    }
    pub fn text_selection(&self) -> TextSelection {
        self.caret
    }
    pub fn set_text_selection(&mut self, anchor: usize, focus: usize) {
        let input = self.input();
        let bound = |offset: usize| {
            let mut offset = offset.min(input.len());
            while !input.is_char_boundary(offset) {
                offset -= 1;
            }
            offset
        };
        self.caret = TextSelection {
            anchor: bound(anchor),
            focus: bound(focus),
        };
    }
    fn caret_to_end(&mut self) {
        let end = self.input().len();
        self.caret = TextSelection {
            anchor: end,
            focus: end,
        };
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            root: self.root.clone(),
            selection: self.selection.clone(),
            draft: self.draft.clone(),
            caret: self.caret,
        }
    }
    fn checkpoint(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > 32 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }
    fn replace(&mut self, node: Node) {
        self.checkpoint();
        self.draft = None;
        if let Some(slot) = self.root.at_mut(&self.selection) {
            *slot = node;
        }
    }
    pub fn update(&mut self, input: &str) -> Result<(), String> {
        // Generated text for an incomplete structure need not round-trip through
        // the expression parser just to navigate. Submission checks completeness.
        if input == self.input() {
            return self.error().map_or(Ok(()), |error| Err(error.to_owned()));
        }
        let parsed: Result<Node, String> = (|| {
            let node = if self.selected().accepts_prose() {
                automatic_content(input)?
            } else if input.contains('\\') {
                let node = from_latex(input)?;
                if node.kind == Kind::Raw {
                    return Err(tr!("这个公式暂不支持结构编辑。").into());
                }
                node
            } else {
                expression(input)?
            };
            node.validate()?;
            Ok(node)
        })();
        self.checkpoint();
        self.draft = Some(Draft {
            input: input.into(),
            error: parsed.as_ref().err().cloned(),
        });
        if let Ok(node) = &parsed {
            *self
                .root
                .at_mut(&self.selection)
                .expect("selection is valid") = node.clone();
        }
        self.caret_to_end();
        parsed.map(|_| ())
    }
    pub fn select(&mut self, path: Vec<usize>) {
        if self.root.at(&path).is_some() {
            self.selection = path;
            self.draft = None;
            self.caret_to_end();
            self.caret.anchor = 0;
        }
    }
    pub fn parent(&mut self) {
        if self.selection.pop().is_some() {
            self.draft = None;
            self.caret_to_end();
            self.caret.anchor = 0;
        }
    }
    pub fn undo(&mut self, redo: bool) {
        let current = self.snapshot();
        let stack = if redo { &mut self.redo } else { &mut self.undo };
        if let Some(previous) = stack.pop() {
            self.root = previous.root;
            self.selection = previous.selection;
            self.draft = previous.draft;
            self.caret = previous.caret;
            if redo {
                self.undo.push(current);
            } else {
                self.redo.push(current);
            }
            self.dirty = true;
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn type_text(&mut self, text: &str) -> Result<(), String> {
        let mut input = self.input();
        let start = self.caret.anchor.min(self.caret.focus);
        let end = self.caret.anchor.max(self.caret.focus);
        input.replace_range(start..end, text);
        let result = self.update(&input);
        self.set_text_selection(start + text.len(), start + text.len());
        result
    }
    pub fn backspace(&mut self) -> Result<(), String> {
        let mut input = self.input();
        let mut start = self.caret.anchor.min(self.caret.focus);
        let end = self.caret.anchor.max(self.caret.focus);
        if start == end {
            if self.at_structure_end() {
                self.navigate("previous");
                return Ok(());
            }
            if input.is_empty() && self.error().is_none() && !self.selection.is_empty() {
                let paths = self.editable_paths();
                let empty_parent = (0..self.selection.len()).find(|depth| {
                    let parent = &self.selection[..*depth];
                    paths
                        .iter()
                        .filter(|path| path.starts_with(parent))
                        .all(|path| {
                            let node = self.root.at(path).unwrap();
                            *node == Node::blank() || node.is_text() && node.value.is_empty()
                        })
                });
                if let Some(depth) = empty_parent {
                    let path = self.selection[..depth].to_vec();
                    self.checkpoint();
                    *self.root.at_mut(&path).unwrap() = Node::blank();
                    self.selection = path;
                    self.draft = None;
                    self.caret_to_end();
                } else if !self.navigate("previous") {
                    self.navigate("parent");
                }
                return Ok(());
            }
            let Some((previous, _)) = input[..start].char_indices().next_back() else {
                return Ok(());
            };
            start = previous;
        }
        // Keep the original caret in history, rather than a temporary range
        // created solely to delete the previous character.
        input.replace_range(start..end, "");
        let result = self.update(&input);
        self.set_text_selection(start, start);
        result
    }
    pub fn at_structure_end(&self) -> bool {
        if self.error().is_some()
            || self.selected().accepts_prose()
            || self.caret.anchor != self.caret.focus
            || self.caret.focus != self.input().len()
        {
            return false;
        }
        self.has_trailing_structure()
    }
    pub fn has_trailing_structure(&self) -> bool {
        let selected = self.selected();
        if selected.accepts_prose() || self.error().is_some() {
            return false;
        }
        if selected.kind == Kind::Row {
            selected
                .children
                .last()
                .is_some_and(|node| !node.children.is_empty())
        } else {
            !selected.children.is_empty()
        }
    }
    fn editable_paths(&self) -> Vec<Vec<usize>> {
        fn visit(node: &Node, path: &mut Vec<usize>, result: &mut Vec<Vec<usize>>) {
            if node.children.is_empty() {
                let separator = matches!(node.kind, Kind::Operator | Kind::Space)
                    || node.kind == Kind::Literal
                        && ["+", "-", "=", "<", ">", ",", ";"].contains(&node.value.as_str())
                    || node.kind == Kind::Symbol
                        && ["cdot", "times", "div", "leq", "geq", "neq", "to"]
                            .contains(&node.value.as_str());
                if !separator {
                    result.push(path.clone());
                }
            } else {
                for (index, child) in node.children.iter().enumerate() {
                    path.push(index);
                    visit(child, path, result);
                    path.pop();
                }
            }
        }
        let mut result = vec![];
        visit(&self.root, &mut vec![], &mut result);
        result
    }
    fn navigation_path(&self, command: &str) -> Option<Vec<usize>> {
        if self.error().is_some() {
            return None;
        }
        if command == "parent" || command == "exit" {
            return (!self.selection.is_empty())
                .then(|| self.selection[..self.selection.len() - 1].to_vec());
        }
        let paths = self.editable_paths();
        if command == "previous" || command == "next" {
            if !self.selected().children.is_empty() {
                let mut inside = paths
                    .iter()
                    .filter(|path| path.starts_with(&self.selection));
                return if command == "next" {
                    inside.next()
                } else {
                    inside.next_back()
                }
                .cloned();
            }
            let current = paths.iter().position(|path| *path == self.selection)?;
            return if command == "next" {
                paths.get(current + 1)
            } else {
                current.checked_sub(1).and_then(|index| paths.get(index))
            }
            .cloned();
        }
        if command == "up" || command == "down" {
            for depth in (0..self.selection.len()).rev() {
                let path = &self.selection[..depth];
                let node = self.root.at(path)?;
                let slot = self.selection[depth];
                let target = match node.kind {
                    Kind::Fraction | Kind::Subscript => {
                        if command == "up" {
                            slot.checked_sub(1)
                        } else {
                            Some(slot + 1)
                        }
                    }
                    Kind::Power | Kind::SubSup => {
                        if command == "up" {
                            Some(slot + 1)
                        } else {
                            slot.checked_sub(1)
                        }
                    }
                    Kind::Grid { columns, .. } => {
                        if command == "up" {
                            slot.checked_sub(columns)
                        } else {
                            Some(slot + columns)
                        }
                    }
                    _ => continue,
                }?;
                if target >= node.children.len() {
                    return None;
                }
                let mut target_path = path.to_vec();
                target_path.push(target);
                return paths
                    .iter()
                    .find(|path| path.starts_with(&target_path))
                    .cloned();
            }
        }
        None
    }
    pub fn can_navigate(&self, command: &str) -> bool {
        self.navigation_path(command).is_some()
    }
    /// Navigation changes the selected slot/caret, never the formula or history.
    pub fn navigate(&mut self, command: &str) -> bool {
        let Some(path) = self.navigation_path(command) else {
            return false;
        };
        self.select(path);
        if command == "previous" || command == "exit" {
            self.caret.anchor = self.caret.focus;
        } else if command == "next" {
            self.caret = TextSelection::default();
        }
        true
    }
    /// Templates act on a text range or the operand immediately before the
    /// caret. A temporary marker preserves the insertion focus through parsing.
    pub fn insert_at(&mut self, tool: &str) -> Result<(), String> {
        if ["clear", "move_out", "unwrap"].contains(&tool) {
            self.insert(tool)?;
            if tool != "clear" {
                self.caret_to_end();
            }
            return Ok(());
        }
        if let Some(error) = self.error() {
            return Err(error.to_owned());
        }
        if tool == "math" && self.selected().is_text() {
            let input = self.input();
            let start = self.caret.anchor.min(self.caret.focus);
            let end = self.caret.anchor.max(self.caret.focus);
            let math = if start == end {
                Node::blank()
            } else {
                expression(&input[start..end])?
            };
            let mut children = vec![];
            if start > 0 {
                children.push(Node::leaf(Kind::Text, &input[..start]));
            }
            let focus = children.len();
            children.push(math);
            if end < input.len() {
                children.push(Node::leaf(Kind::Text, &input[end..]));
            }
            if children.len() == 1 {
                self.replace(children.remove(0));
            } else {
                self.replace(Node::branch(Kind::Row, children));
                self.selection.push(focus);
            }
            self.caret_to_end();
            return Ok(());
        }
        if self.selected().accepts_prose() || self.selected().kind == Kind::Raw || tool == "math" {
            self.insert(tool)?;
            self.caret_to_end();
            return Ok(());
        }
        let input = self.input();
        let mut start = self.caret.anchor.min(self.caret.focus);
        let end = self.caret.anchor.max(self.caret.focus);
        let implicit_operand = [
            "fraction",
            "power",
            "subscript",
            "symbol:degree",
            "symbol:prime",
        ]
        .contains(&tool);
        if start == end && implicit_operand {
            start = operand_start(&input[..end]);
        }
        let operand = if start == end {
            Node::blank()
        } else {
            expression(&input[start..end])?
        };
        let mut template = Self::new("", "");
        if tool == "text" {
            template.root = Node::leaf(Kind::Text, "");
        } else {
            template.root = operand;
            template.insert(tool)?;
        }
        // Focus the first genuinely empty slot rather than a non-empty numerator.
        let first_blank = template.editable_paths().into_iter().find(|path| {
            template
                .root
                .at(path)
                .is_some_and(|node| *node == Node::blank())
        });
        if let Some(blank) = &first_blank {
            template.selection = blank.clone();
        }
        let mut marker = "tryfocus".to_owned();
        while input.contains(&marker) {
            marker.push('q');
        }
        // Parse one isolated atom at the insertion site, then substitute the
        // intact template. Adjacent names and fraction suffixes cannot swallow
        // the marker or become part of the new denominator.
        let candidate = format!("{} {} {}", &input[..start], marker, &input[end..]);
        let mut node = expression(&candidate)?;
        fn find(node: &Node, marker: &str, path: &mut Vec<usize>) -> bool {
            if node.children.is_empty() && node.value == marker {
                return true;
            }
            for (index, child) in node.children.iter().enumerate() {
                path.push(index);
                if find(child, marker, path) {
                    return true;
                }
                path.pop();
            }
            false
        }
        let mut focus = vec![];
        if !find(&node, &marker, &mut focus) {
            return Err(tr!("请先选中要编辑的公式位置。").into());
        }
        *node.at_mut(&focus).unwrap() = template.root;
        node.validate()?;
        self.replace(node);
        self.selection.extend(focus);
        if first_blank.is_some() {
            self.selection.extend(template.selection);
        }
        self.caret_to_end();
        Ok(())
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub fn clear(&mut self) {
        let empty = if self.root.is_text() {
            Node::leaf(Kind::Text, "")
        } else {
            Node::blank()
        };
        if self.root != empty
            || self
                .draft
                .as_ref()
                .is_some_and(|draft| !draft.input.is_empty())
        {
            self.checkpoint();
            self.root = empty;
        }
        self.draft = None;
        self.selection.clear();
        self.caret = TextSelection::default();
    }
    fn wrapper_content(node: &Node) -> Option<usize> {
        match &node.kind {
            Kind::Root if (1..=2).contains(&node.children.len()) => Some(node.children.len() - 1),
            Kind::Group(_, _) | Kind::Accent | Kind::Styled(_) if node.children.len() == 1 => {
                Some(0)
            }
            _ => None,
        }
    }
    fn wrapper_target(&self, move_out: bool) -> Option<(Vec<usize>, usize)> {
        (0..=self.selection.len()).rev().find_map(|depth| {
            let path = &self.selection[..depth];
            let wrapper = self.root.at(path)?;
            let slot = Self::wrapper_content(wrapper)?;
            // An indexed root's degree is a separate field, not its content.
            if self
                .selection
                .get(depth)
                .is_some_and(|index| *index != slot)
            {
                return None;
            }
            let content = &wrapper.children[slot];
            if move_out
                && !matches!(
                    content.kind,
                    Kind::Fraction | Kind::Power | Kind::Subscript | Kind::SubSup
                )
            {
                return None;
            }
            Some((path.to_vec(), slot))
        })
    }
    pub fn can_move_out(&self) -> bool {
        self.wrapper_target(true).is_some()
    }
    pub fn can_unwrap(&self) -> bool {
        self.wrapper_target(false).is_some()
    }
    fn restructure(&mut self, move_out: bool) -> Result<(), String> {
        let (path, slot) = self.wrapper_target(move_out).ok_or_else(|| {
            if move_out {
                tr!("这里没有可外移的分式或上下标。")
            } else {
                tr!("这里没有可拆除的根号、括号或标记。")
            }
            .to_owned()
        })?;
        let mut wrapper = self.root.at(&path).unwrap().clone();
        let mut content = std::mem::replace(&mut wrapper.children[slot], Node::blank());
        let suffix = self.selection.get(path.len() + 1..).unwrap_or_default();
        let mut selection = path.clone();
        if move_out {
            wrapper.children[slot] = std::mem::replace(&mut content.children[0], Node::blank());
            content.children[0] = wrapper;
            if let Some((first, rest)) = suffix.split_first() {
                selection.push(*first);
                if *first == 0 {
                    selection.push(slot);
                }
                selection.extend_from_slice(rest);
            }
        } else {
            selection.extend_from_slice(suffix);
        }
        let mut root = self.root.clone();
        *root.at_mut(&path).unwrap() = content;
        root.validate()?;
        debug_assert!(root.at(&selection).is_some());
        self.checkpoint();
        self.root = root;
        self.selection = selection;
        self.draft = None;
        self.caret_to_end();
        Ok(())
    }
    pub fn insert(&mut self, tool: &str) -> Result<(), String> {
        if tool == "clear" {
            self.clear();
            return Ok(());
        }
        if tool == "move_out" || tool == "unwrap" {
            return self.restructure(tool == "move_out");
        }
        let selected = self.selected().clone();
        if tool == "text" || tool == "math" {
            let next = if tool == "text" {
                Node::leaf(Kind::Text, "")
            } else {
                Node::blank()
            };
            if self.root.is_text() && self.root.value.is_empty() {
                self.replace(next);
            } else {
                let value = Node::branch(Kind::Row, vec![selected, next]);
                self.replace(value);
                self.selection.push(1);
            }
            return Ok(());
        }
        let selected = if selected.is_text() {
            if selected.value.trim().is_empty() {
                Node::blank()
            } else {
                expression(&selected.value)?
            }
        } else {
            selected
        };
        let blank = Node::blank;
        if let Some(name) = tool.strip_prefix("function:") {
            let (node, focus) = if name == "log_base" {
                (logarithm(blank(), selected), vec![0, 1])
            } else if let Some(name) = vocabulary::function(name) {
                if !vocabulary::is_operator(name)
                    && !["abs", "norm", "vec", "dot", "ddot", "bar", "hat"].contains(&name)
                {
                    return Err(tr!("请选择可用的函数。").into());
                }
                let focus = if vocabulary::is_operator(name) {
                    vec![1, 0]
                } else {
                    vec![0]
                };
                (applied_function(name, selected), focus)
            } else {
                return Err(tr!("无法识别这个函数。").into());
            };
            node.validate()?;
            self.replace(node);
            self.selection.extend(focus);
            return Ok(());
        }
        if let Some(name) = tool.strip_prefix("symbol:") {
            let symbol = if let Some(letter) = vocabulary::blackboard(name) {
                Node::leaf(Kind::Styled("mathbb".into()), letter)
            } else if name == "degree" || name == "prime" {
                let exponent = Node::leaf(
                    Kind::Symbol,
                    if name == "degree" { "circ" } else { "prime" },
                );
                let node = Node::branch(Kind::Power, vec![selected, exponent]);
                node.validate()?;
                self.replace(node);
                self.selection.push(0);
                return Ok(());
            } else if let Some(name) = vocabulary::symbol(name) {
                Node::leaf(Kind::Symbol, name)
            } else {
                return Err(tr!("无法识别这个符号。").into());
            };
            let relation = [
                "pm", "mp", "times", "div", "leq", "geq", "neq", "approx", "in", "notin",
                "subseteq", "cup", "cap", "to",
            ]
            .contains(&name);
            let (node, focus) = if selected == blank() {
                (symbol, None)
            } else if relation {
                (
                    Node::branch(Kind::Row, vec![selected, symbol, blank()]),
                    Some(2),
                )
            } else {
                (Node::branch(Kind::Row, vec![selected, symbol]), Some(1))
            };
            node.validate()?;
            self.replace(node);
            if let Some(focus) = focus {
                self.selection.push(focus);
            }
            return Ok(());
        }
        let (mut node, focus) = match tool {
            "fraction" => (Node::branch(Kind::Fraction, vec![selected, blank()]), 1),
            "root" => (Node::branch(Kind::Root, vec![selected]), 0),
            "nth_root" => (Node::branch(Kind::Root, vec![blank(), selected]), 0),
            "power" => (Node::branch(Kind::Power, vec![selected, blank()]), 1),
            "subscript" => (Node::branch(Kind::Subscript, vec![selected, blank()]), 1),
            "group" => (
                Node::branch(Kind::Group("(".into(), ")".into()), vec![selected]),
                0,
            ),
            "integral" | "sum" => {
                let mut big = Node::branch(Kind::BigOperator, vec![blank(), blank()]);
                big.value = if tool == "sum" { "sum" } else { "int" }.into();
                (Node::branch(Kind::Row, vec![big, selected]), 0)
            }
            "limit" => (
                Node::branch(
                    Kind::Row,
                    vec![Node::branch(Kind::Limit, vec![blank()]), selected],
                ),
                0,
            ),
            "matrix" => (
                Node::branch(
                    Kind::Grid {
                        style: "bmatrix".into(),
                        columns: 2,
                    },
                    vec![selected, blank(), blank(), blank()],
                ),
                1,
            ),
            "cases" => (
                Node::branch(
                    Kind::Grid {
                        style: "cases".into(),
                        columns: 2,
                    },
                    vec![
                        selected,
                        Node::leaf(Kind::Text, ""),
                        blank(),
                        Node::leaf(Kind::Text, ""),
                    ],
                ),
                1,
            ),
            "pi" => (Node::leaf(Kind::Symbol, "pi"), 0),
            _ => return Err(tr!("无法识别这个结构。").into()),
        };
        node.validate()?;
        if node.children.is_empty() {
            self.replace(node);
        } else {
            let descend = matches!(tool, "integral" | "sum" | "limit");
            self.replace(std::mem::replace(&mut node, Node::blank()));
            self.selection.push(focus);
            if descend {
                self.selection.push(0);
            }
        }
        Ok(())
    }
    pub fn resize_grid(&mut self, rows: usize, columns: usize) -> Result<(), String> {
        if !(1..=6).contains(&rows) || !(1..=6).contains(&columns) {
            return Err(tr!("矩阵最多为 6 × 6。").into());
        }
        let mut path = self.selection.clone();
        loop {
            if let Some(Node {
                kind:
                    Kind::Grid {
                        style,
                        columns: old_cols,
                    },
                children,
                ..
            }) = self.root.at(&path)
            {
                let old_cols = *old_cols;
                let columns = if style == "cases" { 2 } else { columns };
                let new = (0..rows)
                    .flat_map(|r| (0..columns).map(move |c| (r, c)))
                    .map(|(r, c)| {
                        if c < old_cols {
                            children.get(r * old_cols + c).cloned()
                        } else {
                            None
                        }
                        .unwrap_or_else(|| {
                            if style == "cases" && c == 1 {
                                Node::leaf(Kind::Text, "")
                            } else {
                                Node::blank()
                            }
                        })
                    })
                    .collect();
                let node = Node::branch(
                    Kind::Grid {
                        style: style.clone(),
                        columns,
                    },
                    new,
                );
                node.validate()?;
                self.selection = path;
                self.replace(node);
                return Ok(());
            }
            if path.pop().is_none() {
                return Ok(());
            }
        }
    }
    pub fn grid_size(&self) -> Option<(usize, usize)> {
        let mut path = self.selection.clone();
        loop {
            if let Some(Node {
                kind: Kind::Grid { columns, .. },
                children,
                ..
            }) = self.root.at(&path)
            {
                return Some((children.len() / columns, *columns));
            }
            path.pop()?;
        }
    }
    pub fn content(&self) -> Result<(String, String), String> {
        if let Some(error) = self.error() {
            return Err(error.to_owned());
        }
        if !self.dirty || self.root == self.original_root {
            return Ok(self.original.clone());
        }
        if !self.root.complete() {
            return Err(tr!("还有未填写的公式位置。").into());
        }
        if self.root.is_text() {
            return Ok((self.root.value.clone(), String::new()));
        }
        self.root.validate()?;
        Ok((self.root.plain(), self.root.tex()))
    }
}

// Do not consume a sum, relation or a neighboring matrix cell as an implicit
// operand. Parenthesized expressions and function calls stay together.
fn operand_start(input: &str) -> usize {
    let mut depth = 0usize;
    for (index, character) in input.char_indices().rev() {
        match character {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' if depth > 0 => depth -= 1,
            '(' | '[' | '{' => return index + character.len_utf8(),
            '+' | '-' | '=' | '<' | '>' | ',' | ';' if depth == 0 => {
                return index + character.len_utf8();
            }
            _ => {}
        }
    }
    0
}

/// Only recognizable mathematical runs are converted; prose remains prose.
pub fn automatic_content(input: &str) -> Result<Node, String> {
    if input.chars().count() > MAX_INPUT {
        return Err(tr!("内容不能超过 1000 个字符。").into());
    }
    if looks_mathematical(input)
        && let Ok(node) = expression(input)
    {
        return Ok(node);
    }
    let mut nodes = vec![];
    let mut run = String::new();
    let mut ascii = None;
    let flush = |run: &mut String, nodes: &mut Vec<Node>| {
        if run.is_empty() {
            return;
        }
        let trimmed = run.trim();
        if looks_mathematical(trimmed)
            && let Ok(math) = expression(trimmed)
        {
            if run.starts_with(char::is_whitespace) {
                nodes.push(Node::leaf(Kind::Text, " "));
            }
            nodes.push(math);
            if run.ends_with(char::is_whitespace) {
                nodes.push(Node::leaf(Kind::Text, " "));
            }
        } else {
            nodes.push(Node::leaf(Kind::Text, run.clone()));
        }
        run.clear();
    };
    for c in input.chars() {
        let next =
            c.is_ascii() || vocabulary::math_char(c) || "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻ⁿ₀₁₂₃₄₅₆₇₈₉₊₋".contains(c);
        if ascii.is_some_and(|old| old != next) {
            flush(&mut run, &mut nodes);
        }
        ascii = Some(next);
        run.push(c);
    }
    flush(&mut run, &mut nodes);
    if nodes.iter().all(Node::is_text) {
        Ok(Node::leaf(Kind::Text, input))
    } else if nodes.len() == 1 {
        Ok(nodes.remove(0))
    } else {
        Ok(Node::branch(Kind::Row, nodes))
    }
}

pub(crate) fn looks_mathematical(s: &str) -> bool {
    s.contains(['/', '^', '_', '=', '|'])
        || s.contains("[[")
        || s.contains("'(")
        || s.chars().any(vocabulary::math_char)
        || vocabulary::normalize(s).contains(['^', '_'])
        || s.char_indices().any(|(index, c)| {
            c == '('
                && s[..index]
                    .rsplit(|c: char| !c.is_ascii_alphabetic())
                    .next()
                    .and_then(vocabulary::function)
                    .is_some()
        })
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Word(String),
    Quoted(String),
    Sign(char),
    Arrow,
    End,
}

pub fn expression(input: &str) -> Result<Node, String> {
    if input.trim().is_empty() {
        return Ok(Node::blank());
    }
    if input.chars().count() > MAX_INPUT {
        return Err(tr!("请输入普通表达式，例如 sin(pi/3)。").into());
    }
    let normalized = vocabulary::normalize(input);
    let input = normalized.as_str();
    let mut tokens = vec![];
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c == '"' {
            let mut value = String::new();
            let mut closed = false;
            while let Some(next) = chars.next() {
                if next == '"' {
                    closed = true;
                    break;
                }
                if next == '\\' {
                    if let Some(escaped) = chars.next() {
                        value.push(escaped);
                    }
                } else {
                    value.push(next);
                }
            }
            if !closed {
                return Err(tr!("文字引号尚未闭合。").into());
            }
            tokens.push(Token::Quoted(value));
        } else if c.is_ascii_alphanumeric()
            || c == '.'
            || vocabulary::math_char(c) && !"√°′".contains(c)
        {
            let number = c.is_ascii_digit() || c == '.';
            let identifier = c.is_ascii_alphabetic();
            let mut word = c.to_string();
            while let Some(next) = chars.peek().copied() {
                if (number && (next.is_ascii_digit() || next == '.'))
                    || (identifier && next.is_ascii_alphabetic())
                {
                    word.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            tokens.push(Token::Word(word));
        } else if c == '-' && chars.peek() == Some(&'>') {
            chars.next();
            tokens.push(Token::Arrow);
        } else if c == '\\' {
            return Err(tr!("请输入普通表达式，例如 sin(pi/3)。").into());
        } else if matches!(c, '<' | '>' | '!') && chars.peek() == Some(&'=') {
            chars.next();
            tokens.push(Token::Sign(match c {
                '<' => '≤',
                '>' => '≥',
                _ => '≠',
            }));
        } else if "()+-*/^_=<>?,;[]{}!|√×÷≤≥≠→'′°□".contains(c) {
            tokens.push(if c == '→' {
                Token::Arrow
            } else {
                Token::Sign(c)
            });
        } else {
            return Err(tr!("这里是公式位置；普通文字可用“文字”按钮加入。").into());
        }
    }
    tokens.push(Token::End);
    let mut parser = Parser {
        tokens,
        pos: 0,
        depth: 0,
        limit_condition: false,
    };
    let root = parser.expr(0)?;
    if parser.peek() != &Token::End {
        return Err(tr!("请检查括号或表达式。").into());
    }
    root.validate()?;
    Ok(root)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
    limit_condition: bool,
}
impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }
    fn take(&mut self) -> Token {
        let token = self.peek().clone();
        self.pos += 1;
        token
    }
    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.peek() == &Token::Sign(c) {
            self.take();
            Ok(())
        } else {
            Err(tr!("缺少 {c}。", c = c))
        }
    }
    fn expr(&mut self, min: u8) -> Result<Node, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(tr!("公式结构过深。").into());
        }
        let mut left = self.atom()?;
        loop {
            if self.limit_condition
                && matches!(self.peek(), Token::Sign('+' | '-'))
                && matches!(self.tokens.get(self.pos + 1), Some(Token::Sign(',' | ')')))
            {
                let Token::Sign(sign) = self.take() else {
                    unreachable!()
                };
                left = Node::branch(
                    Kind::Power,
                    vec![left, Node::leaf(Kind::Literal, sign.to_string())],
                );
                break;
            }
            let (precedence, sign, implicit) = match self.peek() {
                Token::Sign('^' | '_') => (5, self.peek().clone(), false),
                Token::Sign('*' | '/' | '×' | '÷') => (3, self.peek().clone(), false),
                Token::Sign('+' | '-') => (2, self.peek().clone(), false),
                Token::Sign('=' | '<' | '>' | '≤' | '≥' | '≠') | Token::Arrow => {
                    (1, self.peek().clone(), false)
                }
                Token::Sign('!') => {
                    self.take();
                    left = row(left, Node::leaf(Kind::Literal, "!"), None);
                    continue;
                }
                Token::Sign('\'' | '′' | '°') => {
                    let prime = self.take() != Token::Sign('°');
                    let exponent = Node::leaf(Kind::Symbol, if prime { "prime" } else { "circ" });
                    if prime && is_primes(&left) {
                        left = row(left, exponent, None);
                    } else if prime && left.kind == Kind::Power && is_primes(&left.children[1]) {
                        let old = std::mem::replace(&mut left.children[1], Node::blank());
                        left.children[1] = row(old, exponent, None);
                    } else {
                        left = Node::branch(Kind::Power, vec![left, exponent]);
                    }
                    continue;
                }
                Token::Word(s)
                    if [
                        "times", "div", "leq", "geq", "neq", "approx", "in", "notin", "subseteq",
                        "cup", "cap", "pm", "mp", "to",
                    ]
                    .contains(&vocabulary::symbol(s).unwrap_or("")) =>
                {
                    (
                        if ["times", "div"].contains(&vocabulary::symbol(s).unwrap()) {
                            3
                        } else {
                            1
                        },
                        self.peek().clone(),
                        false,
                    )
                }
                Token::Word(_) | Token::Quoted(_) | Token::Sign('(' | '√') => {
                    (3, Token::Sign(' '), true)
                }
                _ => break,
            };
            if precedence < min {
                break;
            }
            if !implicit {
                self.take();
            }
            let right = self.expr(precedence + 1)?;
            left = match sign {
                Token::Sign('/' | '÷') => {
                    Node::branch(Kind::Fraction, vec![ungroup(left), ungroup(right)])
                }
                Token::Sign('^') if left.kind == Kind::Subscript => {
                    let mut children = left.children;
                    children.push(ungroup(right));
                    Node::branch(Kind::SubSup, children)
                }
                Token::Sign('_') if left.kind == Kind::Power => {
                    let mut children = left.children;
                    children.insert(1, ungroup(right));
                    Node::branch(Kind::SubSup, children)
                }
                Token::Sign('^') => Node::branch(Kind::Power, vec![left, ungroup(right)]),
                Token::Sign('_') => Node::branch(Kind::Subscript, vec![left, ungroup(right)]),
                Token::Sign('*' | '×') => row(left, Node::leaf(Kind::Symbol, "cdot"), Some(right)),
                Token::Arrow => row(left, Node::leaf(Kind::Symbol, "to"), Some(right)),
                Token::Sign('≤') => row(left, Node::leaf(Kind::Symbol, "leq"), Some(right)),
                Token::Sign('≥') => row(left, Node::leaf(Kind::Symbol, "geq"), Some(right)),
                Token::Sign('≠') => row(left, Node::leaf(Kind::Symbol, "neq"), Some(right)),
                Token::Sign(' ') => row(left, right, None),
                Token::Word(s) if vocabulary::symbol(&s) == Some("div") => {
                    Node::branch(Kind::Fraction, vec![ungroup(left), ungroup(right)])
                }
                Token::Word(s) if vocabulary::symbol(&s) == Some("times") => {
                    row(left, Node::leaf(Kind::Symbol, "cdot"), Some(right))
                }
                Token::Word(s) => row(
                    left,
                    Node::leaf(Kind::Symbol, vocabulary::symbol(&s).unwrap()),
                    Some(right),
                ),
                Token::Sign(s) => row(left, Node::leaf(Kind::Literal, s.to_string()), Some(right)),
                _ => unreachable!(),
            };
        }
        self.depth -= 1;
        Ok(left)
    }
    fn atom(&mut self) -> Result<Node, String> {
        match self.take() {
            Token::Word(s)
                if self.peek() == &Token::Sign('(') && vocabulary::function(&s).is_some() =>
            {
                let name = vocabulary::function(&s).unwrap();
                self.take();
                let previous_condition = self.limit_condition;
                self.limit_condition = name == "lim";
                let mut args = vec![self.expr(0)?];
                self.limit_condition = previous_condition;
                while self.peek() == &Token::Sign(',') {
                    self.take();
                    args.push(self.expr(0)?);
                }
                self.expect(')')?;
                match (name, args.len()) {
                    ("sqrt", 1) => Ok(Node::branch(Kind::Root, args)),
                    ("root", 2) => Ok(Node::branch(Kind::Root, args)),
                    ("sum" | "int", 3) => {
                        let body = args.pop().unwrap();
                        let mut op = Node::branch(Kind::BigOperator, args);
                        op.value = name.into();
                        Ok(Node::branch(Kind::Row, vec![op, body]))
                    }
                    ("sum" | "int", 2) => {
                        let mut op = Node::branch(Kind::BigOperator, args);
                        op.value = name.into();
                        Ok(op)
                    }
                    ("sum" | "int", 1) => {
                        let body = args.pop().unwrap();
                        let mut op = Node::branch(
                            Kind::BigOperator,
                            vec![Node::leaf(Kind::Space, ""), Node::leaf(Kind::Space, "")],
                        );
                        op.value = name.into();
                        Ok(Node::branch(Kind::Row, vec![op, body]))
                    }
                    ("lim", 2) => {
                        let body = args.pop().unwrap();
                        Ok(Node::branch(
                            Kind::Row,
                            vec![Node::branch(Kind::Limit, args), body],
                        ))
                    }
                    ("lim", 1) => Ok(Node::branch(Kind::Limit, args)),
                    ("cases", n) if n >= 2 && n % 2 == 0 => Ok(Node::branch(
                        Kind::Grid {
                            style: "cases".into(),
                            columns: 2,
                        },
                        args,
                    )),
                    ("log", 2) => Ok(logarithm(args.remove(0), args.remove(0))),
                    (_, 1)
                        if vocabulary::is_operator(name)
                            || ["abs", "norm", "vec", "dot", "ddot", "bar", "hat"]
                                .contains(&name) =>
                    {
                        Ok(applied_function(name, args.remove(0)))
                    }
                    _ => Err(tr!("函数中的项目数量不合适。").into()),
                }
            }
            Token::Word(s) => {
                if let Some(letter) = vocabulary::blackboard(&s) {
                    Ok(Node::leaf(Kind::Styled("mathbb".into()), letter))
                } else if let Some(name) = vocabulary::symbol(&s) {
                    Ok(Node::leaf(Kind::Symbol, name))
                } else if let Some(name) =
                    vocabulary::function(&s).filter(|s| vocabulary::is_operator(s))
                {
                    Ok(Node::leaf(Kind::Operator, name))
                } else {
                    Ok(Node::leaf(Kind::Literal, s))
                }
            }
            Token::Quoted(s) => Ok(Node::leaf(Kind::Text, s)),
            Token::Sign('\'' | '′') => Ok(Node::leaf(Kind::Symbol, "prime")),
            Token::Sign('(' | '{') => {
                let close = if self.tokens[self.pos - 1] == Token::Sign('{') {
                    '}'
                } else {
                    ')'
                };
                let body = self.expr(0)?;
                self.expect(close)?;
                Ok(Node::branch(
                    Kind::Group("(".into(), ")".into()),
                    vec![body],
                ))
            }
            Token::Sign('|') => {
                let norm = self.peek() == &Token::Sign('|');
                if norm {
                    self.take();
                }
                let body = self.expr(0)?;
                self.expect('|')?;
                if norm {
                    self.expect('|')?;
                }
                Ok(applied_function(if norm { "norm" } else { "abs" }, body))
            }
            Token::Sign('[') => {
                let mut cells = vec![];
                let mut columns = 0;
                loop {
                    self.expect('[')?;
                    let mut row = vec![self.expr(0)?];
                    while self.peek() == &Token::Sign(',') {
                        self.take();
                        row.push(self.expr(0)?);
                    }
                    self.expect(']')?;
                    if columns == 0 {
                        columns = row.len();
                    }
                    if row.len() != columns || columns > 6 || cells.len() >= 36 {
                        return Err(tr!("矩阵须每行同样列数，最多 6 × 6。").into());
                    }
                    cells.extend(row);
                    if self.peek() != &Token::Sign(',') {
                        break;
                    }
                    self.take();
                }
                self.expect(']')?;
                Ok(Node::branch(
                    Kind::Grid {
                        style: "bmatrix".into(),
                        columns,
                    },
                    cells,
                ))
            }
            Token::Sign('-' | '+') => {
                let sign = if self.tokens[self.pos - 1] == Token::Sign('-') {
                    "-"
                } else {
                    "+"
                };
                if matches!(self.peek(), Token::Sign(')' | '}' | ',' | ']') | Token::End) {
                    return Ok(Node::leaf(Kind::Literal, sign));
                }
                Ok(Node::branch(
                    Kind::Row,
                    vec![Node::leaf(Kind::Literal, sign), self.expr(4)?],
                ))
            }
            Token::Sign('√') => Ok(Node::branch(Kind::Root, vec![ungroup(self.expr(4)?)])),
            Token::Sign('?') => Ok(Node::leaf(Kind::Literal, "?")),
            Token::Sign('□') => Ok(Node::blank()),
            _ => Err(tr!("这个公式位置还需要内容。").into()),
        }
    }
}
fn applied_function(name: &str, body: Node) -> Node {
    match name {
        "abs" => Node::branch(Kind::Group("|".into(), "|".into()), vec![body]),
        "norm" => Node::branch(Kind::Group(r"\lVert".into(), r"\rVert".into()), vec![body]),
        "vec" | "dot" | "ddot" | "bar" | "hat" => {
            let mut node = Node::branch(Kind::Accent, vec![body]);
            node.value = name.into();
            node
        }
        _ => Node::branch(
            Kind::Row,
            vec![
                Node::leaf(Kind::Operator, name),
                Node::branch(Kind::Group("(".into(), ")".into()), vec![body]),
            ],
        ),
    }
}

fn logarithm(base: Node, body: Node) -> Node {
    Node::branch(
        Kind::Row,
        vec![
            Node::branch(
                Kind::Subscript,
                vec![Node::leaf(Kind::Operator, "log"), base],
            ),
            Node::branch(Kind::Group("(".into(), ")".into()), vec![body]),
        ],
    )
}

fn is_primes(node: &Node) -> bool {
    node.kind == Kind::Symbol && node.value == "prime"
        || node.kind == Kind::Row && node.children.iter().all(is_primes)
}

fn row(left: Node, middle: Node, right: Option<Node>) -> Node {
    let mut children = if left.kind == Kind::Row {
        left.children
    } else {
        vec![left]
    };
    if middle.kind == Kind::Row {
        children.extend(middle.children);
    } else {
        children.push(middle);
    }
    if let Some(right) = right {
        children.push(right);
    }
    Node::branch(Kind::Row, children)
}
fn ungroup(node: Node) -> Node {
    // Preserve absolute values and norms when a structural slot absorbs parentheses.
    if matches!(&node.kind, Kind::Group(left, right) if left == "(" && right == ")") {
        node.children.into_iter().next().unwrap()
    } else {
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_templates_remain_navigable_without_becoming_answers() {
        let mut field = Field::new("", "");
        field.insert_at("math").unwrap();
        field.insert_at("root").unwrap();
        assert!(field.navigate("parent"));
        let generated = field.input();
        field.update(&generated).unwrap();
        assert!(field.navigate("next"));
        assert_eq!(field.input(), "");
        assert!(field.content().is_err());
        field.insert_at("fraction").unwrap();
        assert_eq!(field.input(), "");
        assert!(field.navigate("next"));
        field.update("2").unwrap();
        assert!(field.navigate("previous"));
        field.update("3").unwrap();
        assert_eq!(field.root, expression("sqrt(3/2)").unwrap());
        assert!(field.navigate("down"));
        assert_eq!(field.input(), "2");
        assert!(field.navigate("up"));
        assert_eq!(field.input(), "3");
    }

    #[test]
    fn text_commands_replace_ranges_and_restore_their_caret() {
        let mut field = Field::new("12+34", "12+34");
        field.set_text_selection(3, 5);
        field.type_text("7").unwrap();
        assert_eq!(field.input(), "12+7");
        assert_eq!(
            field.text_selection(),
            TextSelection {
                anchor: 4,
                focus: 4
            }
        );
        field.undo(false);
        assert_eq!(field.input(), "12+34");
        assert_eq!(
            field.text_selection(),
            TextSelection {
                anchor: 3,
                focus: 5
            }
        );
        field.set_text_selection(2, 2);
        field.backspace().unwrap();
        assert_eq!(field.input(), "1+34");
        field.undo(false);
        assert_eq!(field.input(), "12+34");
        assert_eq!(
            field.text_selection(),
            TextSelection {
                anchor: 2,
                focus: 2
            }
        );
        field.backspace().unwrap();
        field.set_text_selection(0, 0);
        field.type_text("9").unwrap();
        assert_eq!(field.input(), "91+34");
        let mut prose = Field::new("中文π", "");
        prose.set_text_selection(6, 6);
        prose.backspace().unwrap();
        assert_eq!(prose.input(), "中π");
    }

    #[test]
    fn invalid_drafts_survive_clear_and_undo_without_being_saved() {
        let mut field = Field::new("3", "3");
        assert!(field.update("sqrt(").is_err());
        field.set_text_selection(2, 5);
        assert!(field.content().is_err());
        field.clear();
        field.undo(false);
        assert_eq!(field.input(), "sqrt(");
        assert!(field.error().is_some());
        assert_eq!(
            field.text_selection(),
            TextSelection {
                anchor: 2,
                focus: 5
            }
        );
        assert!(field.content().is_err());
        field.undo(false);
        assert_eq!(field.input(), "3");
        assert_eq!(field.content().unwrap(), ("3".into(), "3".into()));
        field.undo(true);
        assert_eq!(field.input(), "sqrt(");
    }

    #[test]
    fn templates_wrap_the_selected_range_or_local_operand() {
        let mut field = Field::new("12+34", "12+34");
        field.set_text_selection(3, 5);
        field.insert_at("root").unwrap();
        assert_eq!(field.root, expression("12+sqrt(34)").unwrap());
        field.undo(false);
        field.set_text_selection(0, 2);
        field.insert_at("fraction").unwrap();
        assert!(
            field.input().is_empty(),
            "The denominator is the first empty slot"
        );
        field.update("2").unwrap();
        assert_eq!(field.root, expression("12/2+34").unwrap());
        field.undo(false);
        field.undo(false);
        field.set_text_selection(0, 0);
        field.insert_at("pi").unwrap();
        assert!(field.root.tex().contains("\\pi"));
    }

    #[test]
    fn inserted_structures_keep_neighboring_terms_outside_their_slots() {
        let mut field = Field::new("x+y", "x+y");
        field.set_text_selection(1, 1);
        field.insert_at("pi").unwrap();
        assert_eq!(field.root.plain(), "xπ+y");
        assert_eq!(field.input(), "π");
        assert_eq!(
            field.text_selection(),
            TextSelection {
                anchor: 2,
                focus: 2
            }
        );
        field.undo(false);
        field.set_text_selection(0, 1);
        field.insert_at("fraction").unwrap();
        field.update("2").unwrap();
        assert_eq!(field.root, expression("x/2+y").unwrap());
        let mut field = Field::new("1234", "1234");
        field.set_text_selection(1, 3);
        field.insert_at("fraction").unwrap();
        field.update("5").unwrap();
        assert_eq!(field.root.children[1], expression("23/5").unwrap());
        assert_eq!(field.root.children[2].value, "4");
        let mut field = Field::new("12+34", "12+34");
        field.insert_at("root").unwrap();
        assert_eq!(field.root.plain(), "12+34sqrt()");
        assert!(field.input().is_empty());
        assert!(field.content().is_err());
    }

    #[test]
    fn backspace_enters_filled_structures_and_removes_only_empty_templates() {
        let mut field = Field::new("sqrt(3)/2", r"\frac{\sqrt{3}}{2}");
        let root = field.root.clone();
        field.backspace().unwrap();
        assert_eq!(field.root, root);
        assert_eq!(field.selection, vec![1]);
        assert!(!field.can_undo());
        field.backspace().unwrap();
        assert_eq!(field.root.children[0], root.children[0]);
        assert!(field.input().is_empty());
        field.backspace().unwrap();
        assert_eq!(field.input(), "3");
        assert_eq!(field.root.children[0], root.children[0]);
        let mut empty = Field::new("", "");
        empty.insert_at("math").unwrap();
        empty.insert_at("root").unwrap();
        empty.insert_at("fraction").unwrap();
        let before = empty.root.clone();
        let selected = empty.selection.clone();
        empty.backspace().unwrap();
        assert_eq!(empty.root, Node::blank());
        empty.undo(false);
        assert_eq!(empty.root, before);
        assert_eq!(empty.selection, selected);
        assert!(empty.content().is_err());
    }

    #[test]
    fn leaving_a_root_keeps_it_and_continues_after_it() {
        let mut field = Field::new("", "");
        field.insert_at("math").unwrap();
        field.insert_at("root").unwrap();
        field.update("3").unwrap();
        assert!(field.navigate("exit"));
        assert_eq!(field.input(), "sqrt(3)");
        field.type_text("/2").unwrap();
        assert_eq!(field.root, expression("sqrt(3)/2").unwrap());
        assert!(!field.can_navigate("exit"));
    }

    #[test]
    fn clearing_a_field_restores_the_whole_formula_and_selection_on_undo() {
        let mut field = Field::new("sqrt(3)/2", r"\frac{\sqrt{3}}{2}");
        field.select(vec![0, 0]);
        let before = field.root.clone();
        let selection = field.selection.clone();
        field.insert("clear").unwrap();
        assert_eq!(field.input(), "");
        assert!(field.selection.is_empty());
        field.clear(); // An already empty field does not add a history entry.
        assert!(
            field.content().is_err(),
            "Clearing does not create an answer of zero"
        );
        field.undo(false);
        assert_eq!(field.root, before);
        assert_eq!(field.selection, selection);
        assert_eq!(
            field.content().unwrap(),
            ("sqrt(3)/2".into(), r"\frac{\sqrt{3}}{2}".into())
        );
        field.undo(true);
        assert_eq!(field.input(), "");
        assert!(field.content().is_err());
    }

    #[test]
    fn clearing_prose_and_unfilled_templates_is_undoable() {
        let mut prose = Field::new("普通问题", "");
        prose.clear();
        assert!(prose.root.is_text());
        assert_eq!(prose.input(), "");
        prose.undo(false);
        assert_eq!(prose.content().unwrap(), ("普通问题".into(), "".into()));
        assert!(!from_latex(r"\sqrt{}").unwrap().complete());
        let mut formula = Field::new("", "");
        formula.insert("math").unwrap();
        formula.insert("root").unwrap();
        assert!(formula.content().is_err());
        formula.clear();
        formula.undo(false);
        assert_eq!(formula.root.tex(), r"\sqrt{\square}");
        assert!(formula.content().is_err());
    }

    #[test]
    fn root_first_fraction_can_move_out_without_retyping() {
        let mut field = Field::new("", "");
        field.insert("math").unwrap();
        field.insert("root").unwrap();
        field.insert("fraction").unwrap();
        field.update("2").unwrap();
        field.select(vec![0, 0]);
        field.update("3").unwrap();
        assert_eq!(field.root, expression("sqrt(3/2)").unwrap());

        field.select(vec![0, 1]);
        field.insert("move_out").unwrap();
        assert_eq!(field.root, expression("sqrt(3)/2").unwrap());
        assert_eq!(field.input(), "2");
        assert!(field.content().is_ok());
        field.undo(false);
        assert_eq!(field.root, expression("sqrt(3/2)").unwrap());
        field.undo(true);
        assert_eq!(field.root, expression("sqrt(3)/2").unwrap());
    }

    #[test]
    fn moving_structures_preserves_every_slot_and_its_selection() {
        for (before, after, paths) in [
            ("sqrt(3/2)", "sqrt(3)/2", vec![vec![0, 0], vec![0, 1]]),
            ("sqrt(x^2)", "sqrt(x)^2", vec![vec![0, 0], vec![0, 1]]),
            ("sqrt(x_1)", "sqrt(x)_1", vec![vec![0, 0], vec![0, 1]]),
            (
                "sqrt(x_1^2)",
                "sqrt(x)_1^2",
                vec![vec![0, 0], vec![0, 1], vec![0, 2]],
            ),
            ("vec(a/b)", "vec(a)/b", vec![vec![0, 0], vec![0, 1]]),
        ] {
            for path in paths {
                let mut field = Field::new(before, &expression(before).unwrap().tex());
                field.select(path);
                let input = field.input();
                assert!(field.can_move_out());
                field.insert("move_out").unwrap();
                assert_eq!(field.root, expression(after).unwrap(), "{before}");
                assert_eq!(
                    field.input(),
                    input,
                    "The selected slot follows its content"
                );
                assert!(field.content().is_ok());
                field.update("7").unwrap();
                assert_eq!(field.selected().value, "7");
                field.undo(false);
                field.undo(false);
                assert_eq!(field.root, expression(before).unwrap());
            }
        }
    }

    #[test]
    fn structure_repairs_preserve_siblings_and_unfilled_slots() {
        let mut field = Field::new("求 ", "");
        field.insert("math").unwrap();
        field.insert("root").unwrap();
        field.insert("fraction").unwrap();
        field.update("2").unwrap();
        field.insert("move_out").unwrap();
        assert_eq!(field.selection, vec![1, 1]);
        assert!(field.content().is_err(), "The numerator is still unfilled");
        assert_eq!(field.root.children[0].value, "求 ");
        field.select(vec![1, 0, 0]);
        field.update("3").unwrap();
        assert!(field.content().unwrap().1.contains(r"\text{求 }"));
        assert_eq!(field.root.children[1], expression("sqrt(3)/2").unwrap());

        let mut field = Field::new("", "");
        field.insert("nth_root").unwrap();
        field.update("4").unwrap();
        assert!(!field.can_move_out());
        assert!(
            !field.can_unwrap(),
            "The root's degree must not lose its value"
        );
        field.select(vec![1]);
        field.update("3/2").unwrap();
        field.select(vec![1, 0]);
        field.insert("move_out").unwrap();
        assert_eq!(field.root.tex(), r"\frac{\sqrt[4]{3}}{2}");
        assert_eq!(field.selection, vec![0, 1]);
        assert_eq!(field.input(), "3");
    }

    #[test]
    fn removing_a_wrapper_retains_its_content_and_can_be_undone() {
        let mut field = Field::new("sqrt(3/2)", r"\sqrt{\frac{3}{2}}");
        field.select(vec![0, 1]);
        assert!(field.can_unwrap());
        field.insert("unwrap").unwrap();
        assert_eq!(field.root, expression("3/2").unwrap());
        assert_eq!(field.selection, vec![1]);
        assert_eq!(field.input(), "2");
        field.undo(false);
        assert_eq!(field.root, expression("sqrt(3/2)").unwrap());
        assert_eq!(field.selection, vec![0, 1]);
        field.undo(true);
        field.select(vec![0]);
        field.insert("root").unwrap();
        assert_eq!(field.root, expression("sqrt(3)/2").unwrap());

        let mut field = Field::new("", "");
        field.insert("group").unwrap();
        field.update("x+1").unwrap();
        field.parent();
        field.insert("unwrap").unwrap();
        assert_eq!(field.root, expression("x+1").unwrap());
        assert_eq!(field.selection, Vec::<usize>::new());
        assert_eq!(field.input(), "x+1");
    }

    #[test]
    fn unavailable_repairs_do_not_change_formula_or_history() {
        for input in ["3/2", "x^2", "[[1,2],[3,4]]", "log(2,8)"] {
            let mut field = Field::new(input, &expression(input).unwrap().tex());
            let root = field.root.clone();
            assert!(!field.can_move_out(), "{input}");
            assert!(!field.can_unwrap(), "{input}");
            assert!(field.insert("move_out").is_err());
            assert!(field.insert("unwrap").is_err());
            assert_eq!(field.root, root);
            assert!(!field.is_dirty());
            assert!(!field.can_undo());
        }
    }

    #[test]
    fn expressions_build_the_same_structures_as_visual_tools() {
        for input in [
            "sin(pi/3)",
            "sqrt(3)/2",
            "a^2+b^2=c^2",
            "x_1^2",
            "x^2_1",
            "int(0,1,x^2)",
            "sum(k=1,n,k)",
            "lim(x->0,sin(x)/x)",
            "[[1,2],[3,4]]",
            "cases(x,x>=0,-x,x<0)",
        ] {
            let root = expression(input).unwrap_or_else(|e| panic!("{input}: {e}"));
            assert!(root.complete());
            crate::formula::validate(&root.tex()).unwrap();
            let (_, leaves) = root.marked_tex();
            assert!(!leaves.is_empty());
            assert_eq!(expression(&root.editor_input()).unwrap().tex(), root.tex());
        }
        let mut visual = Field::new("", "");
        visual.insert("math").unwrap();
        visual.update("sqrt(3)").unwrap();
        visual.insert("fraction").unwrap();
        visual.update("2").unwrap();
        assert_eq!(visual.root, expression("sqrt(3)/2").unwrap());
    }

    #[test]
    fn imported_content_and_cancel_are_lossless_until_edited() {
        let original = ("sin(π/3) = ?", r"\sin\left(\frac{\pi}{3}\right)=?");
        let mut field = Field::new(original.0, original.1);
        assert_eq!(
            field.content().unwrap(),
            (original.0.into(), original.1.into())
        );
        let (_, leaves) = field.root.marked_tex();
        let denominator = leaves
            .iter()
            .find(|leaf| field.root.at(&leaf.path).unwrap().value == "3")
            .unwrap()
            .path
            .clone();
        field.select(denominator);
        field.update("6").unwrap();
        assert!(field.content().unwrap().1.contains("6"));
        field.undo(false);
        assert_eq!(field.root, from_latex(original.1).unwrap());
    }

    #[test]
    fn mixed_text_and_grid_cells_preserve_native_text() {
        let root = automatic_content("计算 sin(pi/3) 的值").unwrap();
        assert!(root.tex().contains(r"\text{计算}"));
        assert!(root.tex().contains(r"\sin"));
        let mut field = Field::new("", "");
        field.insert("matrix").unwrap();
        assert!(field.content().is_err());
        field.resize_grid(3, 3).unwrap();
        assert_eq!(field.grid_size(), Some((3, 3)));
        assert_eq!(field.root.children.len(), 9);
        assert_eq!(automatic_content("hello world").unwrap().kind, Kind::Text);
    }

    #[test]
    fn common_functions_and_copied_notation_remain_editable() {
        for input in [
            "log(2,x)",
            "log_2(x)",
            "Log(x)",
            "ln(x)",
            "lg(x)",
            "sin^2(x)",
            "arcsin(x)",
            "asin(x)",
            "sinh(x)",
            "|x|",
            "||x||",
            "x²+x₁",
            "a_{n+1}",
            "a_(n+1)",
            "ε+epsilon",
            "x∈ℝ",
            "a±b",
            "vec(v)",
            "dot(x)",
            "ddot(x)",
            "bar(x)",
            "hat(x)",
            "f'(x)",
            "f''(x)",
            "30°",
            "lim(x->0+,1/x)",
            "lim(x->0-,1/x)",
        ] {
            let node = expression(input).unwrap_or_else(|error| panic!("{input}: {error}"));
            assert!(node.complete(), "{input}");
            assert_eq!(
                expression(&node.editor_input()).unwrap().tex(),
                node.tex(),
                "{input}"
            );
            let imported = from_latex(&node.tex()).unwrap();
            assert_ne!(imported.kind, Kind::Raw, "{input}");
            assert!(
                expression(&imported.editor_input()).is_ok(),
                "{input}: {}",
                imported.editor_input()
            );
        }
        assert_eq!(
            expression("log(2,x)").unwrap(),
            expression("log_2(x)").unwrap()
        );
        assert!(expression("sin^2(x)").unwrap().tex().contains(r"\sin"));
        assert!(expression("asin(x)").unwrap().tex().contains(r"\arcsin"));
        assert_eq!(expression("x÷2").unwrap(), expression("x/2").unwrap());
    }

    #[test]
    fn logarithm_template_has_independent_base_and_argument_slots() {
        let mut field = Field::new("", "");
        field.insert("math").unwrap();
        field.insert("function:log_base").unwrap();
        assert_eq!(field.selection, vec![0, 1]);
        assert!(field.content().is_err());
        field.update("2").unwrap();
        field.select(vec![1, 0]);
        field.update("8").unwrap();
        assert_eq!(
            field.content().unwrap().1,
            expression("log(2,8)").unwrap().tex()
        );
        field.undo(false);
        assert!(field.content().is_err());
        field.undo(true);
        assert!(field.content().is_ok());

        field.select(vec![]);
        field.update(r"\log_{3}(9)").unwrap();
        assert!(field.content().unwrap().1.contains(r"\log"));
        let before = field.root.clone();
        assert!(field.update("log(3,)").is_err());
        assert_eq!(
            field.root, before,
            "Invalid input preserves the formula draft"
        );
    }

    #[test]
    fn visual_functions_and_symbols_preserve_the_current_expression() {
        for name in [
            "log", "ln", "lg", "sin", "arcsin", "sinh", "abs", "norm", "vec", "dot", "ddot", "bar",
            "hat",
        ] {
            let mut field = Field::new("x", "x");
            field.insert(&format!("function:{name}")).unwrap();
            assert_eq!(field.selected().value, "x");
            assert_eq!(
                field.content().unwrap().1,
                expression(&format!("{name}(x)")).unwrap().tex()
            );
            field.undo(false);
            assert_eq!(field.content().unwrap(), ("x".into(), "x".into()));
        }
        let mut field = Field::new("x", "x");
        field.insert("symbol:in").unwrap();
        assert!(field.content().is_err());
        field.insert("symbol:reals").unwrap();
        assert_eq!(field.content().unwrap().1, expression("x∈ℝ").unwrap().tex());

        for name in [
            "epsilon",
            "varepsilon",
            "pi",
            "infty",
            "partial",
            "nabla",
            "reals",
            "integers",
            "naturals",
            "complexes",
        ] {
            let mut field = Field::new("", "");
            field.insert("math").unwrap();
            field.insert(&format!("symbol:{name}")).unwrap();
            assert!(field.content().is_ok(), "{name}");
        }
    }

    #[test]
    fn ordinary_questions_stay_plain_and_can_add_a_math_fragment() {
        for input in [
            "犹豫的英文？",
            "Ctrl+C",
            "len(items)",
            "Please log this error.",
            "It's fine.",
            "C:/notes/topic",
            "https://example.com",
        ] {
            let mut field = Field::new("", "");
            field.update(input).unwrap();
            assert!(field.root.is_text(), "{input}");
            assert_eq!(field.content().unwrap(), (input.into(), String::new()));
        }
        let mut field = Field::new("求 ", "");
        field.insert("math").unwrap();
        field.insert("function:log_base").unwrap();
        field.update("2").unwrap();
        field.select(vec![1, 1, 0]);
        field.update("8").unwrap();
        assert!(field.content().unwrap().1.contains(r"\text{求 }"));
        assert!(field.content().unwrap().1.contains(r"\log"));
    }
}
