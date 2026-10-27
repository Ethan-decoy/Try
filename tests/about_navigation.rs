//! About requests must survive the management window's initial layout.
mod support;
use slint::platform::Key;
use slint::{ComponentHandle, PhysicalSize, Rgb8Pixel, SharedPixelBuffer};
use support::{click, key, paint, software_window};

slint::include_modules!();

fn panel_pixel(window: &MainWindow, pixels: &SharedPixelBuffer<Rgb8Pixel>) -> (u8, u8, u8) {
    let x = (window.get_about_left() + 1.0).round() as usize;
    let y = (window.get_about_top() + 40.0).round() as usize;
    let pixel = pixels.as_slice()[y * pixels.width() as usize + x];
    (pixel.r, pixel.g, pixel.b)
}

#[test]
fn about_opens_with_the_first_window_layout() {
    let adapter = software_window();
    let window = MainWindow::new().unwrap();
    window.show().unwrap();
    for dark in [true, false] {
        window.set_dark_theme(dark);
        adapter.set_size(PhysicalSize::new(1000, 720));
        // The native host can deliver its initial final size after show_main()
        // returns, while the tray callback has already requested About.
        window.invoke_show_about();
        adapter.set_size(PhysicalSize::new(880, 640));
        let first = panel_pixel(&window, &paint(&adapter));
        assert!(window.get_about_open());

        // Manual opening in an already laid-out window is the control.
        window.invoke_show_about();
        let manual = panel_pixel(&window, &paint(&adapter));
        window.invoke_hide_about();
        let closed = panel_pixel(&window, &paint(&adapter));
        println!("dark={dark} first={first:?} manual={manual:?} closed={closed:?}");
        assert_ne!(
            manual, closed,
            "The manual About control must render its panel"
        );
        assert_eq!(
            first, manual,
            "Tray About must be visible after the first layout"
        );

        window.invoke_show_about();
        paint(&adapter);
        key(&window, &adapter, Key::Escape);
        assert!(!window.get_about_open());
        adapter.set_size(PhysicalSize::new(1000, 720));
        paint(&adapter);
        assert!(
            !window.get_about_open(),
            "Resize must not reopen a dismissed panel"
        );

        window.invoke_show_about();
        paint(&adapter);
        click(&window, &adapter, 970.0, 20.0);
        assert!(!window.get_about_open());
        adapter.set_size(PhysicalSize::new(880, 640));
        paint(&adapter);
        assert!(!window.get_about_open());

        window.invoke_show_notices();
        let notices = paint(&adapter);
        let sample = |pixels: &SharedPixelBuffer<Rgb8Pixel>| {
            let x = (window.get_about_left() + 1.0) as usize;
            let y = (window.get_notices_top() + 40.0) as usize;
            let pixel = pixels.as_slice()[y * pixels.width() as usize + x];
            (pixel.r, pixel.g, pixel.b)
        };
        let before = sample(&notices);
        adapter.set_size(PhysicalSize::new(760, 560));
        assert_eq!(
            sample(&paint(&adapter)),
            before,
            "Notices must follow the same anchor"
        );
        window.invoke_hide_notices();
    }
}
