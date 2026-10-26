//! Software-only rendering and input shared by UI integration tests.
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{
    Key, Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
};
use slint::{ComponentHandle, LogicalPosition, Rgb8Pixel, SharedPixelBuffer};
use std::rc::Rc;

struct SoftwarePlatform(Rc<MinimalSoftwareWindow>);

impl Platform for SoftwarePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

pub fn software_window() -> Rc<MinimalSoftwareWindow> {
    let adapter = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(SoftwarePlatform(adapter.clone()))).unwrap();
    adapter
}

pub fn paint(adapter: &MinimalSoftwareWindow) -> SharedPixelBuffer<Rgb8Pixel> {
    slint::platform::update_timers_and_animations();
    adapter.request_redraw();
    let size = adapter.size();
    let mut pixels = SharedPixelBuffer::new(size.width, size.height);
    adapter.draw_if_needed(|renderer| {
        renderer.render(pixels.make_mut_slice(), size.width as usize);
    });
    pixels
}

pub fn click(component: &impl ComponentHandle, adapter: &MinimalSoftwareWindow, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    for event in [
        WindowEvent::PointerMoved { position },
        WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        },
    ] {
        component.window().dispatch_event(event);
    }
    paint(adapter);
}

pub fn key(component: &impl ComponentHandle, adapter: &MinimalSoftwareWindow, key: Key) {
    let text: slint::SharedString = key.into();
    component
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    component
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
    paint(adapter);
}
