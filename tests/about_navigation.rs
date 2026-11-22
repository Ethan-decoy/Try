//! About requests must survive the management window's initial layout and focus changes.
mod support;
use slint::platform::Key;
use slint::{ComponentHandle, ModelRc, PhysicalSize, Rgb8Pixel, SharedPixelBuffer, VecModel};
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

#[test]
fn pointer_about_does_not_show_a_pack_focus_outline() {
    let adapter = software_window();
    let window = MainWindow::new().unwrap();
    let theme = window.global::<Theme>();
    theme.set_motion(MotionPreference::Reduced);
    window.set_packs(ModelRc::new(VecModel::from(
        ["常用公式", "单词"]
            .into_iter()
            .map(|name| PackRow {
                id: name.into(),
                name: name.into(),
                detail: "4 道题".into(),
                participating: true,
                has_cards: true,
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    )));
    window.set_has_selection(true);
    window.set_selected_index(0);
    let weak = window.as_weak();
    window.on_select_pack(move |index| weak.unwrap().set_selected_index(index));
    window.show().unwrap();
    adapter.set_size(PhysicalSize::new(880, 640));
    paint(&adapter);

    let left = window.get_pack_list_left();
    let first_top = window.get_pack_list_top();
    let stride = window.get_pack_row_stride();
    let focus = theme.get_focus();
    let has_outline = |index: i32| {
        let pixels = paint(&adapter);
        let y = (first_top + index as f32 * stride).round() as usize;
        let x = left.round() as usize;
        pixels
            .as_slice()
            .chunks_exact(pixels.width() as usize)
            .skip(y)
            .take(7)
            .any(|row| {
                row[x + 12..x + 72]
                    .iter()
                    .filter(|pixel| {
                        (pixel.r, pixel.g, pixel.b) == (focus.red(), focus.green(), focus.blue())
                    })
                    .count()
                    >= 12
            })
    };
    let select_second = || {
        click(&window, &adapter, left + 32.0, first_top + stride + 20.0);
        assert_eq!(window.get_selected_index(), 1);
        assert!(
            !has_outline(1),
            "Pointer selection uses the selected background"
        );
    };
    let open_about = || {
        click(
            &window,
            &adapter,
            window.get_about_icon_center_x(),
            window.get_about_icon_center_y(),
        );
        assert!(window.get_about_open());
        has_outline(1)
    };

    select_second();
    let while_open = open_about();
    click(&window, &adapter, 850.0, 90.0);
    assert!(!window.get_about_open());
    let after_click_close = has_outline(1);

    select_second();
    let after_reopening = open_about();
    key(&window, &adapter, Key::Escape);
    assert!(!window.get_about_open());
    let after_escape = has_outline(1);

    assert_eq!(
        [while_open, after_click_close, after_reopening, after_escape],
        [false; 4],
        "The mouse-selected pack gains a keyboard outline on About open or dismissal"
    );
    select_second();
    key(&window, &adapter, Key::UpArrow);
    assert_eq!(window.get_selected_index(), 0);
    assert!(
        has_outline(0),
        "Keyboard navigation shows the pack focus outline"
    );
    window.invoke_show_about();
    assert!(!has_outline(0));
    key(&window, &adapter, Key::Escape);
    assert!(has_outline(0), "Dismissal restores genuine keyboard focus");
}

#[test]
fn pointer_open_fades_about_and_respects_motion_changes() {
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, PlatformError, WindowAdapter};
    use std::{cell::Cell, rc::Rc, time::Duration};

    struct Clock {
        adapter: Rc<MinimalSoftwareWindow>,
        elapsed: Rc<Cell<u64>>,
    }
    impl Platform for Clock {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(self.adapter.clone())
        }

        fn duration_since_start(&self) -> Duration {
            Duration::from_millis(self.elapsed.get())
        }
    }

    let adapter = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    let elapsed = Rc::new(Cell::new(1000));
    slint::platform::set_platform(Box::new(Clock {
        adapter: adapter.clone(),
        elapsed: elapsed.clone(),
    }))
    .unwrap();
    let window = MainWindow::new().unwrap();
    let theme = window.global::<Theme>();
    theme.set_motion(MotionPreference::Full);
    window.show().unwrap();
    adapter.set_size(PhysicalSize::new(880, 640));

    let sample = || {
        let pixels = paint(&adapter);
        let x = window.get_about_left().round() as usize + 64;
        let y = window.get_about_top().round() as usize + 20;
        // Read the title's contrast across several pixels so late frames
        // remain distinguishable from the settled panel after quantization.
        pixels
            .as_slice()
            .chunks_exact(pixels.width() as usize)
            .skip(y)
            .take(26)
            .flat_map(|row| &row[x..x + 52])
            .map(|pixel| u32::from(pixel.r) + u32::from(pixel.g) + u32::from(pixel.b))
            .sum::<u32>()
    };
    let open = || {
        click(
            &window,
            &adapter,
            window.get_about_icon_center_x(),
            window.get_about_icon_center_y(),
        );
        assert!(window.get_about_open());
        sample()
    };
    let closed = sample();
    let first = open();
    elapsed.set(1080);
    let during = sample();
    elapsed.set(1230);
    let settled = sample();
    assert_ne!(first, closed);
    assert_ne!(first, during);
    assert_ne!(during, settled);
    key(&window, &adapter, Key::Escape);
    assert!(!window.get_about_open());

    assert_eq!(open(), first);
    theme.set_motion(MotionPreference::Reduced);
    assert_eq!(sample(), settled);
    theme.set_motion(MotionPreference::Full);
    assert_eq!(sample(), settled);

    for motion in [MotionPreference::Reduced, MotionPreference::ColorOnly] {
        window.invoke_hide_about();
        theme.set_motion(motion);
        assert_eq!(open(), settled);
        key(&window, &adapter, Key::Escape);
        assert!(!window.get_about_open());
    }
    theme.set_motion(MotionPreference::Full);
    window.invoke_show_about();
    assert_eq!(sample(), settled);
}
