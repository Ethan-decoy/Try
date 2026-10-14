//! Friendly spellings used by both typed formulas and visual templates.

pub(crate) fn function(name: &str) -> Option<&'static str> {
    Some(match name {
        "log" | "Log" => "log",
        "ln" => "ln",
        "lg" => "lg",
        "exp" => "exp",
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "cot" => "cot",
        "sec" => "sec",
        "csc" => "csc",
        "asin" | "arcsin" => "arcsin",
        "acos" | "arccos" => "arccos",
        "atan" | "arctan" => "arctan",
        "sinh" => "sinh",
        "cosh" => "cosh",
        "tanh" => "tanh",
        "abs" => "abs",
        "norm" => "norm",
        "sqrt" => "sqrt",
        "root" => "root",
        "sum" => "sum",
        "int" => "int",
        "lim" => "lim",
        "cases" => "cases",
        "vec" => "vec",
        "dot" => "dot",
        "ddot" => "ddot",
        "bar" => "bar",
        "hat" => "hat",
        _ => return None,
    })
}

pub(crate) fn is_operator(name: &str) -> bool {
    matches!(
        name,
        "log"
            | "ln"
            | "lg"
            | "exp"
            | "sin"
            | "cos"
            | "tan"
            | "cot"
            | "sec"
            | "csc"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "sinh"
            | "cosh"
            | "tanh"
    )
}

const SYMBOLS: &[(&str, char)] = &[
    ("pi", 'π'),
    ("theta", 'θ'),
    ("alpha", 'α'),
    ("beta", 'β'),
    ("gamma", 'γ'),
    ("delta", 'δ'),
    ("epsilon", 'ϵ'),
    ("varepsilon", 'ε'),
    ("zeta", 'ζ'),
    ("eta", 'η'),
    ("kappa", 'κ'),
    ("lambda", 'λ'),
    ("mu", 'μ'),
    ("nu", 'ν'),
    ("xi", 'ξ'),
    ("rho", 'ρ'),
    ("sigma", 'σ'),
    ("tau", 'τ'),
    ("phi", 'φ'),
    ("phi", 'ϕ'),
    ("chi", 'χ'),
    ("psi", 'ψ'),
    ("omega", 'ω'),
    ("Gamma", 'Γ'),
    ("Delta", 'Δ'),
    ("Theta", 'Θ'),
    ("Lambda", 'Λ'),
    ("Xi", 'Ξ'),
    ("Pi", 'Π'),
    ("Sigma", 'Σ'),
    ("Phi", 'Φ'),
    ("Psi", 'Ψ'),
    ("Omega", 'Ω'),
    ("infty", '∞'),
    ("pm", '±'),
    ("mp", '∓'),
    ("times", '×'),
    ("div", '÷'),
    ("leq", '≤'),
    ("geq", '≥'),
    ("neq", '≠'),
    ("approx", '≈'),
    ("in", '∈'),
    ("notin", '∉'),
    ("subseteq", '⊆'),
    ("cup", '∪'),
    ("cap", '∩'),
    ("emptyset", '∅'),
    ("partial", '∂'),
    ("nabla", '∇'),
    ("prime", '′'),
    ("circ", '∘'),
    ("to", '→'),
];

pub(crate) fn glyph(c: char) -> Option<&'static str> {
    SYMBOLS
        .iter()
        .find_map(|(name, glyph)| (*glyph == c).then_some(*name))
}

pub(crate) fn symbol(name: &str) -> Option<&'static str> {
    if name == "infinity" {
        return Some("infty");
    }
    if name.chars().count() == 1
        && let Some(found) = glyph(name.chars().next()?)
    {
        return Some(found);
    }
    SYMBOLS
        .iter()
        .find_map(|(key, _)| (*key == name).then_some(*key))
}

pub(crate) fn blackboard(name: &str) -> Option<&'static str> {
    Some(match name {
        "ℝ" | "reals" => "R",
        "ℤ" | "integers" => "Z",
        "ℕ" | "naturals" => "N",
        "ℚ" | "rationals" => "Q",
        "ℂ" | "complexes" => "C",
        _ => return None,
    })
}

pub(crate) fn math_char(c: char) -> bool {
    glyph(c).is_some() || "ℝℤℕℚℂ√°".contains(c)
}

/// Keep copied Unicode scripts equivalent to keyboard-built script slots.
pub(crate) fn normalize(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut mode = None;
    for c in input.chars() {
        let script = "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻ⁿ"
            .chars()
            .position(|s| s == c)
            .map(|index| ('^', "0123456789+-n".as_bytes()[index] as char))
            .or_else(|| {
                "₀₁₂₃₄₅₆₇₈₉₊₋"
                    .chars()
                    .position(|s| s == c)
                    .map(|index| ('_', "0123456789+-".as_bytes()[index] as char))
            });
        let next_mode = script.map(|(kind, _)| kind);
        if mode != next_mode {
            if mode.is_some() {
                output.push(')');
            }
            if let Some(kind) = next_mode {
                output.push(kind);
                output.push('(');
            }
            mode = next_mode;
        }
        output.push(script.map_or(if c == '−' { '-' } else { c }, |(_, digit)| digit));
    }
    if mode.is_some() {
        output.push(')');
    }
    output
}
