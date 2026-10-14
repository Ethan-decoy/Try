#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#[cfg(all(
    target_os = "windows",
    feature = "distribution",
    not(target_feature = "crt-static")
))]
compile_error!("Windows distribution builds must link the C runtime statically");
use r#try::tr;

mod app;
mod floating_window;
mod formula_presentation;
#[cfg(target_os = "windows")]
mod single_instance;
mod window_repaint;

use r#try::knowledge::AppResult;
use r#try::storage::Store;

slint::include_modules!();

fn run(logger: &mut Option<flexi_logger::LoggerHandle>) -> AppResult<()> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.iter().any(|arg| arg == "--distribution-info") {
        println!("{}", r#try::distribution::information());
        return Ok(());
    }
    if arguments.iter().any(|arg| arg == "--third-party-notices") {
        if let Err(error) = r#try::distribution::write_notices(std::io::stdout().lock())
            && error.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(error.to_string());
        }
        return Ok(());
    }
    let practice = arguments.iter().any(|arg| arg == "--practice");
    let background = arguments.iter().any(|arg| arg == "--background");
    let diagnostics = arguments.iter().any(|arg| arg == "--diagnostics");
    let data_argument = arguments
        .iter()
        .position(|arg| arg == "--data-dir")
        .map(|index| {
            arguments
                .get(index + 1)
                .filter(|path| !path.is_empty())
                .cloned()
                .ok_or(tr!("--data-dir 需要指定数据目录。"))
        })
        .transpose()?;
    let directory = if let Some(path) = data_argument.or_else(|| std::env::var_os("TRY_DATA_DIR")) {
        std::path::PathBuf::from(path)
    } else if let Some(directory) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_owned))
        .filter(|path| {
            cfg!(all(target_os = "windows", feature = "portable"))
                || path.join("portable.flag").is_file()
        })
    {
        directory.join("data")
    } else {
        directories::ProjectDirs::from("", "", "try")
            .ok_or(tr!("无法找到本地数据目录。"))?
            .data_local_dir()
            .to_owned()
    };
    #[cfg(target_os = "windows")]
    if single_instance::Instance::forward(&directory, practice)? {
        return Ok(());
    }
    let store = match Store::open(directory.clone()) {
        Ok(store) => store,
        Err(error) => {
            #[cfg(target_os = "windows")]
            for _ in 0..10 {
                // A second double-click can arrive while the first instance is
                // loading its library, before its tray advertises readiness.
                std::thread::sleep(std::time::Duration::from_millis(50));
                if single_instance::Instance::forward(&directory, practice)? {
                    return Ok(());
                }
            }
            return Err(error);
        }
    };
    *logger = r#try::diagnostics::initialize(&directory, diagnostics);
    log::info!(target: "try::main", "event=launch_requested background={background} practice={practice}");
    let library = store.load()?;
    let floating = std::rc::Rc::new(floating_window::FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(move |attributes| window_hook.attributes(attributes))
        .select()
        .map_err(|error| error.to_string())?;
    let app = app::App::new(store, library, floating)?;
    app.start(background || practice)?;
    #[cfg(target_os = "windows")]
    let _instance = {
        // Slint creates its tray through delayed property change handlers. A
        // background start has no window show to flush them before we listen.
        slint::platform::update_timers_and_animations();
        let weak = std::rc::Rc::downgrade(&app);
        single_instance::Instance::listen(
            &directory,
            std::rc::Rc::new(move |practice| {
                if let Some(app) = weak.upgrade() {
                    app.open_from_launcher(practice);
                }
            }),
        )?
    };
    if practice {
        app.show_practice()?;
    }
    slint::run_event_loop_until_quit().map_err(|error| error.to_string())
}

fn main() -> std::process::ExitCode {
    r#try::i18n::activate(r#try::i18n::InterfaceLanguage::System);
    let mut logger = None;
    let code = if let Err(error) = run(&mut logger) {
        log::error!(target: "try::main", "event=run_failed reason={error}");
        eprintln!("Try: {error}");
        rfd::MessageDialog::new()
            .set_title(tr!("Try · 无法启动"))
            .set_description(error)
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::ExitCode::FAILURE
    } else {
        log::info!(target: "try::main", "event=run_finished");
        std::process::ExitCode::SUCCESS
    };
    if let Some(logger) = logger {
        logger.shutdown();
    }
    code
}
