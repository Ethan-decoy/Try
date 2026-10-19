//! Question drafts and mixed text/math authoring in the main window.
use super::*;

impl App {
    pub(super) fn set_editor_answer_mode(&self, index: i32) {
        if self.pending_card_delete.borrow().is_some() {
            return;
        }
        let Some(mode) = usize::try_from(index)
            .ok()
            .and_then(|index| AnswerMode::ALL.get(index).copied())
        else {
            return;
        };
        if !self.apply_content_math() {
            return;
        }
        if !self.flush_math() {
            return;
        }
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_editor_answer_mode(index);
            window.set_editor_error("".into());
            window.set_math_active_field(-1);
        }
        self.seed_judgment(mode);
        self.render_editor_preview();
    }

    pub(super) fn seed_judgment(&self, mode: AnswerMode) {
        if mode != AnswerMode::TrueFalse {
            return;
        }
        let mut fields = self.math_fields.borrow_mut();
        if fields.len() >= 5
            && fields[4].root.plain().trim().is_empty()
            && let Ok((text, latex)) = fields[0].content()
        {
            fields[4] = Field::new(&text, &latex);
        }
    }

    pub(super) fn open_editor(
        &self,
        index: Option<i32>,
        new_pack: bool,
        requested_mode: Option<AnswerMode>,
    ) {
        self.close_structured_math_for(false);
        if self.pending_delete.borrow().is_some()
            || self.pending_import.borrow().is_some()
            || self.pending_card_delete.borrow().is_some()
        {
            return;
        }
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let library = self.library.borrow();
        let (target, draft, name, title, mode) = if new_pack {
            let id = loop {
                use rand::Rng;
                let id = format!("pack_{:016x}", rand::rng().random::<u64>());
                if !library.packs.iter().any(|p| p.id == id) {
                    break id;
                }
            };
            (
                EditorTarget::NewPack { id },
                CardDraft::default(),
                String::new(),
                tr!("新建知识包"),
                AnswerMode::Input,
            )
        } else {
            let Some(pack) = library.packs.get(self.selected_pack.get()) else {
                return;
            };
            let card = index
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| pack.cards.get(i));
            if index.is_some() && card.is_none() {
                return;
            }
            (
                EditorTarget::Card {
                    pack_id: pack.id.clone(),
                    card_id: card.and_then(|c| c.id.clone()),
                },
                card.map(CardDraft::from).unwrap_or_default(),
                pack.name.clone(),
                if card.is_some() {
                    tr!("编辑题目")
                } else {
                    tr!("新增题目")
                },
                card.map(|card| {
                    library.card_answer_mode(&pack.id, card.id.as_deref().unwrap_or_default())
                })
                .unwrap_or_default(),
            )
        };
        let mode = requested_mode.unwrap_or(mode);
        self.image_revision
            .set(self.image_revision.get().wrapping_add(1));
        let draft_image = draft.prompt_image.clone();
        window.set_editor_image_loading(self.image_task.borrow().is_some());
        self.math_fields.replace(vec![
            Field::new(&draft.prompt, &draft.prompt_latex),
            Field::new(&draft.answer, &draft.answer_latex),
            Field::new(&draft.choice_one, &draft.choice_one_latex),
            Field::new(&draft.choice_two, &draft.choice_two_latex),
            Field::new(&draft.statement, &draft.statement_latex),
            Field::new(&draft.correction, &draft.correction_latex),
        ]);
        self.math_pending.set(false);
        self.math_paths.borrow_mut().clear();
        window.set_math_active_field(-1);
        window.set_math_error("".into());
        window.set_current_tab(0);
        let show_aliases = !draft.aliases.is_empty();
        window.set_editor_new_pack(new_pack);
        window.set_editor_can_delete(matches!(
            &target,
            EditorTarget::Card {
                card_id: Some(_),
                ..
            }
        ));
        window.set_editor_title(title.into());
        window.set_editor_answer_mode(mode.index());
        window.set_editor_correct_choice(if draft.answer.trim().is_empty() {
            -1
        } else {
            0
        });
        window.set_editor_pack_name(name.into());
        window.set_editor_prompt(draft.prompt.into());
        window.set_editor_answer(draft.answer.into());
        window.set_editor_aliases(draft.aliases.into());
        window.set_editor_prompt_latex(draft.prompt_latex.into());
        window.set_editor_answer_latex(draft.answer_latex.into());
        window.set_editor_choice_one(draft.choice_one.into());
        window.set_editor_choice_two(draft.choice_two.into());
        window.set_editor_choice_one_latex(draft.choice_one_latex.into());
        window.set_editor_choice_two_latex(draft.choice_two_latex.into());
        window.set_editor_statement(draft.statement.into());
        window.set_editor_statement_latex(draft.statement_latex.into());
        window.set_editor_is_true(draft.is_true);
        window.set_editor_truth_choice(if draft.has_judgment {
            i32::from(!draft.is_true)
        } else {
            -1
        });
        window.set_editor_correction(draft.correction.into());
        window.set_editor_correction_latex(draft.correction_latex.into());
        window.set_editor_extra(show_aliases);
        window.set_editor_error("".into());
        window.set_message("".into());
        self.editor.replace(Some(target));
        window.set_editor_visible(true);
        window.invoke_reset_editor_scroll();
        self.seed_judgment(mode);
        self.render_editor_preview();
        if let Err(error) = self.set_editor_image(draft_image) {
            window.set_editor_error(error.into());
        }
    }

    pub(super) fn render_editor_preview(&self) {
        if self.editor.borrow().is_none() {
            return;
        }
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let fields = self.math_fields.borrow();
        let previews = fields
            .iter()
            .map(|field| {
                let mut preview = AnswerOption {
                    label: field.input().into(),
                    ..Default::default()
                };
                if !field.root.is_text()
                    && let Ok(rendered) = formula::render_node(
                        &field.root,
                        16,
                        window.get_pixel_scale(),
                        (232, 232, 232),
                    )
                {
                    preview = formula_presentation::option(preview.label, rendered);
                }
                preview
            })
            .collect::<Vec<_>>();
        if previews.len() >= 2 {
            window.set_editor_prompt_preview(previews[0].clone());
            window.set_editor_answer_preview(previews[1].clone());
        }
        window.set_editor_fields(ModelRc::new(VecModel::from(previews)));
        drop(fields);
        drop(main);
        self.refresh_math(false);
    }

    pub(super) fn queue_row_render(self: &Rc<Self>, index: i32) {
        if let Some(window) = self.main_window.borrow().as_ref()
            && let Ok(index) = usize::try_from(index)
            && let Some(row) = window.get_cards().row_data(index)
            && ((row.formula_source.is_empty() && row.answer_formula_source.is_empty())
                || (row.rendered && (row.formula_scale - window.get_pixel_scale()).abs() < 0.001))
        {
            return;
        }
        self.pending_rows.borrow_mut().insert(index);
        if self.row_render_timer.running() {
            return;
        }
        let weak = Rc::downgrade(self);
        self.row_render_timer
            .start(TimerMode::SingleShot, Duration::ZERO, move || {
                if let Some(app) = weak.upgrade() {
                    let rows = std::mem::take(&mut *app.pending_rows.borrow_mut());
                    for index in rows {
                        app.render_row(index);
                    }
                }
            });
    }

    pub(super) fn render_row(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let model = window.get_cards();
        let Some(model) = model.as_any().downcast_ref::<VecModel<CardRow>>() else {
            return;
        };
        let Some(mut row) = model.row_data(index) else {
            return;
        };
        if (row.formula_source.is_empty() && row.answer_formula_source.is_empty())
            || (row.rendered && (row.formula_scale - window.get_pixel_scale()).abs() < 0.001)
        {
            return;
        }
        row.rendered = true;
        row.formula_scale = window.get_pixel_scale();
        if !row.formula_source.is_empty() {
            row.preview = formula_presentation::content(
                &AnswerContent {
                    text: row.prompt.to_string(),
                    latex: Some(row.formula_source.to_string()),
                },
                12,
                window.get_pixel_scale(),
            );
        }
        if !row.answer_formula_source.is_empty() {
            row.answer_preview = formula_presentation::content(
                &AnswerContent {
                    text: row.answer.to_string(),
                    latex: Some(row.answer_formula_source.to_string()),
                },
                12,
                window.get_pixel_scale(),
            );
        }
        model.set_row_data(index, row);
        // Bound retained row bitmaps even when a large pack is scrolled through.
        let radius = ((window.get_question_area_height() / 48.0).ceil() as usize + 8).max(16);
        for old_index in 0..model.row_count() {
            if old_index.abs_diff(index) > radius
                && let Some(mut row) = model.row_data(old_index)
                && row.rendered
            {
                row.preview = AnswerOption::default();
                row.answer_preview = AnswerOption::default();
                row.rendered = false;
                model.set_row_data(old_index, row);
            }
        }
    }

    pub(super) fn activate_content(&self, index: i32) {
        if !self.apply_content_math() {
            return;
        }
        if !self.flush_math() {
            return;
        }
        if usize::try_from(index)
            .ok()
            .is_none_or(|i| i >= self.math_fields.borrow().len())
        {
            return;
        }
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_math_active_field(index);
        }
        self.refresh_math(true);
    }

    pub(super) fn flush_math(&self) -> bool {
        self.editor_preview_timer.stop();
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return false;
        };
        let Ok(index) = usize::try_from(window.get_math_active_field()) else {
            return true;
        };
        let mut fields = self.math_fields.borrow_mut();
        let Some(field) = fields.get_mut(index) else {
            return false;
        };
        if !self.math_pending.get() {
            return field.error().is_none();
        }
        match field.update(window.get_math_input().as_str()) {
            Ok(()) => {
                self.math_pending.set(false);
                window.set_math_error("".into());
                window.set_editor_error("".into());
                true
            }
            Err(error) => {
                window.set_math_error(error.into());
                false
            }
        }
    }

    pub(super) fn modify_math(&self, change: impl FnOnce(&mut Field) -> AppResult<()>) {
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let Ok(index) = usize::try_from(window.get_math_active_field()) else {
            return;
        };
        let mut fields = self.math_fields.borrow_mut();
        let Some(field) = fields.get_mut(index) else {
            return;
        };
        if let Err(error) = change(field) {
            window.set_math_error(error.into());
            return;
        }
        window.set_math_error("".into());
        drop(fields);
        drop(main);
        self.refresh_math(true);
    }

    pub(super) fn refresh_math(&self, reset_input: bool) {
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let Ok(index) = usize::try_from(window.get_math_active_field()) else {
            return;
        };
        let fields = self.math_fields.borrow();
        let Some(field) = fields.get(index) else {
            return;
        };
        if reset_input {
            window.set_math_input(field.input().into());
        }
        let (position, navigation) =
            editing_state(field, window.get_math_position().revision, reset_input);
        window.set_math_position(position);
        window.set_math_navigation(navigation);
        window.set_math_error(field.error().unwrap_or_default().into());
        window.set_math_text_mode(field.selected().accepts_prose());
        window.set_math_has_formula(!field.root.is_text());
        window.set_math_can_undo(field.can_undo());
        window.set_math_can_redo(field.can_redo());
        window.set_math_can_move_out(field.can_move_out());
        window.set_math_can_unwrap(field.can_unwrap());
        let (rows, cols) = field.grid_size().unwrap_or_default();
        window.set_math_grid_rows(rows as i32);
        window.set_math_grid_columns(cols as i32);
        if field.root.is_text() {
            window.set_math_regions(ModelRc::default());
            window.set_math_selection(MathRegion::default());
            self.math_paths.borrow_mut().clear();
            Self::publish_content_preview(
                window,
                index,
                AnswerOption {
                    label: field.root.value.clone().into(),
                    ..Default::default()
                },
            );
            return;
        }
        match formula::render_node(&field.root, 16, window.get_pixel_scale(), (232, 232, 232)) {
            Ok(rendered) => {
                let selected = rendered
                    .regions
                    .iter()
                    .filter(|region| region.path.starts_with(&field.selection))
                    .collect::<Vec<_>>();
                let selection = if field.selection.is_empty() {
                    MathRegion {
                        x: 0.0,
                        y: 0.0,
                        width: rendered.width,
                        height: rendered.height,
                    }
                } else if selected.is_empty() {
                    MathRegion::default()
                } else {
                    let left = selected.iter().map(|r| r.x).fold(f32::INFINITY, f32::min);
                    let top = selected.iter().map(|r| r.y).fold(f32::INFINITY, f32::min);
                    let right = selected.iter().map(|r| r.x + r.width).fold(0.0, f32::max);
                    let bottom = selected.iter().map(|r| r.y + r.height).fold(0.0, f32::max);
                    MathRegion {
                        x: left,
                        y: top,
                        width: right - left,
                        height: bottom - top,
                    }
                };
                window.set_math_selection(selection);
                window.set_math_regions(ModelRc::new(VecModel::from(
                    rendered
                        .regions
                        .iter()
                        .map(|region| MathRegion {
                            x: region.x,
                            y: region.y,
                            width: region.width,
                            height: region.height,
                        })
                        .collect::<Vec<_>>(),
                )));
                self.math_paths.replace(
                    rendered
                        .regions
                        .iter()
                        .map(|region| region.path.clone())
                        .collect(),
                );
                let preview = formula_presentation::option(field.root.plain().into(), rendered);
                Self::publish_content_preview(window, index, preview);
            }
            Err(error) => {
                window.set_math_preview(AnswerOption {
                    label: field.input().into(),
                    ..Default::default()
                });
                window.set_math_regions(ModelRc::default());
                window.set_math_selection(MathRegion::default());
                self.math_paths.borrow_mut().clear();
                window.set_math_error(error.into());
            }
        }
    }

    pub(super) fn publish_content_preview(
        window: &MainWindow,
        index: usize,
        preview: AnswerOption,
    ) {
        window.set_math_preview(preview.clone());
        let model = window.get_editor_fields();
        if let Some(model) = model.as_any().downcast_ref::<VecModel<AnswerOption>>() {
            model.set_row_data(index, preview.clone());
        }
        if index == 0 {
            window.set_editor_prompt_preview(preview.clone());
        }
        if index == 1 {
            window.set_editor_answer_preview(preview);
        }
    }

    pub(super) fn palette_model(tools: &[r#try::math_palette::Tool]) -> ModelRc<MathTool> {
        ModelRc::new(VecModel::from(
            tools
                .iter()
                .map(|tool| MathTool {
                    name: i18n::text(tool.name).into(),
                    command: tool.command.into(),
                })
                .collect::<Vec<_>>(),
        ))
    }

    pub(super) fn save_editor(self: &Rc<Self>) {
        if !self.apply_content_math() {
            return;
        }
        if self.image_task.borrow().is_some() {
            return;
        }
        if self.pending_card_delete.borrow().is_some() {
            return;
        }
        if !self.flush_math() {
            return;
        }
        let Some(window) = self
            .main_window
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
        else {
            return;
        };
        let Some(mode) = usize::try_from(window.get_editor_answer_mode())
            .ok()
            .and_then(|index| AnswerMode::ALL.get(index).copied())
        else {
            window.set_editor_error(tr!("请选择这道题的答题方式。").into());
            return;
        };
        if (mode == AnswerMode::MultipleChoice
            && !(0..3).contains(&window.get_editor_correct_choice()))
            || (mode == AnswerMode::TrueFalse
                && !(0..2).contains(&window.get_editor_truth_choice()))
        {
            window.set_editor_error(tr!("请选择正确答案。").into());
            return;
        }
        let old_answer = window.get_editor_answer();
        let old_latex = window.get_editor_answer_latex();
        for (index, field) in self.math_fields.borrow().iter().enumerate() {
            if !(field.is_dirty()
                || (mode == AnswerMode::TrueFalse
                    && index == 4
                    && window.get_editor_statement().trim().is_empty()))
            {
                continue;
            }
            let (text, tex) = match field.content() {
                Ok(content) => content,
                Err(error) => {
                    window.set_editor_error(error.into());
                    return;
                }
            };
            match index {
                0 => {
                    window.set_editor_prompt(text.into());
                    window.set_editor_prompt_latex(tex.into());
                }
                1 => {
                    window.set_editor_answer(text.into());
                    window.set_editor_answer_latex(tex.into());
                }
                2 => {
                    window.set_editor_choice_one(text.into());
                    window.set_editor_choice_one_latex(tex.into());
                }
                3 => {
                    window.set_editor_choice_two(text.into());
                    window.set_editor_choice_two_latex(tex.into());
                }
                4 => {
                    window.set_editor_statement(text.into());
                    window.set_editor_statement_latex(tex.into());
                }
                5 => {
                    window.set_editor_correction(text.into());
                    window.set_editor_correction_latex(tex.into());
                }
                _ => {}
            }
        }
        let mut draft = CardDraft {
            prompt_image: self.editor_image.borrow().clone(),
            prompt: window.get_editor_prompt().to_string(),
            answer: window.get_editor_answer().to_string(),
            aliases: window.get_editor_aliases().to_string(),
            prompt_latex: window.get_editor_prompt_latex().to_string(),
            answer_latex: window.get_editor_answer_latex().to_string(),
            choice_one: window.get_editor_choice_one().to_string(),
            choice_two: window.get_editor_choice_two().to_string(),
            choice_one_latex: window.get_editor_choice_one_latex().to_string(),
            choice_two_latex: window.get_editor_choice_two_latex().to_string(),
            has_judgment: window.get_editor_answer_mode() == AnswerMode::TrueFalse.index()
                || !window.get_editor_statement().trim().is_empty()
                || !window.get_editor_statement_latex().trim().is_empty()
                || !window.get_editor_correction().trim().is_empty()
                || !window.get_editor_correction_latex().trim().is_empty(),
            statement: window.get_editor_statement().to_string(),
            statement_latex: window.get_editor_statement_latex().to_string(),
            is_true: window.get_editor_is_true(),
            correction: window.get_editor_correction().to_string(),
            correction_latex: window.get_editor_correction_latex().to_string(),
        };
        if mode == AnswerMode::MultipleChoice
            && draft.answer.trim() != old_answer.trim()
            && !math_answer::matches(
                &draft.answer,
                old_answer.as_str(),
                (!old_latex.is_empty()).then_some(old_latex.as_str()),
                &[],
            )
        {
            draft.aliases.clear();
        }
        let card = draft.into_card_for_mode(
            mode,
            usize::try_from(window.get_editor_correct_choice()).unwrap_or_default(),
        );
        if let Some(id) = &card.prompt_image
            && let Err(error) = self.store.media().read(id)
        {
            window.set_editor_error(error.into());
            return;
        }
        let result = match self.editor.borrow().as_ref() {
            Some(EditorTarget::NewPack { id }) => {
                let pack = KnowledgePack {
                    format: "try".into(),
                    schema_version: 1,
                    id: id.clone(),
                    name: window.get_editor_pack_name().trim().to_owned(),
                    tag: String::new(),
                    description: String::new(),
                    cards: vec![card],
                };
                let result = self.commit(|library| {
                    library.import(pack, false)?;
                    let card_id = library.packs.last().unwrap().cards[0].id.clone().unwrap();
                    library.set_card_answer_mode(id, &card_id, mode)
                });
                if result.is_ok() {
                    self.selected_pack
                        .set(self.library.borrow().packs.len() - 1);
                    window.invoke_reset_overview();
                }
                result
            }
            Some(EditorTarget::Card { pack_id, card_id }) => self.commit(|library| {
                library.save_card_with_mode(pack_id, card_id.as_deref(), card, mode)
            }),
            None => return,
        };
        match result {
            Ok(()) => {
                self.close_editor();
                self.refresh();
            }
            Err(error) => {
                window.set_editor_error(error.into());
            }
        }
    }

    pub(super) fn close_editor(&self) {
        self.close_structured_math_for(false);
        self.image_revision
            .set(self.image_revision.get().wrapping_add(1));
        self.editor_image.borrow_mut().take();
        self.close_image_preview();
        self.editor_preview_timer.stop();
        self.pending_card_delete.borrow_mut().take();
        self.editor.borrow_mut().take();
        self.math_fields.borrow_mut().clear();
        self.math_paths.borrow_mut().clear();
        self.math_pending.set(false);
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_card_delete_pending(false);
            window.set_editor_can_delete(false);
            window.set_card_delete_prompt("".into());
            window.set_editor_visible(false);
            window.set_editor_picture(slint::Image::default());
            window.set_editor_picture_width(0.0);
            window.set_editor_picture_height(0.0);
            window.set_editor_has_image(false);
            window.set_editor_error("".into());
            window.set_editor_prompt_preview(AnswerOption::default());
            window.set_editor_answer_preview(AnswerOption::default());
            window.set_editor_fields(ModelRc::default());
            window.set_math_active_field(-1);
            window.set_math_preview(AnswerOption::default());
            window.set_math_regions(ModelRc::default());
        }
        self.collect_media();
    }
}

pub(super) fn editing_state(
    field: &Field,
    revision: i32,
    reset: bool,
) -> (FormulaPosition, FormulaNavigation) {
    let caret = field.text_selection();
    (
        FormulaPosition {
            anchor: caret.anchor as i32,
            caret: caret.focus as i32,
            bytes: field.input().len() as i32,
            revision: if reset {
                revision.wrapping_add(1)
            } else {
                revision
            },
        },
        FormulaNavigation {
            previous: field.can_navigate("previous"),
            next: field.can_navigate("next"),
            parent: field.can_navigate("parent"),
            exit: field.can_navigate("exit"),
            up: field.can_navigate("up"),
            down: field.can_navigate("down"),
            enter: field.has_trailing_structure(),
            container: !field.selected().children.is_empty() && !field.selected().accepts_prose(),
        },
    )
}
