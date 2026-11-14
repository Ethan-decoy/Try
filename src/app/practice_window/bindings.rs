//! Slint callbacks and native input dispatch for the current practice window.
use super::*;

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
        let right_pressed = Cell::new(false);
        window.window().on_winit_window_event(move |window, event| {
            crate::window_repaint::prepare(window, event);
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
                let weak_window = weak_window.clone();
                let ready = reshape_ready.clone();
                let floating = floating.clone();
                Timer::single_shot(Duration::ZERO, move || {
                    if !ready.get() {
                        return;
                    }
                    if let Some(window) = weak_window.upgrade() {
                        if reshape {
                            floating.reshape_and_clamp(window.window());
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
                app.close_practice();
            }
        });
        let weak = Rc::downgrade(self);
        window.window().on_close_requested(move || {
            let weak = weak.clone();
            Timer::single_shot(Duration::ZERO, move || {
                if let Some(app) = weak.upgrade() {
                    app.close_practice();
                }
            });
            slint::CloseRequestResponse::HideWindow
        });
        native_ready
    }
}
