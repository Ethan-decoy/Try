//! On-demand content authoring. Public entry points stay stable while the
//! tree, serialized import, expression grammar and edit session remain private.
//! Labels (`plain`), reimportable input (`editor_input`/`answer_input`) and saved
//! LaTeX (`tex`) have distinct roles; display output is owned by `formula`.
mod field;
mod latex;
mod node;
mod parser;

pub use field::{Field, TextSelection};
pub use latex::{answer_input, from_latex};
pub use node::{Kind, Leaf, Node};
pub use parser::{automatic_content, expression};
pub(crate) use parser::{inline_division, looks_mathematical};

const MAX_INPUT: usize = crate::math_latex::MAX_FORMULA_CHARS;
const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 400;

#[cfg(test)]
mod tests;
