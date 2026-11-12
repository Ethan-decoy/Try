//! The pinned editor is the authority for formula structure. Never accept a
//! display-parser conversion merely because both strings happen to parse.
use serde_json::{Value, json};
use slint_math_input::Editor;

pub(crate) fn is_latex(input: &str) -> bool {
    input.contains('\\') || input.contains("^{") || input.contains("_{")
}

pub(crate) fn complete(input: &str) -> bool {
    canonical(input).is_some_and(|latex| !latex.trim().is_empty())
}

pub(crate) fn canonical(input: &str) -> Option<String> {
    if input.chars().count() > 1000 {
        return None;
    }
    let mut editor = Editor::new();
    editor.import_latex(input).ok()?;
    editor.latex().ok()
}

pub(crate) fn same(left: &str, right: &str) -> bool {
    match (key(left), key(right)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// Use the editor's versioned public draft format, excluding document IDs and
/// cursor/layout state. Preserve unknown formula properties rather than erase
/// them: a newly supported structure must not become equal to an empty node.
pub(crate) fn key(latex: &str) -> Option<Value> {
    if latex.chars().count() > 1000 {
        return None;
    }
    let mut editor = Editor::new();
    editor.import_latex(latex).ok()?;
    editor.latex().ok()?;
    let draft: Value = serde_json::from_str(&editor.draft().ok()?).ok()?;
    if draft.get("version")?.as_u64()? != 1 {
        return None;
    }
    let mut root = draft.get("state")?.get("root")?.clone();
    normalize(&mut root);
    Some(root)
}

fn normalize(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("id");
            object.remove("ghost_open");
            for child in object.values_mut() {
                normalize(child);
            }
            if object.get("kind").and_then(Value::as_str) == Some("symbol")
                && object.get("text").and_then(Value::as_str) == Some("'")
            {
                object.insert("text".into(), json!("′"));
            }
            if let Some(Value::Array(nodes)) = object.get_mut("nodes") {
                let mut result: Vec<Value> = vec![];
                for node in std::mem::take(nodes) {
                    // u', u^{'} and u^{\prime} are the same derivative
                    // notation. Quotes inside a text node remain literal.
                    if is_prime(&node) && result.last().is_some_and(|n| !is_prime(n)) {
                        let mut base = result.pop().unwrap();
                        if base.get("kind").and_then(Value::as_str) == Some("script")
                            && base
                                .get("sup")
                                .and_then(|n| n.get("nodes"))
                                .and_then(Value::as_array)
                                .is_some_and(|nodes| {
                                    !nodes.is_empty() && nodes.iter().all(is_prime)
                                })
                        {
                            base["sup"]["nodes"].as_array_mut().unwrap().push(node);
                            result.push(base);
                        } else {
                            result.push(json!({"kind":"script", "base":{"nodes":[base]},
                                "sub":null, "sup":{"nodes":[node]}}));
                        }
                    } else {
                        result.push(node);
                    }
                }
                *nodes = result;
            }
        }
        Value::Array(array) => array.iter_mut().for_each(normalize),
        _ => {}
    }
}

fn is_prime(node: &Value) -> bool {
    node.get("kind").and_then(Value::as_str) == Some("symbol")
        && node.get("text").and_then(Value::as_str) == Some("′")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_keys_keep_structure_and_ignore_document_state() {
        assert!(same(r"u'f''(u)", r"u^{\prime}f^{''}(u)"));
        assert!(same(r"a^{u}\ln u", r"{a}^{u}\ln u"));
        for (left, right) in [
            (r"\binom{5}{2}", r"\left(\frac{5}{2}\right)"),
            (r"2^{3}!", r"2^{3!}"),
            (r"\prod_{i=1}^{n}i", r"\prod_{i=1}^{m}i"),
            (r"\text{u'}", r"u'"),
        ] {
            assert!(!same(left, right), "{left} != {right}");
        }
        assert!(!same(r"\unknown", r"\unknown"));
    }
}
