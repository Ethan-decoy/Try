//! Source ranges projected onto an already rendered formula. Moving a caret
//! or dragging a selection uses only this small layout, without rasterizing.
use crate::formula::HitRegion;
use crate::math_editor::TextSelection;
use crate::math_editor::{Kind, Node};

#[derive(Clone, Default)]
struct Snapshot {
    input: String,
    selection: TextSelection,
}

/// One native edit is one undo operation, including invalid drafts and full
/// replacements. Native TextInput's coalescing must not erase several drafts.
#[derive(Default)]
pub struct History {
    current: Snapshot,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl History {
    pub fn record(&mut self, input: &str) {
        if self.current.input == input {
            return;
        }
        push_snapshot(&mut self.undo, &self.current);
        self.redo.clear();
        self.current = Snapshot {
            input: input.into(),
            selection: TextSelection {
                anchor: input.len(),
                focus: input.len(),
            },
        };
    }
    pub fn set_selection(&mut self, input: &str, anchor: usize, caret: usize) {
        if self.current.input != input {
            return;
        }
        let bound = |offset: usize| {
            let mut offset = offset.min(input.len());
            while !input.is_char_boundary(offset) {
                offset -= 1;
            }
            offset
        };
        self.current.selection = TextSelection {
            anchor: bound(anchor),
            focus: bound(caret),
        };
    }
    pub fn step(&mut self, redo: bool) -> Option<(String, TextSelection)> {
        let previous = if redo {
            self.redo.pop()?
        } else {
            self.undo.pop()?
        };
        push_snapshot(
            if redo { &mut self.undo } else { &mut self.redo },
            &self.current,
        );
        self.current = previous;
        Some((self.current.input.clone(), self.current.selection))
    }
}

fn push_snapshot(stack: &mut Vec<Snapshot>, snapshot: &Snapshot) {
    // The current native buffer may exceed the parser's limit; do not retain
    // many copies of such a paste. Previous valid drafts remain undoable.
    if snapshot.input.chars().take(1001).count() > 1000 {
        return;
    }
    stack.push(snapshot.clone());
    if stack.len() > 32 {
        stack.remove(0);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug)]
struct Stop {
    offset: usize,
    rect: Rect,
}

#[derive(Clone, Debug)]
struct Cell {
    start: usize,
    end: usize,
    rect: Rect,
}

pub struct Layout {
    pub source: String,
    stops: Vec<Stop>,
    cells: Vec<Cell>,
    bounds: Rect,
}

impl Layout {
    /// Keep the exact source, including pasted LaTeX. Only show an editable
    /// formula when every painted leaf has a source range we can address.
    pub fn new(
        source: &str,
        node: &Node,
        regions: &[HitRegion],
        width: f32,
        height: f32,
    ) -> Option<Self> {
        let tokens = tokens(source);
        let mut next = 0;
        let mut after = 0;
        let mut stops = vec![];
        let mut cells = vec![];
        for region in regions {
            let leaf = node.at(&region.path)?;
            let (index, token, end) =
                tokens[next..]
                    .iter()
                    .enumerate()
                    .find_map(|(index, token)| {
                        let start = token.start.max(after);
                        let end = token.start + token.text.len();
                        if start >= end {
                            return None;
                        }
                        let token = Token {
                            start,
                            text: &source[start..end],
                            command: token.command,
                            structural: token.structural,
                        };
                        let length = token.matched_bytes(leaf, &source[start..])?;
                        Some((next + index, token, start + length))
                    })?;
            next = index;
            after = end;
            while next < tokens.len() && tokens[next].start + tokens[next].text.len() <= end {
                next += 1;
            }
            let text = &source[token.start..end];
            let atomic =
                token.command || !matches!(leaf.kind, Kind::Literal | Kind::Operator | Kind::Text);
            let offsets = if atomic {
                vec![0, text.len()]
            } else {
                text.char_indices()
                    .map(|(index, _)| index)
                    .chain([text.len()])
                    .collect()
            };
            let count = offsets.len() - 1;
            for (index, &offset) in offsets.iter().enumerate() {
                stops.push(Stop {
                    offset: token.start + offset,
                    rect: Rect {
                        x: region.x + region.width * index as f32 / count as f32,
                        y: region.y - 2.0,
                        width: 1.0,
                        height: region.height + 4.0,
                    },
                });
                if let Some(&end) = offsets.get(index + 1) {
                    cells.push(Cell {
                        start: token.start + offset,
                        end: token.start + end,
                        rect: Rect {
                            x: region.x + region.width * index as f32 / count as f32,
                            y: region.y - 2.0,
                            width: region.width / count as f32,
                            height: region.height + 4.0,
                        },
                    });
                }
            }
        }
        if stops.is_empty() {
            return None;
        }
        Some(Self {
            source: source.into(),
            stops,
            cells,
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
        })
    }

    pub fn caret(&self, offset: usize) -> Rect {
        self.stops
            .iter()
            .min_by_key(|stop| stop.offset.abs_diff(offset))
            .unwrap()
            .rect
    }

    pub fn hit(&self, x: f32, y: f32) -> usize {
        self.stops
            .iter()
            .min_by(|a, b| {
                let distance = |stop: &Stop| {
                    let vertical = if y < stop.rect.y {
                        stop.rect.y - y
                    } else {
                        (y - stop.rect.y - stop.rect.height).max(0.0)
                    };
                    (stop.rect.x - x).abs() + vertical * 4.0
                };
                distance(a).total_cmp(&distance(b))
            })
            .unwrap()
            .offset
    }

    pub fn selection(&self, anchor: usize, caret: usize) -> Vec<Rect> {
        let start = anchor.min(caret);
        let end = anchor.max(caret);
        if start == end {
            return vec![];
        }
        if start == 0 && end >= self.source.len() {
            return vec![self.bounds];
        }
        self.cells
            .iter()
            .filter(|cell| cell.start < end && cell.end > start)
            .map(|cell| cell.rect)
            .collect()
    }

    /// Horizontal movement skips hidden command/bracket bytes; vertical
    /// movement chooses a nearby visible row, e.g. numerator/denominator.
    pub fn move_caret(&self, offset: usize, direction: i32) -> usize {
        if direction == 4 {
            return 0;
        }
        if direction == 5 {
            return self.source.len();
        }
        let current = self.caret(offset);
        let moved = |stop: &&Stop| {
            (stop.rect.x - current.x).abs() > 0.1 || (stop.rect.y - current.y).abs() > 0.1
        };
        match direction {
            0 => self
                .stops
                .iter()
                .filter(|stop| stop.offset < offset)
                .filter(moved)
                .max_by_key(|stop| stop.offset),
            1 => self
                .stops
                .iter()
                .filter(|stop| stop.offset > offset)
                .filter(moved)
                .min_by_key(|stop| stop.offset),
            2 | 3 => {
                let center = current.y + current.height / 2.0;
                self.stops
                    .iter()
                    .filter(|stop| {
                        let delta = stop.rect.y + stop.rect.height / 2.0 - center;
                        if direction == 2 {
                            delta < -current.height * 0.4
                        } else {
                            delta > current.height * 0.4
                        }
                    })
                    .min_by(|a, b| {
                        let distance = |stop: &Stop| {
                            (stop.rect.x - current.x).abs()
                                + (stop.rect.y + stop.rect.height / 2.0 - center).abs() * 2.0
                        };
                        distance(a).total_cmp(&distance(b))
                    })
            }
            _ => None,
        }
        .map_or(offset, |stop| stop.offset)
    }
}

struct Token<'a> {
    start: usize,
    text: &'a str,
    command: bool,
    structural: bool,
}

impl Token<'_> {
    fn matched_bytes(&self, node: &Node, remaining: &str) -> Option<usize> {
        if self.structural || self.command && !matches!(node.kind, Kind::Symbol | Kind::Operator) {
            return None;
        }
        let text = self.text.trim_start_matches('\\');
        let exact = text == node.value
            || text == node.editor_input()
            || crate::math_vocabulary::normalize(text).trim_start_matches(['^', '_'])
                == node.editor_input()
            || crate::math_vocabulary::function(text).is_some_and(|name| name == node.value)
            || match text {
                "²" => node.value == "2",
                "³" => node.value == "3",
                "−" => node.value == "-",
                "*" | "×" => node.value == "times" || node.value == "×" || node.value == "*",
                "->" => node.value == "rightarrow",
                "'" => node.value == "prime",
                _ => false,
            };
        if exact {
            Some(self.text.len())
        } else if matches!(node.kind, Kind::Literal | Kind::Text)
            && !node.value.is_empty()
            && remaining.starts_with(&node.value)
        {
            Some(node.value.len())
        } else {
            None
        }
    }
}

fn tokens(source: &str) -> Vec<Token<'_>> {
    let mut tokens = vec![];
    let latex = source.contains('\\');
    let mut index = 0;
    while index < source.len() {
        let start = index;
        let character = source[index..].chars().next().unwrap();
        index += character.len_utf8();
        let command = character == '\\';
        if command {
            if index < source.len() && !source.as_bytes()[index].is_ascii_alphabetic() {
                index += source[index..].chars().next().unwrap().len_utf8();
            } else {
                while index < source.len() && source.as_bytes()[index].is_ascii_alphabetic() {
                    index += 1;
                }
            }
        } else if character.is_ascii_alphanumeric() || character == '.' {
            while index < source.len()
                && (source.as_bytes()[index].is_ascii_alphanumeric()
                    || source.as_bytes()[index] == b'.')
            {
                index += 1;
            }
        } else if source[start..].starts_with("->") {
            index += 1;
        }
        let text = &source[start..index];
        if matches!(
            text,
            r"\begin" | r"\end" | r"\color" | r"\textcolor" | r"\hspace" | r"\vspace" | r"\kern"
        ) {
            index = skip_tex_metadata(source, index);
            continue;
        }
        let structural = !latex
            && !command
            && source[index..].trim_start().starts_with('(')
            && matches!(
                text,
                "sqrt"
                    | "root"
                    | "abs"
                    | "norm"
                    | "vec"
                    | "hat"
                    | "bar"
                    | "dot"
                    | "ddot"
                    | "sum"
                    | "prod"
                    | "int"
                    | "lim"
                    | "cases"
            );
        tokens.push(Token {
            start,
            text,
            command,
            structural,
        });
    }
    tokens
}

fn skip_tex_metadata(source: &str, mut index: usize) -> usize {
    // Environment names, colours and spacing parameters have no painted leaf.
    // Skip their balanced argument while retaining the original source buffer.
    while index < source.len() && source[index..].starts_with(char::is_whitespace) {
        index += source[index..].chars().next().unwrap().len_utf8();
    }
    let group = |start: usize, left: char, right: char| {
        let mut depth = 0;
        for (offset, character) in source[start..].char_indices() {
            if character == left {
                depth += 1;
            }
            if character == right {
                depth -= 1;
                if depth == 0 {
                    return start + offset + character.len_utf8();
                }
            }
        }
        source.len()
    };
    if source[index..].starts_with('[') {
        index = group(index, '[', ']');
    }
    while index < source.len() && source[index..].starts_with(char::is_whitespace) {
        index += source[index..].chars().next().unwrap().len_utf8();
    }
    if source[index..].starts_with('{') {
        index = group(index, '{', '}');
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout(input: &str) -> Layout {
        let node = crate::math_answer::recognize_inline(input, true).unwrap();
        let rendered = crate::formula::render_node(&node, 22, 1.0, (232, 232, 232)).unwrap();
        Layout::new(
            input,
            &node,
            &rendered.regions,
            rendered.width,
            rendered.height,
        )
        .unwrap()
    }
    #[test]
    fn inline_ranges_follow_actual_fraction_rows_and_utf8_symbols() {
        let input = "sqrt(π)/2";
        let layout = layout(input);
        let top = layout.caret(5);
        let bottom = layout.caret(input.len());
        assert!(bottom.y > top.y);
        assert_eq!(layout.hit(top.x, top.y + top.height / 2.0), 5);
        assert!(layout.caret(layout.move_caret(input.len(), 2)).y < bottom.y);
        assert!(layout.caret(layout.move_caret(5, 3)).y > top.y);
        assert!(layout.source.is_char_boundary(layout.move_caret(5, 1)));
        assert_eq!(layout.selection(0, input.len()).len(), 1);
    }
    #[test]
    fn latex_keeps_original_source_and_maps_commands_to_visible_symbols() {
        let input = r"\frac{\sqrt{\pi}}{2}";
        let layout = layout(input);
        let pi = input.find(r"\pi").unwrap();
        assert_eq!(layout.hit(layout.caret(pi).x, layout.caret(pi).y), pi);
        assert!(layout.caret(input.find('2').unwrap()).y > layout.caret(pi).y);
        assert!(!layout.selection(pi, pi + 3).is_empty());
        assert_eq!(layout.source, input);
    }
    #[test]
    fn repeated_variables_do_not_match_structural_function_names() {
        let layout = layout("sqrt(s)+s^2");
        let first = layout.caret(5);
        let second = layout.caret(8);
        assert!(second.x > first.x);
        assert_ne!(layout.hit(first.x, first.y), 0);
        assert!(layout.caret(10).y < second.y);
    }

    #[test]
    fn common_friendly_and_latex_formulas_have_addressable_input_ranges() {
        for source in [
            "1/2",
            "(x+1)/2",
            "sin(pi/3)",
            "log(2,x)",
            "root(3,x)",
            "f'(x)",
            "sin²(x)",
            "|x|",
            "[[1,2],[3,4]]",
            "sum(n=1,10,n^2)",
            r"\sqrt[3]{x}",
            r"\sum_{n=1}^{10}n^2",
            r"\mathbb{R}",
            r"\frac{ab}{cd}",
            r"\frac{12034}{2}",
        ] {
            let node = crate::math_answer::recognize_inline(source, true).unwrap();
            let rendered = crate::formula::render_node(&node, 22, 1.0, (232, 232, 232)).unwrap();
            let layout = Layout::new(
                source,
                &node,
                &rendered.regions,
                rendered.width,
                rendered.height,
            );
            assert!(layout.is_some(), "{source}");
            let layout = layout.unwrap();
            assert!(
                layout
                    .stops
                    .iter()
                    .all(|stop| source.is_char_boundary(stop.offset))
            );
        }
    }

    #[test]
    fn latex_environment_names_are_not_mistaken_for_matrix_cells() {
        let source = r"\begin{bmatrix}b&n\\m&s\end{bmatrix}";
        let layout = layout(source);
        let body = source.find("}b").unwrap() + 1;
        let rect = layout.caret(body);
        assert_eq!(layout.hit(rect.x, rect.y + rect.height / 2.0), body);
        assert!(layout.stops.iter().all(|stop| stop.offset >= body));
    }

    #[test]
    fn history_restores_each_replacement_and_its_source_selection() {
        let mut history = History::default();
        history.record("Paris");
        history.set_selection("Paris", 0, 5);
        history.record("sqrt(");
        assert_eq!(
            history.step(false),
            Some((
                "Paris".into(),
                TextSelection {
                    anchor: 0,
                    focus: 5
                }
            ))
        );
        assert_eq!(history.step(true).unwrap().0, "sqrt(");
        history.record(r"\frac{1}{2}");
        assert_eq!(history.step(false).unwrap().0, "sqrt(");
        history.record("π");
        history.set_selection("π", 1, 2);
        assert_eq!(history.step(false).unwrap().0, "sqrt(");
        assert_eq!(
            history.step(true).unwrap().1,
            TextSelection {
                anchor: 0,
                focus: 2
            }
        );
    }

    #[test]
    fn history_is_bounded_and_does_not_accumulate_oversized_pastes() {
        let mut history = History::default();
        for index in 0..40 {
            history.record(&index.to_string());
        }
        assert_eq!(history.undo.len(), 32);
        history.record(&"x".repeat(1001));
        history.record("short");
        assert_eq!(history.undo.len(), 32);
        assert!(
            history
                .undo
                .iter()
                .all(|snapshot| snapshot.input.len() <= 1000)
        );
        assert_eq!(history.step(false).unwrap().0, "39");
    }
}
