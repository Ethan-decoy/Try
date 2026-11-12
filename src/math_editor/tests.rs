use super::*;

#[test]
fn bridge_saves_complete_formulas_without_dropping_or_rewriting_them() {
    for latex in [
        r"\binom{5}{2}",
        r"\prod_{i=1}^{n}i",
        r"\overline{x}",
        r"\underbrace{x+y}_{n}",
        r"\overbrace{x+y}^{n}",
        r"\underset{n}{x}",
        r"\overset{n}{x}",
        r"2^{3}!",
        r"\frac{6}{3}!",
        r"\frac{\binom{n}{k}}{n!}",
        r"\begin{vmatrix}a&b\\c&d\end{vmatrix}",
    ] {
        let mut field = Field::new("", "");
        field.replace_formula(latex).unwrap();
        let (text, saved) = field.content().unwrap();
        assert!(!text.trim().is_empty(), "blank answer: {latex}");
        let mut before = slint_math_input::Editor::new();
        before.import_latex(latex).unwrap();
        let mut after = slint_math_input::Editor::new();
        after.import_latex(&saved).unwrap();
        assert_eq!(after.latex().unwrap(), before.latex().unwrap(), "{latex}");
    }
}

#[test]
fn serialized_math_keeps_word_boundaries_and_script_scope() {
    for (latex, plain) in [
        (r"a^{u}\ln u", "a^u ln u"),
        (r"u'a^{u}\ln u", "u^′a^u ln u"),
        (r"a^{u}b", "a^u b"),
        (r"a^{u}b^{v}", "a^u b^v"),
        (r"a^{2}3", "a^2 3"),
        (r"x_{i}y", "x_i y"),
        (r"x_{1}2", "x_1 2"),
        (r"x_{i}^{j}y", "x_i^j y"),
        (r"\ln u", "ln u"),
        (r"\sin x", "sin x"),
    ] {
        let node = from_latex(latex).unwrap();
        assert_eq!(node.plain(), plain, "{latex}");
        let restored = expression(&node.editor_input()).unwrap();
        assert_eq!(restored.tex(), node.tex(), "{latex}");
    }
    for (latex, text) in [
        (r"\text{un}\text{do}", "undo"),
        (r"\text{中文}x", "中文x"),
        (r"x\text{中文}", "x中文"),
    ] {
        assert_eq!(from_latex(latex).unwrap().plain(), text);
    }
}

#[test]
fn latex_prime_shorthand_round_trips_without_empty_bases() {
    for (shorthand, explicit) in [
        ("u^{'}", r"u^{\prime}"),
        ("f^{''}(x)", r"f^{\prime\prime}(x)"),
        ("f^{'''}(x)", r"f^{\prime\prime\prime}(x)"),
        (
            r"u^{'}f^{'}\left(u\right)",
            r"u^{\prime}f^{\prime}\left(u\right)",
        ),
    ] {
        let node = from_latex(shorthand).unwrap();
        assert!(node.complete(), "complete derivative: {shorthand}");
        assert_eq!(
            node.tex(),
            from_latex(explicit).unwrap().tex(),
            "{shorthand}"
        );
        assert_eq!(
            from_latex(&node.tex()).unwrap().tex(),
            node.tex(),
            "{shorthand}"
        );
        let plain = expression(&node.plain()).unwrap();
        assert!(plain.complete(), "round trip through {}", node.plain());
        let mut field = Field::new("", "");
        field.replace_formula(shorthand).unwrap();
        assert!(field.content().is_ok(), "authoring: {shorthand}");
    }

    // A primed expression used as an exponent still has a real base.
    assert_eq!(
        from_latex("f^{n'}(x)").unwrap(),
        from_latex(r"f^{n^{\prime}}(x)").unwrap()
    );
    for unfinished in [r"u^{}", r"{}^2", r"\frac{1}{}", r"\sqrt{}"] {
        assert!(!from_latex(unfinished).unwrap().complete(), "{unfinished}");
    }
}

#[test]
fn accepted_external_formula_preserves_context_and_undo() {
    let mut field = Field::new("提示：1/2", r"\text{提示：}\frac{1}{2}");
    let original = field.root.clone();
    field.select(vec![1]);
    field.replace_formula(r"\sqrt{x}").unwrap();
    let (_, latex) = field.content().unwrap();
    assert!(latex.contains(r"\text{提示：}"));
    assert!(latex.contains(r"\sqrt{x}"));
    field.undo(false);
    assert_eq!(field.root, original);
    let before = field.root.clone();
    assert!(field.replace_formula(r"\frac{").is_err());
    assert_eq!(field.root, before);
}

#[test]
fn empty_templates_remain_navigable_without_becoming_answers() {
    let mut field = Field::new("", "");
    field.insert_at("math").unwrap();
    field.insert_at("root").unwrap();
    assert!(field.navigate("parent"));
    let generated = field.input();
    field.update(&generated).unwrap();
    assert!(field.navigate("next"));
    assert_eq!(field.input(), "");
    assert!(field.content().is_err());
    field.insert_at("fraction").unwrap();
    assert_eq!(field.input(), "");
    assert!(field.navigate("next"));
    field.update("2").unwrap();
    assert!(field.navigate("previous"));
    field.update("3").unwrap();
    assert_eq!(field.root, expression("sqrt(3/2)").unwrap());
    assert!(field.navigate("down"));
    assert_eq!(field.input(), "2");
    assert!(field.navigate("up"));
    assert_eq!(field.input(), "3");
}

#[test]
fn text_commands_replace_ranges_and_restore_their_caret() {
    let mut field = Field::new("12+34", "12+34");
    field.set_text_selection(3, 5);
    field.type_text("7").unwrap();
    assert_eq!(field.input(), "12+7");
    assert_eq!(
        field.text_selection(),
        TextSelection {
            anchor: 4,
            focus: 4
        }
    );
    field.undo(false);
    assert_eq!(field.input(), "12+34");
    assert_eq!(
        field.text_selection(),
        TextSelection {
            anchor: 3,
            focus: 5
        }
    );
    field.set_text_selection(2, 2);
    field.backspace().unwrap();
    assert_eq!(field.input(), "1+34");
    field.undo(false);
    assert_eq!(field.input(), "12+34");
    assert_eq!(
        field.text_selection(),
        TextSelection {
            anchor: 2,
            focus: 2
        }
    );
    field.backspace().unwrap();
    field.set_text_selection(0, 0);
    field.type_text("9").unwrap();
    assert_eq!(field.input(), "91+34");
    let mut prose = Field::new("中文π", "");
    prose.set_text_selection(6, 6);
    prose.backspace().unwrap();
    assert_eq!(prose.input(), "中π");
}

#[test]
fn invalid_drafts_survive_clear_and_undo_without_being_saved() {
    let mut field = Field::new("3", "3");
    assert!(field.update("sqrt(").is_err());
    field.set_text_selection(2, 5);
    assert!(field.content().is_err());
    field.clear();
    field.undo(false);
    assert_eq!(field.input(), "sqrt(");
    assert!(field.error().is_some());
    assert_eq!(
        field.text_selection(),
        TextSelection {
            anchor: 2,
            focus: 5
        }
    );
    assert!(field.content().is_err());
    field.undo(false);
    assert_eq!(field.input(), "3");
    assert_eq!(field.content().unwrap(), ("3".into(), "3".into()));
    field.undo(true);
    assert_eq!(field.input(), "sqrt(");
}

#[test]
fn templates_wrap_the_selected_range_or_local_operand() {
    let mut field = Field::new("12+34", "12+34");
    field.set_text_selection(3, 5);
    field.insert_at("root").unwrap();
    assert_eq!(field.root, expression("12+sqrt(34)").unwrap());
    field.undo(false);
    field.set_text_selection(0, 2);
    field.insert_at("fraction").unwrap();
    assert!(
        field.input().is_empty(),
        "The denominator is the first empty slot"
    );
    field.update("2").unwrap();
    assert_eq!(field.root, expression("12/2+34").unwrap());
    field.undo(false);
    field.undo(false);
    field.set_text_selection(0, 0);
    field.insert_at("pi").unwrap();
    assert!(field.root.tex().contains("\\pi"));
}

#[test]
fn inserted_structures_keep_neighboring_terms_outside_their_slots() {
    let mut field = Field::new("x+y", "x+y");
    field.set_text_selection(1, 1);
    field.insert_at("pi").unwrap();
    assert_eq!(field.root.plain(), "xπ+y");
    assert_eq!(field.input(), "π");
    assert_eq!(
        field.text_selection(),
        TextSelection {
            anchor: 2,
            focus: 2
        }
    );
    field.undo(false);
    field.set_text_selection(0, 1);
    field.insert_at("fraction").unwrap();
    field.update("2").unwrap();
    assert_eq!(field.root, expression("x/2+y").unwrap());
    let mut field = Field::new("1234", "1234");
    field.set_text_selection(1, 3);
    field.insert_at("fraction").unwrap();
    field.update("5").unwrap();
    assert_eq!(field.root.children[1], expression("23/5").unwrap());
    assert_eq!(field.root.children[2].value, "4");
    let mut field = Field::new("12+34", "12+34");
    field.insert_at("root").unwrap();
    assert_eq!(field.root.plain(), "12+34sqrt()");
    assert!(field.input().is_empty());
    assert!(field.content().is_err());
}

#[test]
fn backspace_enters_filled_structures_and_removes_only_empty_templates() {
    let mut field = Field::new("sqrt(3)/2", r"\frac{\sqrt{3}}{2}");
    let root = field.root.clone();
    field.backspace().unwrap();
    assert_eq!(field.root, root);
    assert_eq!(field.selection, vec![1]);
    assert!(!field.can_undo());
    field.backspace().unwrap();
    assert_eq!(field.root.children[0], root.children[0]);
    assert!(field.input().is_empty());
    field.backspace().unwrap();
    assert_eq!(field.input(), "3");
    assert_eq!(field.root.children[0], root.children[0]);
    let mut empty = Field::new("", "");
    empty.insert_at("math").unwrap();
    empty.insert_at("root").unwrap();
    empty.insert_at("fraction").unwrap();
    let before = empty.root.clone();
    let selected = empty.selection.clone();
    empty.backspace().unwrap();
    assert_eq!(empty.root, Node::blank());
    empty.undo(false);
    assert_eq!(empty.root, before);
    assert_eq!(empty.selection, selected);
    assert!(empty.content().is_err());
}

#[test]
fn leaving_a_root_keeps_it_and_continues_after_it() {
    let mut field = Field::new("", "");
    field.insert_at("math").unwrap();
    field.insert_at("root").unwrap();
    field.update("3").unwrap();
    assert!(field.navigate("exit"));
    assert_eq!(field.input(), "sqrt(3)");
    field.type_text("/2").unwrap();
    assert_eq!(field.root, expression("sqrt(3)/2").unwrap());
    assert!(!field.can_navigate("exit"));
}

#[test]
fn clearing_a_field_restores_the_whole_formula_and_selection_on_undo() {
    let mut field = Field::new("sqrt(3)/2", r"\frac{\sqrt{3}}{2}");
    field.select(vec![0, 0]);
    let before = field.root.clone();
    let selection = field.selection.clone();
    field.insert("clear").unwrap();
    assert_eq!(field.input(), "");
    assert!(field.selection.is_empty());
    field.clear(); // An already empty field does not add a history entry.
    assert!(
        field.content().is_err(),
        "Clearing does not create an answer of zero"
    );
    field.undo(false);
    assert_eq!(field.root, before);
    assert_eq!(field.selection, selection);
    assert_eq!(
        field.content().unwrap(),
        ("sqrt(3)/2".into(), r"\frac{\sqrt{3}}{2}".into())
    );
    field.undo(true);
    assert_eq!(field.input(), "");
    assert!(field.content().is_err());
}

#[test]
fn clearing_prose_and_unfilled_templates_is_undoable() {
    let mut prose = Field::new("普通问题", "");
    prose.clear();
    assert!(prose.root.is_text());
    assert_eq!(prose.input(), "");
    prose.undo(false);
    assert_eq!(prose.content().unwrap(), ("普通问题".into(), "".into()));
    assert!(!from_latex(r"\sqrt{}").unwrap().complete());
    let mut formula = Field::new("", "");
    formula.insert("math").unwrap();
    formula.insert("root").unwrap();
    assert!(formula.content().is_err());
    formula.clear();
    formula.undo(false);
    assert_eq!(formula.root.tex(), r"\sqrt{\square}");
    assert!(formula.content().is_err());
}

#[test]
fn root_first_fraction_can_move_out_without_retyping() {
    let mut field = Field::new("", "");
    field.insert("math").unwrap();
    field.insert("root").unwrap();
    field.insert("fraction").unwrap();
    field.update("2").unwrap();
    field.select(vec![0, 0]);
    field.update("3").unwrap();
    assert_eq!(field.root, expression("sqrt(3/2)").unwrap());

    field.select(vec![0, 1]);
    field.insert("move_out").unwrap();
    assert_eq!(field.root, expression("sqrt(3)/2").unwrap());
    assert_eq!(field.input(), "2");
    assert!(field.content().is_ok());
    field.undo(false);
    assert_eq!(field.root, expression("sqrt(3/2)").unwrap());
    field.undo(true);
    assert_eq!(field.root, expression("sqrt(3)/2").unwrap());
}

#[test]
fn moving_structures_preserves_every_slot_and_its_selection() {
    for (before, after, paths) in [
        ("sqrt(3/2)", "sqrt(3)/2", vec![vec![0, 0], vec![0, 1]]),
        ("sqrt(x^2)", "sqrt(x)^2", vec![vec![0, 0], vec![0, 1]]),
        ("sqrt(x_1)", "sqrt(x)_1", vec![vec![0, 0], vec![0, 1]]),
        (
            "sqrt(x_1^2)",
            "sqrt(x)_1^2",
            vec![vec![0, 0], vec![0, 1], vec![0, 2]],
        ),
        ("vec(a/b)", "vec(a)/b", vec![vec![0, 0], vec![0, 1]]),
    ] {
        for path in paths {
            let mut field = Field::new(before, &expression(before).unwrap().tex());
            field.select(path);
            let input = field.input();
            assert!(field.can_move_out());
            field.insert("move_out").unwrap();
            assert_eq!(field.root, expression(after).unwrap(), "{before}");
            assert_eq!(
                field.input(),
                input,
                "The selected slot follows its content"
            );
            assert!(field.content().is_ok());
            field.update("7").unwrap();
            assert_eq!(field.selected().value, "7");
            field.undo(false);
            field.undo(false);
            assert_eq!(field.root, expression(before).unwrap());
        }
    }
}

#[test]
fn structure_repairs_preserve_siblings_and_unfilled_slots() {
    let mut field = Field::new("求 ", "");
    field.insert("math").unwrap();
    field.insert("root").unwrap();
    field.insert("fraction").unwrap();
    field.update("2").unwrap();
    field.insert("move_out").unwrap();
    assert_eq!(field.selection, vec![1, 1]);
    assert!(field.content().is_err(), "The numerator is still unfilled");
    assert_eq!(field.root.children[0].value, "求 ");
    field.select(vec![1, 0, 0]);
    field.update("3").unwrap();
    assert!(field.content().unwrap().1.contains(r"\text{求 }"));
    assert_eq!(field.root.children[1], expression("sqrt(3)/2").unwrap());

    let mut field = Field::new("", "");
    field.insert("nth_root").unwrap();
    field.update("4").unwrap();
    assert!(!field.can_move_out());
    assert!(
        !field.can_unwrap(),
        "The root's degree must not lose its value"
    );
    field.select(vec![1]);
    field.update("3/2").unwrap();
    field.select(vec![1, 0]);
    field.insert("move_out").unwrap();
    assert_eq!(field.root.tex(), r"\frac{\sqrt[4]{3}}{2}");
    assert_eq!(field.selection, vec![0, 1]);
    assert_eq!(field.input(), "3");
}

#[test]
fn removing_a_wrapper_retains_its_content_and_can_be_undone() {
    let mut field = Field::new("sqrt(3/2)", r"\sqrt{\frac{3}{2}}");
    field.select(vec![0, 1]);
    assert!(field.can_unwrap());
    field.insert("unwrap").unwrap();
    assert_eq!(field.root, expression("3/2").unwrap());
    assert_eq!(field.selection, vec![1]);
    assert_eq!(field.input(), "2");
    field.undo(false);
    assert_eq!(field.root, expression("sqrt(3/2)").unwrap());
    assert_eq!(field.selection, vec![0, 1]);
    field.undo(true);
    field.select(vec![0]);
    field.insert("root").unwrap();
    assert_eq!(field.root, expression("sqrt(3)/2").unwrap());

    let mut field = Field::new("", "");
    field.insert("group").unwrap();
    field.update("x+1").unwrap();
    field.parent();
    field.insert("unwrap").unwrap();
    assert_eq!(field.root, expression("x+1").unwrap());
    assert_eq!(field.selection, Vec::<usize>::new());
    assert_eq!(field.input(), "x+1");
}

#[test]
fn unavailable_repairs_do_not_change_formula_or_history() {
    for input in ["3/2", "x^2", "[[1,2],[3,4]]", "log(2,8)"] {
        let mut field = Field::new(input, &expression(input).unwrap().tex());
        let root = field.root.clone();
        assert!(!field.can_move_out(), "{input}");
        assert!(!field.can_unwrap(), "{input}");
        assert!(field.insert("move_out").is_err());
        assert!(field.insert("unwrap").is_err());
        assert_eq!(field.root, root);
        assert!(!field.is_dirty());
        assert!(!field.can_undo());
    }
}

#[test]
fn expressions_build_the_same_structures_as_visual_tools() {
    for input in [
        "sin(pi/3)",
        "sqrt(3)/2",
        "a^2+b^2=c^2",
        "x_1^2",
        "x^2_1",
        "int(0,1,x^2)",
        "sum(k=1,n,k)",
        "lim(x->0,sin(x)/x)",
        "[[1,2],[3,4]]",
        "cases(x,x>=0,-x,x<0)",
    ] {
        let root = expression(input).unwrap_or_else(|e| panic!("{input}: {e}"));
        assert!(root.complete());
        crate::math_latex::validate(&root.tex()).unwrap();
        let (_, leaves) = root.marked_tex();
        assert!(!leaves.is_empty());
        assert_eq!(expression(&root.editor_input()).unwrap().tex(), root.tex());
    }
    let mut visual = Field::new("", "");
    visual.insert("math").unwrap();
    visual.update("sqrt(3)").unwrap();
    visual.insert("fraction").unwrap();
    visual.update("2").unwrap();
    assert_eq!(visual.root, expression("sqrt(3)/2").unwrap());
}

#[test]
fn multiplication_dots_round_trip_with_product_precedence() {
    for (input, canonical) in [
        ("p^n⋅q^n", "p^n*q^n"),
        ("p^n·q^n", "p^n*q^n"),
        ("a⋅b+c", "a*b+c"),
        ("a+b·c", "a+b*c"),
        ("a⋅b/c", "a*b/c"),
    ] {
        let node = expression(input).unwrap_or_else(|error| panic!("{input}: {error}"));
        assert_eq!(node, expression(canonical).unwrap());
        assert_eq!(expression(&node.editor_input()).unwrap(), node);
    }
    let quoted = expression(r#""A·B""#).unwrap();
    assert_eq!(quoted.kind, Kind::Text);
    assert_eq!(quoted.value, "A·B");
}

#[test]
fn imported_content_and_cancel_are_lossless_until_edited() {
    let original = ("sin(π/3) = ?", r"\sin\left(\frac{\pi}{3}\right)=?");
    let mut field = Field::new(original.0, original.1);
    assert_eq!(
        field.content().unwrap(),
        (original.0.into(), original.1.into())
    );
    let (_, leaves) = field.root.marked_tex();
    let denominator = leaves
        .iter()
        .find(|leaf| field.root.at(&leaf.path).unwrap().value == "3")
        .unwrap()
        .path
        .clone();
    field.select(denominator);
    field.update("6").unwrap();
    assert!(field.content().unwrap().1.contains("6"));
    field.undo(false);
    assert_eq!(field.root, from_latex(original.1).unwrap());
}

#[test]
fn mixed_text_and_grid_cells_preserve_native_text() {
    let root = automatic_content("计算 sin(pi/3) 的值").unwrap();
    assert!(root.tex().contains(r"\text{计算}"));
    assert!(root.tex().contains(r"\sin"));
    let mut field = Field::new("", "");
    field.insert("matrix").unwrap();
    assert!(field.content().is_err());
    field.resize_grid(3, 3).unwrap();
    assert_eq!(field.grid_size(), Some((3, 3)));
    assert_eq!(field.root.children.len(), 9);
    assert_eq!(automatic_content("hello world").unwrap().kind, Kind::Text);
}

#[test]
fn common_functions_and_copied_notation_remain_editable() {
    for input in [
        "log(2,x)",
        "log_2(x)",
        "Log(x)",
        "ln(x)",
        "lg(x)",
        "sin^2(x)",
        "arcsin(x)",
        "asin(x)",
        "sinh(x)",
        "|x|",
        "||x||",
        "x²+x₁",
        "a_{n+1}",
        "a_(n+1)",
        "ε+epsilon",
        "x∈ℝ",
        "a±b",
        "vec(v)",
        "dot(x)",
        "ddot(x)",
        "bar(x)",
        "hat(x)",
        "f'(x)",
        "f''(x)",
        "30°",
        "lim(x->0+,1/x)",
        "lim(x->0-,1/x)",
    ] {
        let node = expression(input).unwrap_or_else(|error| panic!("{input}: {error}"));
        assert!(node.complete(), "{input}");
        assert_eq!(
            expression(&node.editor_input()).unwrap().tex(),
            node.tex(),
            "{input}"
        );
        let imported = from_latex(&node.tex()).unwrap();
        assert_ne!(imported.kind, Kind::Raw, "{input}");
        assert!(
            expression(&imported.editor_input()).is_ok(),
            "{input}: {}",
            imported.editor_input()
        );
    }
    assert_eq!(
        expression("log(2,x)").unwrap(),
        expression("log_2(x)").unwrap()
    );
    assert!(expression("sin^2(x)").unwrap().tex().contains(r"\sin"));
    assert!(expression("asin(x)").unwrap().tex().contains(r"\arcsin"));
    assert_eq!(expression("x÷2").unwrap(), expression("x/2").unwrap());
}

#[test]
fn logarithm_template_has_independent_base_and_argument_slots() {
    let mut field = Field::new("", "");
    field.insert("math").unwrap();
    field.insert("function:log_base").unwrap();
    assert_eq!(field.selection, vec![0, 1]);
    assert!(field.content().is_err());
    field.update("2").unwrap();
    field.select(vec![1, 0]);
    field.update("8").unwrap();
    assert_eq!(
        field.content().unwrap().1,
        expression("log(2,8)").unwrap().tex()
    );
    field.undo(false);
    assert!(field.content().is_err());
    field.undo(true);
    assert!(field.content().is_ok());

    field.select(vec![]);
    field.update(r"\log_{3}(9)").unwrap();
    assert!(field.content().unwrap().1.contains(r"\log"));
    let before = field.root.clone();
    assert!(field.update("log(3,)").is_err());
    assert_eq!(
        field.root, before,
        "Invalid input preserves the formula draft"
    );
}

#[test]
fn visual_functions_and_symbols_preserve_the_current_expression() {
    for name in [
        "log", "ln", "lg", "sin", "arcsin", "sinh", "abs", "norm", "vec", "dot", "ddot", "bar",
        "hat",
    ] {
        let mut field = Field::new("x", "x");
        field.insert(&format!("function:{name}")).unwrap();
        assert_eq!(field.selected().value, "x");
        assert_eq!(
            field.content().unwrap().1,
            expression(&format!("{name}(x)")).unwrap().tex()
        );
        field.undo(false);
        assert_eq!(field.content().unwrap(), ("x".into(), "x".into()));
    }
    let mut field = Field::new("x", "x");
    field.insert("symbol:in").unwrap();
    assert!(field.content().is_err());
    field.insert("symbol:reals").unwrap();
    assert_eq!(field.content().unwrap().1, expression("x∈ℝ").unwrap().tex());

    for name in [
        "epsilon",
        "varepsilon",
        "pi",
        "infty",
        "partial",
        "nabla",
        "reals",
        "integers",
        "naturals",
        "complexes",
    ] {
        let mut field = Field::new("", "");
        field.insert("math").unwrap();
        field.insert(&format!("symbol:{name}")).unwrap();
        assert!(field.content().is_ok(), "{name}");
    }
}

#[test]
fn ordinary_questions_stay_plain_and_can_add_a_math_fragment() {
    for input in [
        "犹豫的英文？",
        "Ctrl+C",
        "len(items)",
        "Please log this error.",
        "It's fine.",
        "C:/notes/topic",
        "https://example.com",
    ] {
        let mut field = Field::new("", "");
        field.update(input).unwrap();
        assert!(field.root.is_text(), "{input}");
        assert_eq!(field.content().unwrap(), (input.into(), String::new()));
    }
    let mut field = Field::new("求 ", "");
    field.insert("math").unwrap();
    field.insert("function:log_base").unwrap();
    field.update("2").unwrap();
    field.select(vec![1, 1, 0]);
    field.update("8").unwrap();
    assert!(field.content().unwrap().1.contains(r"\text{求 }"));
    assert!(field.content().unwrap().1.contains(r"\log"));
}
