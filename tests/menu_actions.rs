//! Exercise the actual popup/repeater/button chain without a desktop session.
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{
    Key, Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
};
use slint::{ComponentHandle, LogicalPosition, PhysicalSize, Rgb8Pixel, SharedPixelBuffer};
use std::rc::Rc;

slint::slint! {
    import { ActionMenu, DesktopButton } from "../ui/desktop_controls.slint";
    export component MenuActionsProbe inherits Window {
        width: 360px; height: 300px;
        background: #202124;
        in-out property <int> last-action: -1;
        in-out property <int> action-count: 0;
        DesktopButton {
            x: 24px; y: 24px; width: 88px; height: 32px;
            text: "Direct";
            activated => { root.last-action = 99; }
        }
        ActionMenu {
            x: 296px; y: 24px; width: 32px; height: 32px;
            label: "Pack actions";
            options: ["Rename", "Tag", "Description", "Export", "Delete"];
            destructive-last: true;
            choose(action) => { root.last-action = action; root.action-count += 1; }
        }
    }
}

struct HeadlessPlatform(Rc<MinimalSoftwareWindow>);

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

fn paint(adapter: &MinimalSoftwareWindow) {
    slint::platform::update_timers_and_animations();
    adapter.request_redraw();
    adapter.draw_if_needed(|renderer| {
        let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(360, 300);
        renderer.render(pixels.make_mut_slice(), 360);
    });
}

fn click(probe: &MenuActionsProbe, adapter: &MinimalSoftwareWindow, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    probe
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    probe.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    probe.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    paint(adapter);
}

fn key(probe: &MenuActionsProbe, adapter: &MinimalSoftwareWindow, text: slint::SharedString) {
    probe
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    probe
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
    paint(adapter);
}

#[test]
fn mouse_popup_actions_dispatch_the_clicked_item_once() {
    let adapter = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(HeadlessPlatform(adapter.clone()))).unwrap();
    let probe = MenuActionsProbe::new().unwrap();
    adapter.set_size(PhysicalSize::new(360, 300));
    probe.show().unwrap();
    paint(&adapter);

    click(&probe, &adapter, 68.0, 40.0);
    assert_eq!(
        probe.get_last_action(),
        99,
        "A standalone button must accept mouse input"
    );

    click(&probe, &adapter, 312.0, 40.0);
    key(&probe, &adapter, Key::End.into());
    key(&probe, &adapter, Key::Return.into());
    assert_eq!(
        probe.get_last_action(),
        4,
        "The popup's keyboard route must dispatch"
    );

    let mut failures = Vec::new();
    for action in 0..5 {
        probe.set_last_action(-1);
        probe.set_action_count(0);
        click(&probe, &adapter, 312.0, 40.0);
        click(&probe, &adapter, 240.0, 80.0 + action as f32 * 32.0);
        let actual = (probe.get_last_action(), probe.get_action_count());
        if actual != (action, 1) {
            failures.push((action, actual));
        }
    }
    assert!(
        failures.is_empty(),
        "Mouse popup commands were not dispatched correctly: {failures:?}"
    );
}
