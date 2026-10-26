//! Project application state into the tray and main-window display models.
use super::super::*;

impl App {
    pub(in crate::app) fn refresh(&self) {
        self.refresh_update_status();
        self.refresh_tray_navigation();
        let library = self.library.borrow();
        if let Some(session) = self.practice.borrow().as_ref() {
            let dark = self.resolved_dark_theme(library.settings.appearance);
            if session.window.get_dark_theme() != dark {
                session.window.set_dark_theme(dark);
                self.floating.apply_theme(session.window.window(), dark);
            }
        }
        let count = library.enabled_card_count();
        let now = chrono::Local::now();
        let pause_minutes = library.settings.pause.minutes(now.timestamp());
        let pause_end = match &library.settings.pause {
            PracticePause::Timed { until, .. } if pause_minutes > 0 => {
                let end = chrono::DateTime::from_timestamp(*until, 0)
                    .expect("validated pause")
                    .with_timezone(&chrono::Local);
                let pattern = if end.date_naive() == now.date_naive() {
                    "%H:%M"
                } else {
                    "%m-%d %H:%M"
                };
                Some(end.format(pattern).to_string())
            }
            _ => None,
        };
        self.tray.set_reminders_enabled(pause_minutes == 0);
        self.tray.set_can_practice(count > 0);
        self.tray.set_pause_minutes(pause_minutes);
        self.tray.set_pause_title(match &pause_end {
            Some(end) => tr!("暂离至 {p0}", p0 = end).into(),
            _ => tr!("暂离").into(),
        });
        let window = self.main_window.borrow();
        let Some(window) = window.as_ref() else {
            return;
        };
        self.refresh_library_view(window, &library);
        self.refresh_settings_view(window, &library, count, pause_minutes, &pause_end);
        self.refresh_pending_actions(window, &library);
    }

    fn refresh_library_view(&self, window: &MainWindow, library: &Library) {
        let rows = library
            .packs
            .iter()
            .map(|pack| {
                let participating = library.pack_enabled_card_count(pack);
                PackRow {
                    id: pack.id.clone().into(),
                    name: pack.name.clone().into(),
                    tag: pack.tag.trim().into(),
                    participating: library.is_enabled(&pack.id),
                    has_cards: !pack.cards.is_empty(),
                    detail: if participating > 0 {
                        tr!("{p0} / {p1} 题", p0 = participating, p1 = pack.cards.len())
                    } else {
                        tr!("{p0} 题 · 未参与", p0 = pack.cards.len())
                    }
                    .into(),
                }
            })
            .collect::<Vec<_>>();
        let packs = window.get_packs();
        if let Some(model) = packs.as_any().downcast_ref::<VecModel<PackRow>>()
            && model.row_count() == rows.len()
        {
            for (index, row) in rows.into_iter().enumerate() {
                if model.row_data(index).as_ref() != Some(&row) {
                    model.set_row_data(index, row);
                }
            }
        } else {
            window.set_packs(ModelRc::new(VecModel::from(rows)));
        }
        window.set_participation_summary(
            tr!(
                "{p0} 个包 · {p1} 道题",
                p0 = library
                    .packs
                    .iter()
                    .filter(|pack| library.pack_enabled_card_count(pack) > 0)
                    .count(),
                p1 = library.enabled_card_count()
            )
            .into(),
        );
        let index = self
            .selected_pack
            .get()
            .min(library.packs.len().saturating_sub(1));
        self.selected_pack.set(index);
        window.set_selected_index(index as i32);
        window.set_has_selection(!library.packs.is_empty());
        if let Some(pack) = library.packs.get(index) {
            window.set_selected_name(pack.name.clone().into());
            window.set_selected_description(pack.description.clone().into());
            window.set_selected_description_summary(
                pack.description
                    .lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .unwrap_or_default()
                    .into(),
            );
            window.set_selected_tag(pack.tag.trim().into());
            let enabled = library.pack_enabled_card_count(pack);
            window.set_selected_enabled(!pack.cards.is_empty() && enabled == pack.cards.len());
            window.set_selection_partial(enabled > 0 && enabled < pack.cards.len());
            window.set_selected_card_count(
                tr!(
                    "{total} 道题 · {enabled} 道参与",
                    total = pack.cards.len(),
                    enabled = enabled
                )
                .into(),
            );
            let existing = window.get_cards();
            let card_rows = pack
                .cards
                .iter()
                .enumerate()
                .map(|(index, card)| {
                    let mode =
                        library.card_answer_mode(&pack.id, card.id.as_deref().unwrap_or_default());
                    let display = if self.knowledge_only.get() {
                        PreparedQuestion::knowledge(card)
                    } else {
                        let display_mode = if mode == AnswerMode::TrueFalse {
                            mode
                        } else {
                            AnswerMode::Input
                        };
                        PreparedQuestion::new(card, display_mode, &mut rand::rng())
                            .expect("the selected mode has validated content")
                    };
                    let prompt = if display.has_prompt() {
                        display.prompt.text
                    } else {
                        tr!("图片题 {p0}", p0 = index + 1)
                    };
                    let source = display.prompt.latex.unwrap_or_default();
                    let answer = if mode == AnswerMode::TrueFalse {
                        if card
                            .true_false
                            .as_ref()
                            .is_some_and(|statement| statement.is_true)
                        {
                            "✓".into()
                        } else {
                            "✕".into()
                        }
                    } else {
                        card.answer.clone()
                    };
                    let answer_source = if mode == AnswerMode::TrueFalse {
                        String::new()
                    } else {
                        card.answer_latex.clone().unwrap_or_default()
                    };
                    let cached = existing.row_data(index).filter(|row| {
                        row.prompt.as_str() == prompt
                            && row.formula_source.as_str() == source
                            && row.answer.as_str() == answer
                            && row.answer_formula_source.as_str() == answer_source
                    });
                    CardRow {
                        id: card.id.clone().unwrap_or_default().into(),
                        pack_id: pack.id.clone().into(),
                        prompt: prompt.into(),
                        quick_answer: card.answer.clone().into(),
                        truth: card
                            .true_false
                            .as_ref()
                            .is_some_and(|statement| statement.is_true),
                        answer: answer.into(),
                        answer_formula_source: answer_source.into(),
                        answer_preview: cached
                            .as_ref()
                            .map(|row| row.answer_preview.clone())
                            .unwrap_or_default(),
                        choice_available: card.supports(AnswerMode::MultipleChoice),
                        judgment_available: card.supports(AnswerMode::TrueFalse),
                        enabled: library
                            .is_card_enabled(&pack.id, card.id.as_deref().unwrap_or_default()),
                        answer_mode: library
                            .card_answer_mode(&pack.id, card.id.as_deref().unwrap_or_default())
                            .index(),
                        formula_source: source.into(),
                        rendered: cached.as_ref().is_some_and(|row| row.rendered),
                        formula_scale: cached.as_ref().map_or(0.0, |row| row.formula_scale),
                        preview: cached.map(|row| row.preview).unwrap_or_default(),
                    }
                })
                .collect::<Vec<_>>();
            let model = window.get_cards();
            if let Some(model) = model.as_any().downcast_ref::<VecModel<CardRow>>()
                && model.row_count() == card_rows.len()
            {
                // Keep the same row components and keyboard focus when a box changes.
                for (index, row) in card_rows.into_iter().enumerate() {
                    if model.row_data(index).as_ref() != Some(&row) {
                        model.set_row_data(index, row);
                    }
                }
            } else {
                window.set_cards(ModelRc::new(VecModel::from(card_rows)));
            }
            window.set_uniform_answer_mode(
                library
                    .uniform_answer_mode(&pack.id)
                    .map_or(-1, AnswerMode::index),
            );
            window.set_all_choice_available(pack.supports(AnswerMode::MultipleChoice));
            window.set_all_judgment_available(pack.supports(AnswerMode::TrueFalse));
        } else {
            window.set_selected_name("".into());
            window.set_selected_description("".into());
            window.set_selected_description_summary("".into());
            window.set_selected_tag("".into());
            window.set_selected_card_count("".into());
            window.set_selected_enabled(false);
            window.set_selection_partial(false);
            window.set_cards(ModelRc::default());
            window.set_quick_options(ModelRc::default());
            window.set_uniform_answer_mode(-1);
            window.set_all_choice_available(false);
            window.set_all_judgment_available(false);
        }
        window.set_knowledge_only(self.knowledge_only.get());
    }

    fn refresh_settings_view(
        &self,
        window: &MainWindow,
        library: &Library,
        count: usize,
        pause_minutes: i32,
        pause_end: &Option<String>,
    ) {
        let dark = self.resolved_dark_theme(library.settings.appearance);
        if window.get_dark_theme() != dark {
            window.set_dark_theme(dark);
            self.floating.apply_theme(window.window(), dark);
        }
        window.set_appearance(library.settings.appearance.index());
        window.set_interface_language(library.settings.interface_language.index());
        window.set_interval_minutes(library.settings.interval_minutes as i32);
        let (options, index) = preset_options(
            library.settings.interval_minutes as i32,
            &[15, 25, 30, 45],
            |n| tr!("{n} 分钟", n = n),
        );
        window.set_interval_options(options);
        window.set_interval_selection(index);
        window.set_feedback_seconds(library.settings.feedback_seconds as i32);
        let (options, index) =
            preset_options(library.settings.feedback_seconds as i32, &[1, 2, 3], |n| {
                tr!("{n} 秒", n = n)
            });
        window.set_feedback_options(options);
        window.set_feedback_selection(index);
        window.set_knowledge_seconds(library.settings.knowledge_seconds as i32);
        let (options, index) = preset_options(
            library.settings.knowledge_seconds as i32,
            &[5, 8, 15, 30],
            |n| tr!("{n} 秒", n = n),
        );
        window.set_knowledge_options(options);
        window.set_knowledge_selection(index);
        let (options, index) = preset_options(pause_minutes, &[0, 60, 120, 180, -1], |n| match n {
            0 => tr!("不暂停").into(),
            -1 => tr!("直到恢复").into(),
            60 | 120 | 180 => tr!("{p0} 小时", p0 = n / 60),
            _ => tr!("{n} 分钟", n = n),
        });
        window.set_pause_options(options);
        window.set_pause_selection(index);
        window.set_pause_minutes(pause_minutes);
        window.set_pause_detail(match pause_end {
            Some(end) => tr!("至 {p0}", p0 = end).into(),
            None if pause_minutes == -1 => tr!("等待恢复").into(),
            _ => "".into(),
        });
        window.set_autostart_available(self.autostart.borrow().is_ok());
        window.set_autostart_enabled(self.autostart_enabled.get());
        window.set_autostart_error(self.autostart_error.borrow().clone().into());
        let quiet = &library.settings.quiet_hours;
        window.set_quiet_hours_enabled(quiet.enabled);
        window.set_quiet_start(format_time(quiet.start_minute).into());
        window.set_quiet_end(format_time(quiet.end_minute).into());
        window.set_quiet_summary(
            format!(
                "{}–{}{}",
                format_time(quiet.start_minute),
                if quiet.start_minute > quiet.end_minute {
                    tr!("次日 ")
                } else {
                    ""
                },
                format_time(quiet.end_minute)
            )
            .into(),
        );
        window.set_quiet_time_error("".into());
        window.set_popup_corner(library.settings.popup_corner.index());
        window.set_remember_position(library.settings.remember_position);
        window.set_show_pack_tags(library.settings.show_pack_tags);
        window.set_focus_on_reminder(library.settings.focus_on_reminder);
        window.set_auto_check_updates(library.settings.auto_check_updates);
        window.set_has_saved_position(library.settings.popup_position.is_some());
        window.set_practice_summary(if count == 0 {
            tr!("0 题").into()
        } else if pause_minutes == 0 {
            tr!(
                "{count} 题 · {p0} 分钟",
                p0 = library.settings.interval_minutes,
                count = count
            )
            .into()
        } else {
            tr!("{count} 题 · 暂停", count = count).into()
        });
    }

    fn refresh_pending_actions(&self, window: &MainWindow, library: &Library) {
        let pending = self.pending_import.borrow();
        window.set_import_pending(pending.is_some());
        if let Some(pack) = pending.as_ref() {
            let original = library.packs.iter().find(|item| item.id == pack.id);
            let original_description = original
                .map(|item| tr!("“{p0}”（{p1} 道题）", p0 = item.name, p1 = item.cards.len()))
                .unwrap_or_else(|| pack.id.clone());
            window.set_import_description(
                tr!(
                    "已存在{original_description}。将用导入的“{p0}”（{p1} 道题）替换原内容。",
                    p0 = pack.name,
                    p1 = pack.cards.len(),
                    original_description = original_description
                )
                .into(),
            );
        }
        let pending_delete = self.pending_delete.borrow();
        window.set_delete_pending(pending_delete.is_some());
        window.set_delete_description(
            pending_delete
                .as_ref()
                .and_then(|id| library.packs.iter().find(|pack| &pack.id == id))
                .map(|pack| {
                    tr!(
                        "删除“{name}”及其中的 {count} 道题？此操作无法撤销。",
                        name = pack.name,
                        count = pack.cards.len()
                    )
                })
                .unwrap_or_default()
                .into(),
        );
    }
}
