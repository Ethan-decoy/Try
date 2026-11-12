//! Content-editing session: drafts, UTF-8 selection, structural tools and undo.
use super::parser::{applied_function, logarithm};
use super::{Kind, Node, automatic_content, expression, from_latex};
use crate::{math_vocabulary as vocabulary, tr};

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
        if self.selected().accepts_prose() {
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
            } else if crate::math_structure::is_latex(input) {
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
    /// Apply an explicitly accepted external formula to the selected slot.
    /// Import/validation happens before the original field is changed.
    pub fn replace_formula(&mut self, latex: &str) -> Result<(), String> {
        let node = from_latex(latex).unwrap_or_else(|_| Node::leaf(Kind::Raw, latex));
        node.validate()?;
        self.replace(node);
        self.caret_to_end();
        Ok(())
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
