//! Knowledge-pack participation, metadata, import/export and quick edits.
use super::*;

impl App {
    pub(super) fn save_pack_metadata(self: &Rc<Self>, field: PackMetadata) {
        let Some(window) = self
            .main_window
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
        else {
            return;
        };
        let id = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .map(|pack| pack.id.clone());
        let Some(id) = id else {
            return;
        };
        let result =
            match field {
                PackMetadata::Description => self.commit(|library| {
                    library.set_pack_description(&id, window.get_description_draft().as_str())
                }),
                PackMetadata::Name => self
                    .commit(|library| library.rename_pack(&id, window.get_name_draft().as_str())),
                PackMetadata::Tag => self
                    .commit(|library| library.set_pack_tag(&id, window.get_tag_draft().as_str())),
            };
        match result {
            Ok(()) => {
                window.set_metadata_error("".into());
                match field {
                    PackMetadata::Description => window.set_description_editing(false),
                    PackMetadata::Name => window.set_name_editing(false),
                    PackMetadata::Tag => window.set_tag_editing(false),
                }
            }
            Err(error) => window.set_metadata_error(error.into()),
        }
    }

    pub(super) fn set_pack_enabled(self: &Rc<Self>, id: &str, enabled: bool) {
        let result = self.commit(|library| library.set_pack_enabled(id, enabled));
        self.report(result, "");
    }

    pub(super) fn set_all_cards_enabled(self: &Rc<Self>, enabled: bool) {
        let id = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .map(|pack| pack.id.clone());
        if let Some(id) = id {
            let result = self.commit(|library| library.set_all_cards_enabled(&id, enabled));
            self.report(result, "");
        }
    }

    pub(super) fn set_card_enabled(self: &Rc<Self>, index: i32, enabled: bool) {
        let target = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .and_then(|pack| {
                usize::try_from(index)
                    .ok()
                    .and_then(|index| pack.cards.get(index))
                    .and_then(|card| card.id.clone())
                    .map(|id| (pack.id.clone(), id))
            });
        if let Some((pack_id, card_id)) = target {
            let result =
                self.commit(|library| library.set_card_enabled(&pack_id, &card_id, enabled));
            self.report(result, "");
        }
    }

    pub(super) fn set_card_answer_mode(self: &Rc<Self>, row: i32, index: i32) {
        let Some(mode) = usize::try_from(index)
            .ok()
            .and_then(|index| AnswerMode::ALL.get(index).copied())
        else {
            return;
        };
        let target = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .and_then(|pack| {
                usize::try_from(row)
                    .ok()
                    .and_then(|row| pack.cards.get(row))
                    .and_then(|card| card.id.clone().map(|id| (pack.id.clone(), id)))
            });
        if let Some((pack_id, card_id)) = target {
            let result =
                self.commit(|library| library.set_card_answer_mode(&pack_id, &card_id, mode));
            self.report(result, "");
        }
    }

    pub(super) fn quick_card(
        &self,
        pack_id: &str,
        card_id: &str,
    ) -> AppResult<(r#try::knowledge::Card, AnswerMode)> {
        let library = self.library.borrow();
        let card = library
            .packs
            .iter()
            .find(|pack| pack.id == pack_id)
            .and_then(|pack| {
                pack.cards
                    .iter()
                    .find(|card| card.id.as_deref() == Some(card_id))
            })
            .ok_or(tr!("这道题已不存在，请重新选择。"))?;
        Ok((card.clone(), library.card_answer_mode(pack_id, card_id)))
    }

    pub(super) fn save_quick_answer(
        self: &Rc<Self>,
        pack_id: &str,
        card_id: &str,
        input: &str,
    ) -> AppResult<()> {
        let (card, mode) = self.quick_card(pack_id, card_id)?;
        if mode != AnswerMode::Input {
            return Err(tr!("答题方式已改变，请重新选择答案。").into());
        }
        let updated = r#try::card_editor::with_answer(&card, input)?;
        if updated == card {
            return Ok(());
        }
        self.commit(|library| library.save_card_with_mode(pack_id, Some(card_id), updated, mode))
    }

    pub(super) fn choose_quick_answer(
        self: &Rc<Self>,
        pack_id: &str,
        card_id: &str,
        mode_index: i32,
        index: i32,
    ) -> AppResult<()> {
        let (card, mode) = self.quick_card(pack_id, card_id)?;
        if mode.index() != mode_index {
            return Err(tr!("答题方式已改变，请重新选择答案。").into());
        }
        let mut draft = CardDraft::from(&card);
        let updated = match mode {
            AnswerMode::MultipleChoice if (0..3).contains(&index) => {
                if index == 0 {
                    return Ok(());
                }
                draft.into_card_for_mode(mode, index as usize)
            }
            AnswerMode::TrueFalse if (0..2).contains(&index) => {
                let truth = index == 0;
                if truth == draft.is_true {
                    return Ok(());
                }
                draft.is_true = truth;
                draft.into_card_for_mode(mode, 0)
            }
            _ => return Err(tr!("请选择一个有效答案。").into()),
        };
        self.commit(|library| library.save_card_with_mode(pack_id, Some(card_id), updated, mode))
    }

    pub(super) fn prepare_quick_options(&self, pack_id: &str, card_id: &str) {
        let options = self
            .quick_card(pack_id, card_id)
            .ok()
            .filter(|(_, mode)| *mode == AnswerMode::MultipleChoice)
            .map(|(card, _)| {
                let scale = self
                    .main_window
                    .borrow()
                    .as_ref()
                    .map_or(1.0, MainWindow::get_pixel_scale);
                std::iter::once(AnswerContent {
                    text: card.answer,
                    latex: card.answer_latex,
                })
                .chain(card.distractors)
                .map(|option| formula_presentation::content(&option, 12, scale))
                .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_quick_options(ModelRc::new(VecModel::from(options)));
        }
    }

    pub(super) fn import_pack(self: &Rc<Self>) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!("导入 Try 知识包"))
            .add_filter(tr!("Try 知识包"), &["json"])
            .pick_file()
        else {
            return;
        };
        let imported = self.store.import_pack(&path);
        if imported.is_err() {
            self.collect_media();
        }
        match imported {
            Ok(pack) => {
                let duplicate = self
                    .library
                    .borrow()
                    .packs
                    .iter()
                    .any(|item| item.id == pack.id);
                if duplicate {
                    self.pending_import.replace(Some(pack));
                    self.refresh();
                    self.notify("", false);
                } else {
                    let id = pack.id.clone();
                    let result = self.commit(|library| library.import(pack, false));
                    if result.is_ok() {
                        let index = self
                            .library
                            .borrow()
                            .packs
                            .iter()
                            .position(|pack| pack.id == id)
                            .unwrap_or(0);
                        self.selected_pack.set(index);
                        if let Some(window) = self.main_window.borrow().as_ref() {
                            window.invoke_reset_overview();
                        }
                        self.refresh();
                    }
                    self.report(result, tr!("已导入。"));
                }
            }
            Err(error) => self.notify(&error, true),
        }
    }

    pub(super) fn confirm_import(self: &Rc<Self>) {
        let Some(pack) = self.pending_import.borrow_mut().take() else {
            return;
        };
        let replacement = pack.clone();
        let result = self.commit(|library| library.import(pack, true));
        if result.is_err() {
            self.pending_import.replace(Some(replacement));
        }
        self.refresh();
        self.report(result, tr!("已替换。"));
    }

    pub(super) fn request_delete_card(&self) {
        let key = match self.editor.borrow().as_ref() {
            Some(EditorTarget::Card {
                pack_id,
                card_id: Some(card_id),
            }) => QuestionKey {
                pack_id: pack_id.clone(),
                card_id: card_id.clone(),
            },
            _ => return,
        };
        if self.pending_card_delete.borrow().is_some() {
            return;
        }
        self.editor_preview_timer.stop();
        let prompt = self
            .library
            .borrow()
            .packs
            .iter()
            .find(|pack| pack.id == key.pack_id)
            .and_then(|pack| {
                pack.cards
                    .iter()
                    .enumerate()
                    .find(|(_, card)| card.id.as_deref() == Some(&key.card_id))
            })
            .map(|(index, card)| {
                if card.prompt.trim().is_empty() {
                    tr!("图片题 {p0}", p0 = index + 1)
                } else {
                    card.prompt.clone()
                }
            })
            .unwrap_or_default();
        self.pending_card_delete.replace(Some(key));
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_card_delete_prompt(prompt.into());
            window.set_card_delete_pending(true);
            window.set_editor_error("".into());
            window.set_message("".into());
        }
    }

    pub(super) fn cancel_delete_card(&self) {
        self.pending_card_delete.borrow_mut().take();
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_card_delete_pending(false);
            window.set_card_delete_prompt("".into());
            window.set_editor_error("".into());
            window.set_message("".into());
        }
        self.render_editor_preview();
    }

    pub(super) fn confirm_delete_card(self: &Rc<Self>) {
        let Some(key) = self.pending_card_delete.borrow().clone() else {
            return;
        };
        let result = self.commit(|library| library.delete_card(&key.pack_id, &key.card_id));
        if result.is_ok() {
            if self.previous_question.borrow().as_ref() == Some(&key) {
                self.previous_question.borrow_mut().take();
            }
            self.close_editor();
            log::info!(target: "try::app", "event=card_deleted remaining_cards={}", self.library.borrow().packs.iter().map(|pack| pack.cards.len()).sum::<usize>());
        }
        self.refresh();
        self.report(result, "");
    }

    pub(super) fn request_delete_pack(&self) {
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        if !window.get_tray_settings_available() {
            return;
        }
        let id = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .map(|pack| pack.id.clone());
        let Some(id) = id else { return };
        window.set_current_tab(0);
        self.pending_delete.replace(Some(id));
        self.notify("", false);
        self.refresh();
    }

    pub(super) fn confirm_delete_pack(self: &Rc<Self>) {
        // Keep the stable pack id and confirmation until the atomic save succeeds.
        let Some(id) = self.pending_delete.borrow().clone() else {
            return;
        };
        let result = self.commit(|library| library.delete_pack(&id));
        if result.is_ok() {
            self.pending_delete.borrow_mut().take();
            if self
                .previous_question
                .borrow()
                .as_ref()
                .is_some_and(|key| key.pack_id == id)
            {
                self.previous_question.borrow_mut().take();
            }
            if let Some(window) = self.main_window.borrow().as_ref() {
                window.invoke_reset_overview();
                window.set_question_scroll_y(0.0);
                window.set_quick_options(ModelRc::default());
            }
            log::info!(target: "try::app", "event=pack_deleted remaining_packs={}", self.library.borrow().packs.len());
        }
        self.refresh();
        self.report(result, "");
    }

    pub(super) fn export_pack(&self) {
        let pack = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .cloned();
        let Some(pack) = pack else { return };
        let Some(mut path) = rfd::FileDialog::new()
            .set_title(tr!("导出 Try 知识包"))
            .set_file_name(format!("{}.json", pack.id))
            .add_filter(tr!("Try 知识包"), &["json"])
            .save_file()
        else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension("json");
        }
        self.report(
            self.store.export_portable_pack(&path, &pack),
            tr!("已导出。"),
        );
    }
}
