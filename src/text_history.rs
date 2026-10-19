//! Bounded native-text undo history, including draft selection and UTF-8 positions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub focus: usize,
}

#[derive(Clone, Default)]
struct Snapshot {
    input: String,
    selection: Selection,
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
            selection: Selection {
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
        self.current.selection = Selection {
            anchor: bound(anchor),
            focus: bound(caret),
        };
    }
    pub fn step(&mut self, redo: bool) -> Option<(String, Selection)> {
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

#[cfg(test)]
mod tests {
    use super::*;
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
                Selection {
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
            Selection {
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
