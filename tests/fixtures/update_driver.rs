//! Exercise the real parent-side handoff without an interactive test click.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
fn main() {
    use std::path::Path;
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if let Some(result) = r#try::update_install::helper_mode(&arguments) {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    let result = (|| -> Result<(), String> {
        if arguments.len() != 2 {
            return Err("expected payload and isolated data directory".into());
        }
        let data = Path::new(&arguments[1]);
        {
            let store = r#try::storage::Store::open(data.to_owned())?;
            let library = store.load()?;
            store.save(&library)?;
        }
        r#try::updater::prepare_fixture(Path::new(&arguments[0]))?.launch(data)
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!(
        "The parent-handoff fixture is Windows-only; use the packaged helper checks on Unix."
    );
    std::process::exit(2);
}
