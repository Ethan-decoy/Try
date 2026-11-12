//! Friendly expression grammar and mixed-prose recognition.
use super::{Kind, MAX_DEPTH, MAX_INPUT, Node, node::is_primes};
use crate::{math_vocabulary as vocabulary, tr};

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
    Atom(Box<Node>),
    Sign(char),
    Arrow,
    End,
}

/// Reuse the expression grammar for inline division, with already parsed
/// structures as opaque operands. Serializing operands to text here would
/// move factorials into scripts or denominators and erase matrix styles.
pub(crate) fn inline_division(node: Node) -> Node {
    if node.kind != Kind::Row
        || !node.children.iter().any(|child| {
            child.kind == Kind::Literal && child.value == "/"
                || child.kind == Kind::Symbol && child.value == "div"
        })
    {
        return node;
    }
    let mut tokens = node
        .children
        .iter()
        .map(|child| {
            if child.kind == Kind::Literal
                && child.value.len() == 1
                && "+-*/=<>!".contains(&child.value)
            {
                Token::Sign(child.value.chars().next().unwrap())
            } else if child.kind == Kind::Symbol && child.value == "div" {
                Token::Sign('/')
            } else if child.kind == Kind::Symbol && matches!(child.value.as_str(), "times" | "cdot")
            {
                Token::Sign('*')
            } else {
                Token::Atom(Box::new(child.clone()))
            }
        })
        .collect::<Vec<_>>();
    tokens.push(Token::End);
    let mut parser = Parser {
        tokens,
        pos: 0,
        depth: 0,
        limit_condition: false,
    };
    match parser.expr(0) {
        Ok(parsed) if parser.peek() == &Token::End => parsed,
        _ => node,
    }
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
                        "cdot", "times", "div", "leq", "geq", "neq", "approx", "in", "notin",
                        "subseteq", "cup", "cap", "pm", "mp", "to",
                    ]
                    .contains(&vocabulary::symbol(s).unwrap_or("")) =>
                {
                    (
                        if ["cdot", "times", "div"].contains(&vocabulary::symbol(s).unwrap()) {
                            3
                        } else {
                            1
                        },
                        self.peek().clone(),
                        false,
                    )
                }
                Token::Word(_) | Token::Quoted(_) | Token::Atom(_) | Token::Sign('(' | '√') => {
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
                Token::Word(s) if matches!(vocabulary::symbol(&s), Some("cdot" | "times")) => {
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
            Token::Atom(node) => Ok(*node),
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
pub(super) fn applied_function(name: &str, body: Node) -> Node {
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

pub(super) fn logarithm(base: Node, body: Node) -> Node {
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
