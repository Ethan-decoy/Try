//! Formula recognition and notation matching; no numeric approximation or algebraic evaluation.
use crate::math_editor::{self, Kind, Node};

pub fn recognize(input: &str, math_hint: bool) -> Option<Node> {
    let input = input.trim();
    if input.is_empty() || input.chars().count() > 1000 {
        return None;
    }
    let normalized = if input.contains('\\') {
        input.to_owned()
    } else {
        input
            .replace('−', "-")
            .replace('²', "^2")
            .replace('³', "^3")
    };
    let input = normalized.as_str();
    let node = if input.contains('\\') {
        math_editor::from_latex(input).ok()?
    } else if math_hint
        || input
            .chars()
            .all(|c| c.is_ascii_digit() || ".+- ".contains(c))
        || ["pi", "theta", "alpha", "beta", "gamma", "infinity"].contains(&input)
    {
        math_editor::expression(input).ok()?
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
    let short_variables = input
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .all(|word| word.is_ascii() && word.len() <= 2);
    let expression = input.contains(['+', '-', '−', '*', '×']) && short_variables;
    let variable = math_hint && input.chars().count() == 1 && input.is_ascii();
    (expression || variable)
        .then(|| recognize(input, true))
        .flatten()
}

/// Start structural editing as soon as notation needs it. Plain words and
/// source-code answers retain native text/IME editing rather than a math overlay.
pub fn starts_structured_edit(input: &str, math_hint: bool) -> bool {
    // Keep a partly written function call native until its parentheses close.
    // Importing its complete expression avoids replaying written parentheses
    // as an extra group inside the editor's automatic function shortcut.
    if !input.contains('\\') && input.contains('(') && recognize_inline(input, math_hint).is_none()
    {
        return false;
    }
    // A trailing operator without mathematical context is ambiguous (C++,
    // command flags, word fragments). Wait for a complete expression instead.
    if !math_hint
        && input
            .trim_end()
            .ends_with(['+', '-', '*', '×', '/', '^', '_', '='])
    {
        return false;
    }
    if input.chars().any(|c| {
        matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}')
    }) {
        return false;
    }
    let notation = math_editor::looks_mathematical(input)
        || input.starts_with('\\')
        || input.contains(['+', '-', '−', '*', '×']);
    notation && (math_hint || recognize_inline(input, false).is_some() || input.starts_with('\\'))
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
    let parsed = if input.contains('\\') {
        math_editor::from_latex(input)
    } else {
        math_editor::expression(input)
    };
    match parsed {
        Err(error) => Some(error),
        Ok(node) if !node.complete() => Some(crate::tr!("请先填写完整的公式。").into()),
        Ok(node) => node.validate().err(),
    }
}

pub fn matches(input: &str, answer: &str, latex: Option<&str>, aliases: &[String]) -> bool {
    let expected = latex
        .and_then(|tex| recognize(tex, true))
        .or_else(|| recognize(answer, false));
    let alternatives = aliases.iter().filter_map(|alias| recognize(alias, false));
    let Some(supplied) = recognize(input, expected.is_some()) else {
        return false;
    };
    let supplied = notation(&supplied);
    expected
        .into_iter()
        .chain(alternatives)
        .any(|node| notation(&node) == supplied)
}

fn notation(node: &Node) -> String {
    // Reparse friendly notation so inline LaTeX slashes and stacked fractions
    // share the same tree. Parentheses inside a product/sum remain significant.
    let friendly = node.editor_input().replace('−', "-");
    let node = math_editor::expression(&friendly).unwrap_or_else(|_| node.clone());
    normalize(node, true).tex()
}

fn normalize(mut node: Node, grouped_slot: bool) -> Node {
    // Some math keyboards export a multiplication dot in a text wrapper.
    // Normalize that single glyph without rewriting other quoted content.
    if node.kind == Kind::Text && matches!(node.value.as_str(), "⋅" | "·") {
        return Node::leaf(Kind::Symbol, "cdot");
    }
    if grouped_slot && matches!(&node.kind, Kind::Group(left, right) if left == "(" && right == ")")
    {
        return normalize(node.children.remove(0), true);
    }
    let is_row = node.kind == Kind::Row;
    node.children = node
        .children
        .into_iter()
        .filter(|child| child.kind != Kind::Space)
        .map(|child| normalize(child, !is_row))
        .collect();
    if is_row {
        let mut children = vec![];
        let mut source = node.children.into_iter().peekable();
        while let Some(mut child) = source.next() {
            // sin²(x) and (sin(x))² are the same function notation. Keep
            // algebraic simplification and inverse-function inference out.
            if child.kind == Kind::Power
                && is_function_name(&child.children[0])
                && source.peek().is_some_and(is_parentheses)
            {
                let body = source.next().unwrap();
                let base = child.children.remove(0);
                let argument = if body.children[0].kind == Kind::Row {
                    body
                } else {
                    normalize(body, true)
                };
                let call = Node {
                    kind: Kind::Row,
                    value: String::new(),
                    children: vec![base, argument],
                };
                child.children.insert(0, call);
            } else if children.last().is_some_and(is_function_name)
                && is_parentheses(&child)
                && child.children[0].kind != Kind::Row
            {
                child = normalize(child, true);
            }
            children.push(child);
        }
        node.children = children;
    }
    if is_row && node.children.len() == 1 {
        return normalize(node.children.remove(0), grouped_slot);
    }
    node
}

fn is_parentheses(node: &Node) -> bool {
    matches!(&node.kind, Kind::Group(left, right) if left == "(" && right == ")")
}

fn is_function_name(node: &Node) -> bool {
    node.kind == Kind::Operator && crate::math_vocabulary::is_operator(&node.value)
        || node.kind == Kind::Subscript && is_function_name(&node.children[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_editing_starts_without_a_keypad_and_preserves_plain_answers() {
        for text in [
            "Paris",
            "犹豫",
            "1 海里",
            "well-being",
            "&mut T",
            "C++",
            "Vec<T>",
            "123",
        ] {
            assert!(!starts_structured_edit(text, false), "{text}");
        }
        for text in ["1/2", "sqrt(3)/2", "x+y", "x^2", r"\frac{1}{2}"] {
            assert!(starts_structured_edit(text, false), "{text}");
        }
        for text in ["sqrt(", "sqrt(3", "sin("] {
            assert!(!starts_structured_edit(text, true), "{text}");
        }
        for text in ["1/", "x^", r"\frac{1}{"] {
            assert!(starts_structured_edit(text, true), "{text}");
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
        for input in ["sqrt(", "1/", "x^", r"\frac{1}{", r"\frac{}{2}"] {
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
                    matches(&plain, "", Some(expected), &[]),
                    "{plain}: {expected}"
                );
                assert!(
                    matches(latex, "", Some(expected), &[]),
                    "{latex}: {expected}"
                );
            }
            assert!(!matches("p^m*q^n", "", Some(latex), &[]));
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
            assert!(matches(input, input, Some(latex), &[]), "{input}: {latex}");
        }
        assert!(!matches("sin^2(x)", "cos^2(x)", Some(r"\cos^2(x)"), &[]));
        assert!(!matches("sin^-1(x)", "arcsin(x)", Some(r"\arcsin(x)"), &[]));
        assert!(!matches("sin(x)+y", "sin(x+y)", Some(r"\sin(x+y)"), &[]));
        assert!(!matches(
            "sin^2(x+y)",
            "(sin(x)+y)^2",
            Some(r"(\sin(x)+y)^2"),
            &[]
        ));
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
            assert!(matches(input, input, Some(latex), &[]), "{input}");
            assert!(!matches(wrong, input, Some(latex), &[]), "{wrong}: {latex}");
        }
        assert!(matches("1/(x+y)", "1/(x+y)", Some(r"\frac{1}{x+y}"), &[]));
        assert!(matches("x^(n+1)", "x^(n+1)", Some("x^{n+1}"), &[]));
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
                matches(input, "√3/2", Some(r"\frac{\sqrt{3}}{2}"), &[]),
                "{input}"
            );
        }
        assert!(matches("pi / 3", "π/3", Some(r"\frac{\pi}{3}"), &[]));
        assert!(matches("x^2", "x²", Some("x^{2}"), &[]));
    }

    #[test]
    fn grouping_wrong_values_and_approximation_are_not_silently_changed() {
        assert!(!matches("(a+b)*c", "a+b*c", Some("a+b*c"), &[]));
        assert!(!matches(
            "sqrt(2)/2",
            "√3/2",
            Some(r"\frac{\sqrt{3}}{2}"),
            &[]
        ));
        assert!(!matches("0.5", "1/2", Some(r"\frac{1}{2}"), &[]));
        assert!(!matches("sqrt(", "√3/2", None, &[]));
        assert!(recognize("hesitate", false).is_none());
        assert!(recognize(&"1".repeat(1001), true).is_none());
    }
}
