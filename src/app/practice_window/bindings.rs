//! Slint callbacks and native input dispatch for the current practice window.
use super::*;
use slint::winit_030::winit::event::{ElementState, Ime, KeyEvent};
use slint::winit_030::winit::keyboard::{Key, ModifiersState, NamedKey};

fn starts_answer_input(event: &WindowEvent, modifiers: ModifiersState) -> bool {
    match event {
        WindowEvent::KeyboardInput {
            event,
            is_synthetic: false,
            ..
        } if event.state == ElementState::Pressed => {
            let paste = (modifiers.control_key() || modifiers.super_key())
                && !modifiers.alt_key()
                && matches!(&event.logical_key, Key::Character(key) if key.eq_ignore_ascii_case("v"));
            let text = !modifiers.super_key()
                && (!modifiers.control_key() || modifiers.alt_key())
                && event
                    .text
                    .as_ref()
                    .is_some_and(|text| text.chars().any(|character| !character.is_control()));
            paste || text
        }
        WindowEvent::Ime(Ime::Preedit(text, _) | Ime::Commit(text)) => !text.is_empty(),
        _ => false,
    }
}

fn is_card_shortcut(event: &WindowEvent, modifiers: ModifiersState) -> bool {
    let WindowEvent::KeyboardInput {
        event,
        is_synthetic: false,
        ..
    } = event
    else {
        return false;
    };
    event.state == ElementState::Pressed
        && (matches!(
            event.logical_key,
            Key::Named(NamedKey::Escape | NamedKey::ContextMenu)
        ) || (event.logical_key == Key::Named(NamedKey::F10) && modifiers.shift_key()))
}

impl App {
    pub(in crate::app) fn bind_practice_window(
        self: &Rc<Self>,
        window: &PracticeWindow,
        diagnostic_id: u64,
        awaiting_interaction: bool,
    ) -> Rc<Cell<bool>> {
        let weak = Rc::downgrade(self);
        window.on_preview_question_image(move || {
            if let Some(app) = weak.upgrade() {
                let image = app
                    .practice
                    .borrow()
                    .as_ref()
                    .map(|s| s.window.get_question_image());
                if let Some(image) = image {
                    app.preview_image(image);
                }
            }
        });
        let weak = Rc::downgrade(self);
        window.on_rasterize_formulas(move |scale| {
            if let Some(app) = weak.upgrade() {
                if let Some(session) = app.practice.borrow().as_ref() {
                    // This binding tracks asynchronous monitor DPI updates.
                    formula_presentation::apply_practice(&session.window, &session.prepared, scale);
                }
                app.refresh_answer_preview();
            }
        });
        let weak_window = window.as_weak();
        let weak_app = Rc::downgrade(self);
        let native_ready = Rc::new(Cell::new(!awaiting_interaction));
        let reshape_ready = native_ready.clone();
        let applied_theme = Cell::new(None);
        let floating = self.floating.clone();
        let pointer = Cell::new((0.0_f32, 0.0_f32));
        let modifiers = Cell::new(ModifiersState::empty());
        let right_pressed = Cell::new(false);
        let first_frame = Cell::new(true);
        let shape_window = window.as_weak();
        #[cfg(target_os = "windows")]
        crate::floating_window::card_ripple::bind(window);
        window.on_shape_frame(move |scale| {
            if let Some(card) = shape_window.upgrade() {
                FloatingWindow::set_frame_scale(card.window(), scale);
            }
        });
        window.window().on_winit_window_event(move |window, event| {
            crate::window_repaint::prepare(window, event);
            if let WindowEvent::ModifiersChanged(state) = event {
                modifiers.set(state.state());
            }
            if matches!(
                event,
                WindowEvent::KeyboardInput {
                    is_synthetic: false,
                    event: KeyEvent {
                        state: ElementState::Pressed,
                        ..
                    },
                    ..
                } | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::Touch(_)
                    | WindowEvent::Ime(Ime::Preedit(..) | Ime::Commit(_))
            ) && let Some(card) = weak_window.upgrade()
            {
                if card.get_dismissing() {
                    return EventResult::PreventDefault;
                }
                first_frame.set(false);
                // The card handles keyboard dismissal before interrupting its
                // entrance, so an early Escape can reverse from the current scale.
                if !matches!(event, WindowEvent::KeyboardInput { .. }) {
                    card.invoke_interrupt_entrance();
                }
                // Reveal the existing TextInput before Slint handles the
                // original key or composition event, including its first character.
                if starts_answer_input(event, modifiers.get()) {
                    card.invoke_resume_answer_input();
                } else if is_card_shortcut(event, modifiers.get()) {
                    card.invoke_prepare_card_shortcut();
                }
            }
            if matches!(
                event,
                WindowEvent::Focused(true)
                    | WindowEvent::MouseInput {
                        state: slint::winit_030::winit::event::ElementState::Pressed,
                        ..
                    }
            ) && let Some(app) = weak_app.upgrade()
            {
                // Start before Slint dispatches the click, so the first click
                // can answer a choice card as well as activate the window.
                app.begin_practice_interaction(diagnostic_id, LearningOrigin::Reminder);
            }
            // Capture an explicit right click before child inputs or picture
            // controls can consume it. Left clicks, typing and dragging keep
            // their normal Slint/SMI dispatch, including passive-card guards.
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    let scale = f64::from(window.scale_factor());
                    pointer.set(((position.x / scale) as f32, (position.y / scale) as f32));
                }
                WindowEvent::CursorLeft { .. } => right_pressed.set(false),
                WindowEvent::MouseInput {
                    button: slint::winit_030::winit::event::MouseButton::Right,
                    state,
                    ..
                } => {
                    if *state == slint::winit_030::winit::event::ElementState::Pressed {
                        right_pressed.set(true);
                    } else if right_pressed.replace(false)
                        && let Some(window) = weak_window.upgrade()
                    {
                        let (x, y) = pointer.get();
                        FloatingWindow::activate(window.window());
                        window.invoke_open_context_menu(x, y);
                    }
                    return EventResult::PreventDefault;
                }
                _ => {}
            }
            if matches!(
                event,
                WindowEvent::RedrawRequested | WindowEvent::ThemeChanged(_)
            ) && let Some(app) = weak_app.upgrade()
            {
                app.apply_native_theme(
                    window,
                    &applied_theme,
                    matches!(event, WindowEvent::ThemeChanged(_)),
                );
                if matches!(event, WindowEvent::RedrawRequested)
                    && reshape_ready.get()
                    && first_frame.replace(false)
                    && let Some(card) = weak_window.upgrade()
                    && !card.get_dismissing()
                {
                    card.animate_visibility(false);
                }
            }
            match event {
                WindowEvent::Resized(size) => log::debug!(target: "try::app",
                    "event=practice_resized card={diagnostic_id} width={} height={}",
                    size.width,
                    size.height
                ),
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    log::debug!(target: "try::app",
                        "event=practice_scale_changed card={diagnostic_id} scale={scale_factor}"
                    )
                }
                WindowEvent::Focused(focused) => log::debug!(target: "try::app",
                    "event=practice_focus_changed card={diagnostic_id} focused={focused}"
                ),
                WindowEvent::Moved(position) => log::trace!(target: "try::app",
                    "event=practice_moved card={diagnostic_id} x={} y={}",
                    position.x,
                    position.y
                ),
                _ => {}
            }
            if matches!(
                event,
                WindowEvent::Resized(_)
                    | WindowEvent::ScaleFactorChanged { .. }
                    | WindowEvent::Moved(_)
            ) {
                let reshape = !matches!(event, WindowEvent::Moved(_));
                let manual_resize = weak_window
                    .upgrade()
                    .is_some_and(|card| card.get_resizing());
                let weak_window = weak_window.clone();
                let ready = reshape_ready.clone();
                let floating = floating.clone();
                Timer::single_shot(Duration::ZERO, move || {
                    if !ready.get() {
                        return;
                    }
                    if let Some(window) = weak_window.upgrade() {
                        if reshape && !manual_resize && !window.get_resizing() {
                            floating.reshape_and_clamp(window.window(), window.get_frame_scale());
                        }
                        if let Some(on_right) = FloatingWindow::tag_on_right(window.window()) {
                            window.set_tag_on_right(on_right);
                        }
                    }
                });
            }
            EventResult::Propagate
        });
        let weak = Rc::downgrade(self);
        window.on_answer_edited(move |input| {
            if let Some(app) = weak.upgrade() {
                app.edit_answer_input(input.as_str());
            }
        });
        let weak = Rc::downgrade(self);
        self.bind_structured_math(window.global::<crate::MathInputState>());
        self.bind_practice_menu(window, diagnostic_id);
        window.on_open_structured_entry(move || {
            if let Some(app) = weak.upgrade() {
                app.open_practice_math();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_answer_position_changed(move |input, anchor, caret| {
            if let Some(app) = weak.upgrade()
                && let Some(session) = app.practice.borrow().as_ref()
                && session.window.get_answer_text() == input
            {
                sync_answer_position(session, anchor, caret, false);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_input_command(move |command| {
            if let Some(app) = weak.upgrade() {
                app.inline_answer_command(command.as_str());
            }
        });
        let weak = Rc::downgrade(self);
        window.on_submit_answer(move |input| {
            if let Some(app) = weak.upgrade() {
                app.submit_answer(input.as_str());
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_answer(move |index| {
            if let Some(app) = weak.upgrade() {
                app.choose_answer(index);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_dismiss(move || {
            if let Some(app) = weak.upgrade() {
                app.dismiss_practice(diagnostic_id);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_dismiss_finished(move || {
            if let Some(app) = weak.upgrade()
                && let Some((_, window)) = app
                    .outgoing_practice
                    .borrow_mut()
                    .take_if(|(id, _)| *id == diagnostic_id)
            {
                let _ = window.hide();
            }
        });
        let weak = Rc::downgrade(self);
        window.window().on_close_requested(move || {
            let weak = weak.clone();
            Timer::single_shot(Duration::ZERO, move || {
                if let Some(app) = weak.upgrade() {
                    app.dismiss_practice(diagnostic_id);
                }
            });
            slint::CloseRequestResponse::KeepWindowShown
        });
        native_ready
    }

    #[cfg(target_os = "windows")]
    pub(super) fn watch_practice_resize(
        self: &Rc<Self>,
        window: &PracticeWindow,
        id: u64,
        native: &slint::winit_030::winit::window::Window,
    ) {
        use crate::floating_window::practice_resize::{self, ResizeEvent};
        let weak_window = window.as_weak();
        let weak_app = Rc::downgrade(self);
        let lock_window = weak_window.clone();
        let is_locked = move || {
            lock_window
                .upgrade()
                .is_none_or(|card| card.get_card_locked())
        };
        let installed = practice_resize::install(native, is_locked, move |event| {
            match event {
                ResizeEvent::Started => {
                    if let (Some(window), Some(app)) = (weak_window.upgrade(), weak_app.upgrade()) {
                        // Note any earlier move before the dragged edge changes the origin.
                        let _ = app.floating.moved_position(window.window());
                        window.invoke_interrupt_entrance();
                        window.set_resizing(true);
                        app.update_card_close_hold(id);
                    }
                }
                ResizeEvent::Finished(size) => {
                    // Persist after the native sizing loop exits and its final
                    // Resized event has reached the Slint window.
                    let weak_window = weak_window.clone();
                    let weak_app = weak_app.clone();
                    Timer::single_shot(Duration::ZERO, move || {
                        let (Some(window), Some(app)) = (weak_window.upgrade(), weak_app.upgrade())
                        else {
                            return;
                        };
                        if let Some(size) = size
                            && app
                                .practice
                                .borrow()
                                .as_ref()
                                .is_some_and(|session| session.diagnostic_id == id)
                        {
                            app.floating.keep_resized_position(window.window());
                            window.set_card_width(size.width as f32);
                            window.set_card_height(size.height as f32);
                            let result = app.save_library_edit(|library| {
                                library.settings.practice_card_size = Some(size);
                                Ok(())
                            });
                            if result.is_err() {
                                app.report(result, "");
                            }
                        }
                        window.set_resizing(false);
                        FloatingWindow::set_frame_scale(window.window(), window.get_frame_scale());
                        app.update_card_close_hold(id);
                    });
                }
            }
        });
        window.set_resize_available(installed);
    }
}
