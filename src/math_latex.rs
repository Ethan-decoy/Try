//! Shared serialized LaTeX rules. Validation checks syntax and resource limits;
//! it does not promise that a particular font/backend can display the formula.
//! Native labels and reimportable editor input are separate projections owned
//! by `math_editor`, never substitutes for the stored LaTeX.
use crate::tr;
use std::borrow::Cow;

pub(crate) const MAX_FORMULA_CHARS: usize = 1000;

/// Preserve notation shared by SMI and the display parser without changing
/// command prefixes, escaped backslashes or literal text inside a formula.
pub fn compatible_latex(tex: &str) -> Cow<'_, str> {
    const ALIASES: [(&str, &str); 5] = [
        (r"\degree", "°"),
        (r"\supsetneq", "⊋"),
        (r"\uplus", "⊎"),
        (r"\coloneqq", "≔"),
        (r"\hom", r"\operatorname{hom}"),
    ];
    if !ALIASES.iter().any(|(name, _)| tex.contains(*name)) {
        return Cow::Borrowed(tex);
    }
    let mut output = String::with_capacity(tex.len());
    let mut position = 0;
    let mut changed = false;
    while position < tex.len() {
        let start = position;
        let ch = tex[position..].chars().next().unwrap();
        position += ch.len_utf8();
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        while position < tex.len() && tex.as_bytes()[position].is_ascii_alphabetic() {
            position += 1;
        }
        let command = &tex[start..position];
        if position == start + 1 {
            if let Some(ch) = tex[position..].chars().next() {
                position += ch.len_utf8();
            }
            output.push_str(&tex[start..position]);
        } else if let Some((_, replacement)) = ALIASES.iter().find(|(name, _)| *name == command) {
            output.push_str(replacement);
            changed = true;
        } else {
            if matches!(
                command,
                r"\text"
                    | r"\textrm"
                    | r"\textbf"
                    | r"\textit"
                    | r"\textsf"
                    | r"\texttt"
                    | r"\mbox"
                    | r"\operatorname"
            ) {
                let body = position + tex[position..].len() - tex[position..].trim_start().len();
                if tex[body..].starts_with('{') {
                    position = literal_group_end(tex, body);
                }
            }
            output.push_str(&tex[start..position]);
        }
    }
    if changed {
        Cow::Owned(output)
    } else {
        Cow::Borrowed(tex)
    }
}

fn literal_group_end(tex: &str, start: usize) -> usize {
    let mut depth = 0;
    let mut escaped = false;
    for (offset, ch) in tex[start..].char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return start + offset + 1;
            }
        }
    }
    tex.len()
}

pub fn validate(tex: &str) -> Result<(), String> {
    if tex.trim().is_empty() || tex.chars().count() > MAX_FORMULA_CHARS {
        return Err(tr!("公式须为 1 至 1000 个字符。").into());
    }
    latex_rust::parse(&compatible_latex(tex))
        .map(|_| ())
        .map_err(|error| tr!("公式暂时无法识别：{error}", error = error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notation_aliases_respect_command_boundaries_and_literal_text() {
        for source in [
            r"\degreeC+\homogeneous+\uplusx",
            r"\\degree",
            r"\text{\degree {\hom}}",
            r"\text{\degree",
            r"\operatorname{\hom}",
        ] {
            assert_eq!(compatible_latex(source), source);
        }
        assert_eq!(
            compatible_latex(r"\text{\degree \{x\}}+\degree"),
            r"\text{\degree \{x\}}+°"
        );
        assert!(validate(r"\degreeC").is_err());
    }
}
