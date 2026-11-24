//! Formula recognition and notation matching; no numeric approximation or algebraic evaluation.
use crate::math_editor::{self, Kind, Node};

pub fn recognize(input: &str, math_hint: bool) -> Option<Node> {
    let input = input.trim();
    if input.is_empty() || input.chars().count() > 1000 {
        return None;
    }
    let normalized = if crate::math_structure::is_latex(input) {
        input.to_owned()
    } else {
        input
            .replace('−', "-")
            .replace('²', "^2")
            .replace('³', "^3")
    };
    let input = normalized.as_str();
    let node = if crate::math_structure::is_latex(input) {
        math_editor::from_latex(input).ok()?
    } else if math_hint {
        compact_function(input)
            .map(Ok)
            .unwrap_or_else(|| notation_expression(input))
            .ok()?
    } else if input
        .chars()
        .all(|c| c.is_ascii_digit() || ".+- ".contains(c))
        || ["pi", "theta", "alpha", "beta", "gamma", "infinity"].contains(&input)
    {
        notation_expression(input).ok()?
    } else {
        let node = math_editor::automatic_content(input).ok()?;
        if node.is_text() {
            return None;
        }
        node
    };
    (node.complete() && node.validate().is_ok()).then_some(node)
}

/// Display inference is deliberately narrower than answer matching. A math
/// question can accept a variable, but must not italicize an ordinary word.
pub fn recognize_inline(input: &str, math_hint: bool) -> Option<Node> {
    let input = input.trim();
    if let Some(node) = recognize(input, false) {
        return Some(node);
    }
    if math_hint && let Some(node) = function_notation(input) {
        return Some(node);
    }
    let short_variables = input
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .all(|word| word.is_ascii() && word.len() <= 2);
    let expression = input.contains(['+', '-', '−', '*', '×'])
        && short_variables
        && !input.ends_with(['+', '-', '−', '*', '×', '/', '^', '_', '=']);
    let variable = math_hint && input.chars().count() == 1 && input.is_ascii();
    (expression || variable)
        .then(|| recognize(input, true))
        .flatten()
}

/// A partially typed formula stays a draft when Enter is pressed. Ordinary
/// words still go through answer comparison, even in a mathematical question.
pub fn incomplete_error(input: &str, math_hint: bool) -> Option<String> {
    let input = input.trim();
    if !math_hint
        || !(math_editor::looks_mathematical(input)
            || input.starts_with('\\')
            || input.contains(['+', '-', '−', '*', '×']))
    {
        return None;
    }
    let parsed = if crate::math_structure::is_latex(input) {
        math_editor::from_latex(input)
    } else {
        notation_expression(input)
    };
    match parsed {
        Err(error) => Some(error),
        Ok(node) if !node.complete() => Some(crate::tr!("请先填写完整的公式。").into()),
        Ok(node) => node.validate().err(),
    }
}

pub fn matches_formula(input: &str, latex: &str, aliases: &[String]) -> bool {
    let Some(expected) = recognize(latex, true) else {
        return false;
    };
    let alternatives = aliases.iter().filter_map(|alias| recognize(alias, false));
    let Some(supplied) = recognize(input, true) else {
        return false;
    };
    let supplied = notation(&supplied);
    std::iter::once(expected)
        .chain(alternatives)
        .any(|node| notation(&node) == supplied)
}

pub(crate) fn matches_edited_formula<'a>(
    latex: &str,
    reference: &str,
    allowed: impl Iterator<Item = &'a str>,
) -> bool {
    if latex.trim().is_empty() {
        return false;
    }
    let key = edited_notation(latex);
    if let Some(key) = &key
        && edited_notation(reference).as_ref() == Some(key)
    {
        return true;
    }
    allowed.map(str::trim).any(|spelling| {
        if key.is_some() && latex.trim() == spelling {
            return true;
        }
        match recognize_inline(spelling, true) {
            Some(alias) => key.as_ref().is_some_and(|key| {
                notation(&alias) == *key
                    || named_atomic_script(&alias) && crate::math_structure::same(latex, spelling)
            }),
            None => literal_editor_spelling(latex, spelling),
        }
    })
}

fn edited_notation(latex: &str) -> Option<String> {
    let node = math_editor::from_latex(latex).ok()?;
    (node.complete() && node.validate().is_ok()).then(|| notation(&node))
}

// A term such as sp^3 is also written as a flat LaTeX letter run with a
// one-character script. Compare that spelling only for a declared alias.
fn named_atomic_script(alias: &Node) -> bool {
    matches!(alias.kind, Kind::Power | Kind::Subscript | Kind::SubSup)
        && alias.children.first().is_some_and(|base| {
            base.kind == Kind::Literal
                && base.value.len() > 1
                && base.value.bytes().all(|c| c.is_ascii_alphabetic())
        })
        && alias.children.iter().skip(1).all(|script| {
            script.kind == Kind::Literal
                && script.value.len() == 1
                && script.value.bytes().all(|c| c.is_ascii_alphanumeric())
        })
}

pub(crate) fn matches_edited_literal(latex: &str, spelling: &str) -> bool {
    recognize_inline(spelling, true).is_none() && literal_editor_spelling(latex, spelling)
}

fn literal_editor_spelling(latex: &str, spelling: &str) -> bool {
    use slint_math_input::{Action, Editor};
    if spelling.is_empty() || latex.trim().is_empty() {
        return false;
    }
    if latex.trim() == spelling
        || math_editor::from_latex(latex).is_ok_and(|node| node.is_text() && node.value == spelling)
    {
        return true;
    }
    let mut editor = Editor::new();
    editor.structured_typing = true;
    // Complete text aliases may be displayed as something else by the
    // formula editor, e.g. transpose() becomes an upright function name.
    // Compare the exact preview, including closed delimiters. An empty pair
    // of parentheses in a literal spelling does not need a formula argument.
    if editor.apply(Action::Insert(spelling.into())).is_ok() && editor.latex_preview() == latex {
        return true;
    }
    editor.apply(Action::Clear).is_ok()
        && editor.paste(spelling).is_ok()
        && editor.latex_preview() == latex
}

fn notation_expression(input: &str) -> Result<Node, String> {
    math_editor::expression(input).or_else(|error| {
        // Lossless spellings of the SMI commands that the display parser does
        // not name. Ordinary text and unfinished arithmetic keep their parser.
        if input.contains(['°', '⊋', '⊎', '≔']) {
            math_editor::from_latex(input)
        } else {
            Err(error)
        }
    })
}

/// Whole native commits lack the command prefixes seen while typing. Import
/// compact calls through SMI, while limiting inference to a short coefficient
/// and argument so prose and source-code identifiers retain text editing.
fn compact_function(input: &str) -> Option<Node> {
    if input.len() > 32
        || !input
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'.')
    {
        return None;
    }
    let mut editor = slint_math_input::Editor::new();
    editor.paste(input).ok()?;
    let latex = editor.latex().ok()?;
    let node = math_editor::from_latex(&latex).ok()?;
    let call = match node.children.as_slice() {
        [function, argument] => is_function_name(function) && short_operand(argument),
        [coefficient, function, argument] => {
            short_operand(coefficient) && is_function_name(function) && short_operand(argument)
        }
        _ => false,
    };
    (node.kind == Kind::Row && call && node.complete() && node.validate().is_ok()).then_some(node)
}

fn function_notation(input: &str) -> Option<Node> {
    fn contains_function(node: &Node) -> bool {
        is_function_name(node)
            || node.kind == Kind::Root
            || matches!(&node.kind, Kind::Group(left, _) if left == "|" || left == r"\lVert")
            || node.children.iter().any(contains_function)
    }
    let node = recognize(input, true)?;
    contains_function(&node).then_some(node)
}

fn short_operand(node: &Node) -> bool {
    node.kind == Kind::Symbol && crate::math_vocabulary::symbol(&node.value).is_some()
        || node.kind == Kind::Literal
            && (node.value.parse::<f64>().is_ok()
                || node.value.len() <= 2 && node.value.bytes().all(|c| c.is_ascii_alphabetic())
                || crate::math_vocabulary::numeric_variable(&node.value))
}

fn notation(node: &Node) -> String {
    let tex = normalize(node.clone(), true).tex();
    crate::math_structure::key(&tex)
        .map(|mut key| {
            normalize_products(&mut key);
            key.to_string()
        })
        .unwrap_or(tex)
}

/// Answer comparison permits scalar multiplication with or without a dot.
/// Keep this separate from the lossless keys used by editing and rendering.
fn normalize_products(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                normalize_products(child);
            }
            if let Some(Value::Array(nodes)) = object.get_mut("nodes") {
                let mut factors: Vec<Value> = Vec::with_capacity(nodes.len());
                let mut previous_group = false;
                for node in nodes.iter() {
                    if !factors.last().is_some_and(function_token)
                        && let Some(body) = scalar_group(node)
                    {
                        // Parentheses delimit a factor. Keep that boundary
                        // when both sides are numeric, e.g. 2(3) versus 23.
                        if factors.last().is_some_and(numeric_symbol)
                            && body.first().is_some_and(numeric_symbol)
                        {
                            factors.push(serde_json::json!({"kind":"symbol", "text":"·"}));
                        }
                        factors.extend(body.iter().cloned());
                        previous_group = true;
                    } else {
                        if previous_group
                            && factors.last().is_some_and(numeric_symbol)
                            && numeric_symbol(node)
                        {
                            factors.push(serde_json::json!({"kind":"symbol", "text":"·"}));
                        }
                        factors.push(node.clone());
                        previous_group = false;
                    }
                }
                *nodes = factors;
                let mut result = Vec::with_capacity(nodes.len());
                for (index, node) in nodes.iter().enumerate() {
                    let dot = node.get("kind").and_then(Value::as_str) == Some("symbol")
                        && matches!(node.get("text").and_then(Value::as_str), Some("·" | "⋅"));
                    if dot
                        && let (Some(left), Some(right)) =
                            (index.checked_sub(1).and_then(|i| nodes.get(i)), nodes.get(index + 1))
                        && product_operand(left, false)
                        && product_operand(right, true)
                        // SMI stores digits separately: erasing this dot would
                        // merge two numbers, e.g. 2*3 into the number 23.
                        && !(numeric_symbol(left) && numeric_symbol(right))
                    {
                        continue;
                    }
                    result.push(node.clone());
                }
                *nodes = result;
            }
        }
        Value::Array(array) => array.iter_mut().for_each(normalize_products),
        _ => {}
    }
}

fn scalar_group(node: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    use serde_json::Value;
    if node.get("kind").and_then(Value::as_str) != Some("group")
        || node.get("open").and_then(Value::as_str) != Some("(")
        || node.get("close").and_then(Value::as_str) != Some(")")
        || node.get("closed").and_then(Value::as_bool) != Some(true)
    {
        return None;
    }
    let body = node.get("body")?.get("nodes")?.as_array()?;
    let atomic = body.len() == 1 && product_operand(&body[0], false) && !numeric_symbol(&body[0])
        || body.len() == 2
            && function_token(&body[0])
            && body[1].get("kind").and_then(Value::as_str) == Some("group");
    let number = !body.is_empty()
        && body.iter().all(numeric_symbol)
        && body
            .iter()
            .filter_map(|node| node.get("text")?.as_str())
            .collect::<String>()
            .parse::<f64>()
            .is_ok();
    (atomic || number).then_some(body)
}

fn function_token(node: &serde_json::Value) -> bool {
    use serde_json::Value;
    node.get("kind").and_then(Value::as_str) == Some("function")
        || node.get("kind").and_then(Value::as_str) == Some("script")
            && node
                .get("base")
                .and_then(|base| base.get("nodes"))
                .and_then(Value::as_array)
                .is_some_and(|nodes| nodes.len() == 1 && function_token(&nodes[0]))
}

fn numeric_symbol(node: &serde_json::Value) -> bool {
    node.get("kind").and_then(serde_json::Value::as_str) == Some("symbol")
        && node
            .get("text")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|text| {
                !text.is_empty() && text.chars().all(|c| c.is_ascii_digit() || c == '.')
            })
}

fn product_operand(node: &serde_json::Value, start: bool) -> bool {
    match node.get("kind").and_then(serde_json::Value::as_str) {
        Some("symbol") => node
            .get("text")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|text| {
                !text.is_empty() && text.chars().all(|c| c.is_alphanumeric() || c == '.')
            }),
        Some("function") => start,
        Some("group" | "script" | "fraction" | "root") => true,
        _ => false,
    }
}

fn normalize(mut node: Node, grouped_slot: bool) -> Node {
    // Some math keyboards export a multiplication dot in a text wrapper.
    // Normalize that single glyph without rewriting other quoted content.
    if node.kind == Kind::Text && matches!(node.value.as_str(), "⋅" | "·") {
        return Node::leaf(Kind::Symbol, "cdot");
    }
    if node.kind == Kind::Symbol && node.value == "times" {
        node.value = "cdot".into();
    }
    if node.kind == Kind::Literal && node.value == "−" {
        node.value = "-".into();
    }
    if grouped_slot && matches!(&node.kind, Kind::Group(left, right) if left == "(" && right == ")")
    {
        return normalize(node.children.remove(0), true);
    }
    let is_row = node.kind == Kind::Row;
    node.children.retain(|child| child.kind != Kind::Space);
    if is_row {
        let mut children = vec![];
        let mut source = node.children.into_iter().peekable();
        while let Some(mut child) = source.next() {
            if child.kind == Kind::Operator
                && child.value == "abs"
                && source.peek().is_some_and(is_parentheses)
            {
                child = source.next().unwrap();
                child.kind = Kind::Group("|".into(), "|".into());
            }
            if child.kind == Kind::Literal {
                let exponent = match child.value.as_str() {
                    "°" => Some(Node::leaf(Kind::Symbol, "circ")),
                    "²" => Some(Node::leaf(Kind::Literal, "2")),
                    "³" => Some(Node::leaf(Kind::Literal, "3")),
                    _ => None,
                };
                if let Some(exponent) = exponent
                    && let Some(base) = children.pop()
                {
                    child = Node {
                        kind: Kind::Power,
                        value: String::new(),
                        children: vec![base, exponent],
                    };
                }
            }
            // sin²(x) and (sin(x))² are the same function notation. Keep
            // algebraic simplification and inverse-function inference out.
            if children.last().is_some_and(is_function_name)
                && child.kind == Kind::Power
                && is_parentheses(&child.children[0])
            {
                // In \ln(x)^2 the power applies to the call. Bind it before
                // simplifying structural-slot parentheses around its base.
                let function = children.pop().unwrap();
                let argument = child.children.remove(0);
                child.children.insert(
                    0,
                    Node {
                        kind: Kind::Row,
                        value: String::new(),
                        children: vec![function, argument],
                    },
                );
            } else if child.kind == Kind::Power
                && is_function_name(&child.children[0])
                && source
                    .peek()
                    .is_some_and(|next| is_parentheses(next) || function_argument(next))
            {
                let mut body = source.next().unwrap();
                if !is_parentheses(&body) {
                    body = Node {
                        kind: Kind::Group("(".into(), ")".into()),
                        value: String::new(),
                        children: vec![body],
                    };
                }
                let base = child.children.remove(0);
                let call = Node {
                    kind: Kind::Row,
                    value: String::new(),
                    children: vec![base, body],
                };
                child.children.insert(0, call);
            } else if children.last().is_some_and(is_function_name)
                && !is_parentheses(&child)
                && function_argument(&child)
            {
                // Canonicalize the argument boundary, rather than removing
                // it: ln(xy) must stay distinct from ln(x)y.
                child = Node {
                    kind: Kind::Group("(".into(), ")".into()),
                    value: String::new(),
                    children: vec![child],
                };
            }
            children.push(child);
        }
        node.children = children;
    }
    node.children = node
        .children
        .into_iter()
        .map(|child| normalize(child, !is_row))
        .collect();
    if is_row && node.children.len() == 1 {
        return normalize(node.children.remove(0), grouped_slot);
    }
    math_editor::inline_division(node)
}

fn is_parentheses(node: &Node) -> bool {
    matches!(&node.kind, Kind::Group(left, right) if left == "(" && right == ")")
}

fn is_function_name(node: &Node) -> bool {
    node.kind == Kind::Operator && crate::math_vocabulary::is_operator(&node.value)
        || node.kind == Kind::Subscript && is_function_name(&node.children[0])
}

fn function_argument(node: &Node) -> bool {
    match node.kind {
        Kind::Literal => !node.value.is_empty() && node.value.chars().all(char::is_alphanumeric),
        Kind::Symbol => !matches!(
            node.value.as_str(),
            "cdot"
                | "times"
                | "div"
                | "leq"
                | "geq"
                | "neq"
                | "approx"
                | "in"
                | "notin"
                | "subseteq"
                | "cup"
                | "cap"
                | "pm"
                | "mp"
                | "to"
                | "prime"
                | "circ"
        ),
        Kind::Fraction | Kind::Root | Kind::Power | Kind::Subscript => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edited_answers_preserve_declared_aliases_and_actual_script_scope() {
        for (input, reference, alias, expected) in [
            (
                r"\operatorname{transpose}\left(\right)",
                "x",
                "transpose()",
                true,
            ),
            (
                r"\operatorname{transpose}\left(\right.",
                "x",
                "transpose()",
                false,
            ),
            (r"\frac{u}{二}", r"\frac{u}{2}", "u/二", true),
            (r"sp^{3}", r"\mathrm{sp}^{3}", "sp^3", true),
            (r"sp^{2}", r"\mathrm{sp}^{3}", "sp^3", false),
            (
                r"\ln \left(\operatorname{abs}\left(x\right)\right)+C",
                r"\ln\left|x\right|+C",
                "ln(abs(x))+C",
                true,
            ),
            (r"u'v+uv'", r"u'v+uv'", "u'v+uv'", true),
            (r"u'\cdot v+u\cdot v'", r"u'v+uv'", "u'*v+u*v'", true),
            (
                r"\cos ²\left(x\right)-\sin ²\left(x\right)",
                r"\cos^{2}(x)-\sin^{2}(x)",
                "cos²(x) − sin²(x)",
                true,
            ),
            (r"e^{x}+c", r"e^{x}+C", "e^x+c", true),
            (r"e^{x+c}", r"e^{x}+C", "e^x+c", false),
            (r"\frac{u}{}", r"\frac{u}{2}", "u/2", false),
        ] {
            assert_eq!(
                matches_edited_formula(input, reference, [alias].into_iter()),
                expected,
                "grading {input} against {reference} and {alias}"
            );
        }
    }

    #[test]
    fn notation_entry_recognizes_compact_functions_in_math_context() {
        for (input, latex) in [
            ("lnx", r"\ln(x)"),
            ("sinx", r"\sin(x)"),
            ("ylnx", r"y\ln(x)"),
            ("ysqrt(x)", r"y\sqrt{x}"),
            ("yabs(x)", r"y\left|x\right|"),
            ("ynorm(x)", r"y\left\lVert x\right\rVert"),
            ("sin^2x", r"\sin^{2}(x)"),
            ("abcln(x)", r"abc\ln(x)"),
        ] {
            assert!(matches_formula(input, latex, &[]), "{input}: {latex}");
            assert!(recognize_inline(input, true).is_some(), "inline: {input}");
        }
        for text in [
            "Paris",
            "println(x)",
            "sinister(x)",
            "catalog(x)",
            "2px",
            "12ms",
            "3eV",
            "C++",
            "Vec<T>",
            "1 海里",
        ] {
            assert!(recognize_inline(text, false).is_none(), "text: {text}");
        }
        for text in [
            "lnx",
            "sinx",
            "ylnx",
            "Paris",
            "sinister",
            "println(x)",
            "abcln(x)",
        ] {
            assert!(
                recognize_inline(text, false).is_none(),
                "context required: {text}"
            );
        }
    }

    #[test]
    fn notation_scalar_groups_preserve_function_and_number_boundaries() {
        for (input, latex) in [
            ("2*(x)", "2x"),
            ("2(x)", "2x"),
            ("(x)y", "xy"),
            ("y*(x/2)", r"y\frac{x}{2}"),
            ("(x^2)y", "x^{2}y"),
            ("(2)(x)", "2x"),
            ("2(3)", r"2\cdot3"),
            ("(2)(3)", r"2\cdot3"),
            ("(23)(45)", r"23\cdot45"),
            ("2*(ln(x))", r"2\ln(x)"),
        ] {
            assert!(matches_formula(input, latex, &[]), "{input}: {latex}");
        }
        for (input, wrong) in [
            ("2(3)", "23"),
            ("(2)3", "23"),
            ("1(.2)", "1.2"),
            ("ln(xy)", r"\ln(x)y"),
            ("ln(23)", r"\ln(2)3"),
            ("ln(x^2)", r"\ln(x)^2"),
            ("(x+y)*z", "x+y*z"),
            ("x^(n+m)", "x^{n}+m"),
            ("(x_i)^2", "x_{i^{2}}"),
            ("2*(ln(xy))", r"2\ln(x)y"),
        ] {
            assert!(
                !matches_formula(input, wrong, &[]),
                "scope: {input} == {wrong}"
            );
        }
    }

    #[test]
    fn coefficient_notation_is_equivalent_only_for_formula_answers() {
        for (answer, input) in [
            ("2x", "2*x"),
            ("2x", "2·x"),
            ("2x", "2(x)"),
            ("-2x", "-2*x"),
            ("0.5x", "0.5*x"),
        ] {
            for latex in [None, Some(answer)] {
                let card: crate::knowledge::Card = serde_json::from_value(serde_json::json!({
                    "prompt":"Derivative formula", "answer":answer, "answer_latex":latex
                }))
                .unwrap();
                assert!(card.accepts(answer), "stored spelling: {answer}");
                assert_eq!(
                    card.accepts(input),
                    latex.is_some(),
                    "{input}, answer={answer}, latex={latex:?}"
                );
                for wrong in ["3*x", "2+x", "2*x^2"] {
                    assert!(!card.accepts(wrong), "wrong: {wrong}");
                }
            }
        }
    }

    #[test]
    fn adjacent_function_answers_accept_coefficients_and_multiplication_notation() {
        let card: crate::knowledge::Card = serde_json::from_str(
            r#"{
            "prompt":"Simplify ln(x^y) for x > 0",
            "answer":"y ln(x)",
            "answer_latex":"y\\ln\\left(x\\right)"
        }"#,
        )
        .unwrap();
        for input in ["yln(x)", "y ln(x)", "y*ln(x)", "y⋅ln(x)", r"y\cdot\ln(x)"] {
            assert!(
                card.accepts(input),
                "the same logarithm product was rejected: {input}"
            );
        }
        for input in ["ln(x)", "y+ln(x)", "yln(x+1)", "yln(x)^2"] {
            assert!(
                !card.accepts(input),
                "different formula was accepted: {input}"
            );
        }
        for (input, expected) in [
            ("xsin(t)", r"x\sin(t)"),
            ("xyln(x)", r"xy\ln(x)"),
            ("2yln(x)", r"2y\ln(x)"),
            ("xarcsin(t)", r"x\arcsin(t)"),
            ("xasin(t)", r"x\arcsin(t)"),
            ("yLog_2(x)", r"y\log_{2}(x)"),
        ] {
            assert!(matches_formula(input, expected, &[]), "{input}: {expected}");
        }
    }

    #[test]
    fn adjacent_function_entry_preserves_prose_and_reserved_names() {
        assert!(recognize_inline("yln(x)", true).is_some());
        for word in ["println(x)", "sinister(x)", "catalog(x)", "hello world"] {
            assert!(
                recognize_inline(word, false).is_none(),
                "plain/code: {word}"
            );
        }
        assert!(matches_formula("asin(x)", r"\arcsin(x)", &[]));
        assert!(!matches_formula("asin(x)", r"a\sin(x)", &[]));
        assert!(!matches_formula("asinh(x)", r"a\sinh(x)", &[]));
        assert!(!matches_formula(r#""yln(x)""#, r"y\ln(x)", &[]));
    }

    #[test]
    fn product_notation_keeps_numbers_and_function_arguments_distinct() {
        for (input, expected) in [
            ("x*y", "xy"),
            ("2*sin(x)", r"2\sin(x)"),
            ("x*sin(y)", r"x\sin(y)"),
            ("ln(x)*y", r"\ln(x)y"),
            ("x/(y*z)", r"\frac{x}{yz}"),
        ] {
            assert!(matches_formula(input, expected, &[]), "{input}: {expected}");
        }
        for (input, wrong) in [
            ("2*3", "23"),
            ("1.2*3", "1.23"),
            ("x*2*3", "x23"),
            ("x*y", "yx"),
            ("yln(xy)", r"y\ln(x)y"),
            ("yln(x+y)", r"y\ln(x)+y"),
            ("sin(xy)", r"\sin(x)y"),
            ("ln(23)", r"\ln(2)3"),
            ("ln(x^2)", r"\ln(x)^2"),
            ("x/(y*z)", r"\frac{x}{y}z"),
        ] {
            assert!(
                !matches_formula(input, wrong, &[]),
                "different product/argument: {input} == {wrong}"
            );
        }
    }

    #[test]
    fn bridge_preserves_distinct_formula_meanings() {
        for (expected, wrong) in [
            (r"\binom{5}{2}", r"\frac{5}{2}"),
            (r"\frac{6}{3}!", r"\frac{6}{3!}"),
            (r"2^{3}!", r"2^{3!}"),
            (r"\prod_{i=1}^{n}i", r"\prod_{i=1}^{m}i"),
            (r"\prod_{i=1}^{n}i", r"\overline{x}"),
            (
                r"\begin{vmatrix}a&b\\c&d\end{vmatrix}",
                r"\begin{bmatrix}a&b\\c&d\end{bmatrix}",
            ),
        ] {
            assert!(matches_formula(expected, expected, &[]), "self: {expected}");
            assert!(
                !matches_formula(wrong, expected, &[]),
                "{expected} != {wrong}"
            );
        }
    }

    #[test]
    fn code_identifiers_stay_literal_during_typing_and_authoring() {
        for input in [
            "std::unique_ptr",
            "unique_ptr",
            "std::make_unique<int>",
            ".as_ref()",
            "map_err",
        ] {
            assert!(recognize_inline(input, false).is_none(), "{input}");
            let mut field = math_editor::Field::new("", "");
            field.update(input).unwrap();
            assert_eq!(field.content().unwrap(), (input.into(), String::new()));
        }
        for (input, latex) in [
            ("x_i", r"x_{i}"),
            ("theta_i", r"\theta_{i}"),
            ("log_2(x)", r"\log_{2}(x)"),
        ] {
            assert!(recognize_inline(input, false).is_some(), "{input}");
            assert!(matches_formula(input, latex, &[]), "{input}");
        }
    }

    #[test]
    fn inline_recognition_preserves_prose_and_unfinished_drafts() {
        for text in [
            "Paris",
            "hello world",
            "北京",
            "well-being",
            "sqrt(",
            r"\frac{1}{",
        ] {
            assert!(recognize_inline(text, true).is_none(), "{text}");
        }
        for formula in ["1/2", "sqrt(3)/2", "x+y", "x-y", "x^2", "π", r"\frac{1}{2}"] {
            assert!(recognize_inline(formula, false).is_some(), "{formula}");
        }
        assert!(recognize_inline("x", true).is_some());
        assert!(recognize_inline("x", false).is_none());
    }

    #[test]
    fn incomplete_math_is_retained_without_blocking_plain_text_answers() {
        for input in [
            "sqrt(",
            "1/",
            "x^",
            r"\frac{1}{",
            r"\frac{}{2}",
            r"\prod_{i=1}^{}i",
            r"\overline{}",
            r"\binom{5}{}",
        ] {
            assert!(incomplete_error(input, true).is_some(), "{input}");
        }
        for input in ["1/2", "sqrt(3)/2", "Paris", "北京"] {
            assert!(incomplete_error(input, true).is_none(), "{input}");
        }
        assert!(incomplete_error("literal/", false).is_none());
        assert!(incomplete_error("p^n⋅", true).is_some());
    }

    #[test]
    fn structured_multiplication_survives_submission_and_notation_matching() {
        let forms = [
            r"p^{n}\text{⋅}q^{n}",
            r"p^{n}\text{·}q^{n}",
            r"p^{n}\cdot q^{n}",
            r"p^{n}\times q^{n}",
        ];
        for latex in forms {
            let plain = math_editor::from_latex(latex).unwrap().plain();
            assert!(
                incomplete_error(&plain, true).is_none(),
                "complete structured answer must reach feedback: {latex} -> {plain}"
            );
            for expected in forms {
                assert!(
                    matches_formula(&plain, expected, &[]),
                    "{plain}: {expected}"
                );
                assert!(matches_formula(latex, expected, &[]), "{latex}: {expected}");
            }
            assert!(!matches_formula("p^m*q^n", latex, &[]));
        }
    }

    #[test]
    fn primed_structured_answers_can_submit_and_match_derivative_notation() {
        let forms = [
            r"u^{'}f^{'}\left(u\right)",
            r"u^{\prime}f^{\prime}\left(u\right)",
            "u'f'(u)",
        ];
        for latex in forms {
            // The practice card writes the completed SMI expression back as
            // plain notation before answer comparison and incomplete checks.
            let plain = math_editor::from_latex(latex).unwrap().plain();
            assert!(
                incomplete_error(&plain, true).is_none(),
                "complete answer must reach feedback: {latex} -> {plain}"
            );
            for expected in forms {
                assert!(
                    matches_formula(&plain, expected, &[]),
                    "{plain}: {expected}"
                );
                assert!(matches_formula(latex, expected, &[]), "{latex}: {expected}");
            }
            assert!(!matches_formula("u*f'(u)", latex, &[]));
            assert!(!matches_formula("u'*f(u)", latex, &[]));
            assert!(!matches_formula("u'*f''(u)", latex, &[]));
        }
    }

    #[test]
    fn outer_factors_are_not_matched_as_part_of_a_script_scope() {
        for (correct, wrong) in [
            (r"a^{u}\ln u", r"a^{ulnu}"),
            (r"a^{u}\ln u", r"a^{u\ln u}"),
            (r"u'a^{u}\ln u", r"u'a^{u\ln u}"),
            (r"a^{u}b", r"a^{ub}"),
            (r"x_{i}y", r"x_{iy}"),
        ] {
            let supplied = math_editor::from_latex(correct).unwrap().plain();
            assert!(incomplete_error(&supplied, true).is_none(), "{supplied}");
            assert!(matches_formula(&supplied, correct, &[]), "{correct}");
            assert!(
                !matches_formula(&supplied, wrong, &[]),
                "{correct} != {wrong}"
            );
            assert!(
                !matches_formula(wrong, correct, &[]),
                "{wrong} != {correct}"
            );
        }
    }

    #[test]
    fn friendly_function_spellings_match_generated_formulas() {
        for (input, latex) in [
            ("log(2,x)", r"\log_{2}(x)"),
            ("Log_2(x)", r"\log_{2}x"),
            ("asin(x)", r"\arcsin(x)"),
            ("sin²(x)", r"(\sin(x))^{2}"),
            ("sin(x)^2", r"\sin^{2}(x)"),
            ("sin^2(x+y)", r"(\sin(x+y))^{2}"),
            ("f'(x)", r"f^{\prime}(x)"),
            ("|x|", r"\left|x\right|"),
            ("ε", r"\varepsilon"),
        ] {
            assert!(matches_formula(input, latex, &[]), "{input}: {latex}");
        }
        assert!(!matches_formula("sin^2(x)", r"\cos^2(x)", &[]));
        assert!(!matches_formula("sin^-1(x)", r"\arcsin(x)", &[]));
        assert!(!matches_formula("sin(x)+y", r"\sin(x+y)", &[]));
        assert!(!matches_formula("sin^2(x+y)", r"(\sin(x)+y)^2", &[]));
    }

    #[test]
    fn absolute_values_and_norms_keep_their_meaning_in_structural_slots() {
        for (input, latex, wrong) in [
            ("1/|x|", r"\frac{1}{\left|x\right|}", "1/x"),
            ("|x|/y", r"\frac{\left|x\right|}{y}", "x/y"),
            ("1/norm(x)", r"\frac{1}{\left\lVert x\right\rVert}", "1/x"),
            ("√|x|", r"\sqrt{\left|x\right|}", "sqrt(x)"),
            ("a^abs(x)", r"a^{\left|x\right|}", "a^x"),
            ("a_abs(x)", r"a_{\left|x\right|}", "a_x"),
        ] {
            assert!(matches_formula(input, latex, &[]), "{input}");
            assert!(!matches_formula(wrong, latex, &[]), "{wrong}: {latex}");
        }
        assert!(matches_formula("1/(x+y)", r"\frac{1}{x+y}", &[]));
        assert!(matches_formula("x^(n+1)", "x^{n+1}", &[]));
    }

    #[test]
    fn common_notations_match_the_same_fraction_and_root() {
        for input in [
            "sqrt(3)/2",
            "√3/2",
            " sqrt(3) / 2 ",
            r"\sqrt{3}/2",
            r"\frac{\sqrt{3}}{2}",
        ] {
            assert!(
                matches_formula(input, r"\frac{\sqrt{3}}{2}", &[]),
                "{input}"
            );
        }
        assert!(matches_formula("pi / 3", r"\frac{\pi}{3}", &[]));
        assert!(matches_formula("x^2", "x^{2}", &[]));
    }

    #[test]
    fn grouping_wrong_values_and_approximation_are_not_silently_changed() {
        assert!(!matches_formula("(a+b)*c", "a+b*c", &[]));
        assert!(!matches_formula("sqrt(2)/2", r"\frac{\sqrt{3}}{2}", &[]));
        assert!(!matches_formula("0.5", r"\frac{1}{2}", &[]));
        assert!(!matches_formula("sqrt(", r"\frac{\sqrt{3}}{2}", &[]));
        assert!(recognize("hesitate", false).is_none());
        assert!(recognize(&"1".repeat(1001), true).is_none());
    }

    #[test]
    fn component_symbol_spellings_match_the_same_answer_and_keep_wrong_answers_wrong() {
        for (input, latex, wrong) in [
            ("30°", r"30\degree", "31°"),
            ("A⊋B", r"A\supsetneq B", "A⊋C"),
            ("A⊎B", r"A\uplus B", "A⊎C"),
            ("a≔b", r"a\coloneqq b", "a≔c"),
            (
                r"\operatorname{hom}(x)",
                r"\hom(x)",
                r"\operatorname{hom}(y)",
            ),
        ] {
            assert!(matches_formula(input, latex, &[]), "{input}: {latex}");
            assert!(!matches_formula(wrong, latex, &[]), "{wrong}: {latex}");
            assert!(incomplete_error(input, true).is_none(), "{input}");
        }
    }
}
