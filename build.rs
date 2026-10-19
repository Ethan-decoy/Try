#[path = "src/math_palette.rs"]
mod math_palette;

fn main() {
    record_build_identity();
    let translations = compile_translations();
    compile_notices();
    println!("cargo:rerun-if-changed=ui/app.slint");
    println!("cargo:rerun-if-changed=ui/theme.slint");
    println!("cargo:rerun-if-changed=ui/content.slint");
    println!("cargo:rerun-if-changed=ui/formula_input.slint");
    println!("cargo:rerun-if-changed=ui/structured_math.slint");
    println!("cargo:rerun-if-changed=ui/licenses.slint");
    println!("cargo:rerun-if-changed=ui/math_palette.slint");
    println!("cargo:rerun-if-changed=ui/controls.slint");
    println!("cargo:rerun-if-changed=ui/settings.slint");
    println!("cargo:rerun-if-changed=ui/quick_answer.slint");
    println!("cargo:rerun-if-changed=assets/logo.svg");
    println!("cargo:rerun-if-changed=src/math_palette.rs");
    compile_logo();
    compile_math_palette();
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new()
            .with_style("fluent-light".into())
            .with_library_paths(std::collections::HashMap::from([(
                "math_caption_font".into(),
                math_caption_font_library(),
            )]))
            .with_default_translation_context(slint_build::DefaultTranslationContext::None)
            .with_bundled_translations(translations),
    )
    .expect("Slint UI compilation failed");
}

fn math_caption_font_library() -> std::path::PathBuf {
    // Resolve the font from the same pinned package as the editor. Slint embeds
    // it in the executable; no checkout or installed font is needed at runtime.
    let entry = slint_math_input::slint_library_path();
    let font = entry
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("assets/STIXTwoMath-Regular.ttf");
    println!("cargo:rerun-if-changed={}", font.display());
    let module = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("math_caption_font.slint");
    let source = format!(
        "import {:?};\nexport global MathCaptionFont {{ out property <string> family: \"STIX Two Math\"; }}\n",
        font.to_string_lossy().replace('\\', "/")
    );
    if std::fs::read_to_string(&module).ok().as_deref() != Some(source.as_str()) {
        std::fs::write(&module, source).expect("Math caption font module must be writable");
    }
    module
}

fn compile_translations() -> std::path::PathBuf {
    println!("cargo:rerun-if-changed=translations/en.json");
    let catalog: std::collections::BTreeMap<String, String> = serde_json::from_slice(
        &std::fs::read("translations/en.json").expect("Translation catalog must be readable"),
    )
    .expect("Translation catalog must be valid JSON");
    for (source, translation) in &catalog {
        assert_eq!(
            placeholders(source),
            placeholders(translation),
            "Translation placeholders must match: {source}"
        );
    }
    let mut sources: Vec<String> = [
        "ui/app.slint",
        "ui/image_preview.slint",
        "ui/settings.slint",
        "ui/content.slint",
        "ui/controls.slint",
        "ui/quick_answer.slint",
        "ui/formula_input.slint",
        "ui/structured_math.slint",
        "ui/licenses.slint",
        "ui/math_palette.slint",
        "src/app.rs",
        "src/main.rs",
        "src/single_instance.rs",
        "src/knowledge.rs",
        "src/storage.rs",
        "src/media.rs",
        "src/card_editor.rs",
        "src/formula.rs",
        "src/math_answer.rs",
        "src/math_editor.rs",
        "src/text_history.rs",
        "src/structured_math.rs",
        "src/autostart.rs",
        "src/reminder_schedule.rs",
        "src/practice.rs",
        "src/learning.rs",
        "src/learning_store.rs",
        "src/updater.rs",
        "src/update_install.rs",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for directory in ["src/app", "src/structured_math"] {
        println!("cargo:rerun-if-changed={directory}");
        sources.extend(
            std::fs::read_dir(directory)
                .expect("Source directory must be readable")
                .map(|entry| entry.expect("Source entry must be readable").path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
                .map(|path| path.to_string_lossy().replace('\\', "/")),
        );
    }
    sources.sort();
    for path in sources {
        println!("cargo:rerun-if-changed={path}");
        let source = std::fs::read_to_string(&path).expect("Translation source must be readable");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for marker in ["@tr(", "tr!("] {
            for rest in production.split(marker).skip(1) {
                let rest = rest.trim_start();
                if !rest.starts_with('"') {
                    continue;
                }
                let mut escaped = false;
                let end = rest
                    .char_indices()
                    .skip(1)
                    .find_map(|(index, character)| {
                        if escaped {
                            escaped = false;
                            None
                        } else if character == '\\' {
                            escaped = true;
                            None
                        } else if character == '"' {
                            Some(index)
                        } else {
                            None
                        }
                    })
                    .expect("Translation literal must end");
                let key: String =
                    serde_json::from_str(&rest[..=end]).expect("Translation literal must be valid");
                assert!(
                    catalog.contains_key(&key),
                    "Missing English translation in {path}: {key}"
                );
            }
        }
    }
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let bundled = output.join("translations");
    // Gettext's required layout is generated only inside Cargo's ignored output.
    // Repository sources keep one lowercase JSON catalog for both Rust and UI.
    let directory = bundled.join("en").join("LC_MESSAGES");
    std::fs::create_dir_all(&directory).expect("Translation output must be writable");
    let mut po = String::from(
        "msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Language: en\\n\"\n\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\n",
    );
    let mut rust =
        String::from("fn english(source: &'static str) -> &'static str { match source {\n");
    for (source, translation) in &catalog {
        assert!(
            !translation.trim().is_empty(),
            "Empty translation: {source}"
        );
        let source = serde_json::to_string(source).unwrap();
        let translation = serde_json::to_string(translation).unwrap();
        po.push_str(&format!("msgid {source}\nmsgstr {translation}\n\n"));
        rust.push_str(&format!("{source} => {translation},\n"));
    }
    rust.push_str("_ => source, } }\n");
    std::fs::write(directory.join("try.po"), po).expect("Bundled translations must be saved");
    std::fs::write(output.join("i18n_catalog.rs"), rust).expect("Rust translations must be saved");
    bundled
}

fn compile_notices() {
    use std::fmt::Write;
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let mut generated = String::new();
    if std::env::var_os("CARGO_FEATURE_DISTRIBUTION").is_none() {
        generated.push_str("pub const NOTICES_COMPLETE: bool = false;\npub const NOTICE_LOCK_SHA256: &str = \"\";\npub const COMPONENTS: &[Component] = &[];\n");
    } else {
        let path = "local_artifacts/open_source/embedded_notices.json";
        println!("cargo:rerun-if-changed={path}");
        println!("cargo:rerun-if-changed=Cargo.lock");
        println!("cargo:rerun-if-changed=license");
        println!("cargo:rerun-if-changed=third_party_licenses/slint_royalty_free_2_0.md");
        println!("cargo:rerun-if-changed=third_party_licenses/stix_two_math_ofl.txt");
        println!("cargo:rerun-if-changed=third_party_licenses/fsrs_bsd_3_clause.txt");
        let bundle: serde_json::Value = serde_json::from_slice(
            &std::fs::read(path).expect("Prepare embedded notices before a distribution build"),
        )
        .expect("Embedded notices must be valid JSON");
        assert_eq!(bundle["format"], "try_embedded_notices");
        assert_eq!(bundle["schema_version"], 1);
        assert_eq!(
            bundle["lock_content"]
                .as_str()
                .expect("Bundle needs its lock file"),
            std::fs::read_to_string("Cargo.lock").expect("Cargo.lock must be readable"),
            "Embedded notices must match Cargo.lock"
        );
        let components = bundle["components"]
            .as_array()
            .expect("Bundle needs components");
        assert!(
            !components.is_empty(),
            "Distribution notices must not be empty"
        );
        for (index, path) in [
            "license",
            "third_party_licenses/slint_royalty_free_2_0.md",
            "third_party_licenses/stix_two_math_ofl.txt",
            "third_party_licenses/fsrs_bsd_3_clause.txt",
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(
                components[index]["documents"][0]["text"].as_str().unwrap(),
                std::fs::read_to_string(path).expect("Tracked notice must be readable"),
                "Prepare embedded notices again after changing a tracked notice"
            );
        }
        writeln!(generated, "pub const NOTICES_COMPLETE: bool = true;").unwrap();
        writeln!(
            generated,
            "pub const NOTICE_LOCK_SHA256: &str = {:?};",
            bundle["lock_sha256"]
                .as_str()
                .expect("Bundle needs its lock hash")
        )
        .unwrap();
        let mut texts = std::collections::HashMap::<String, usize>::new();
        let mut entries = String::from("pub const COMPONENTS: &[Component] = &[\n");
        for component in components {
            writeln!(
                entries,
                "Component {{ name: {:?}, version: {:?}, license: {:?}, source: {:?}, documents: &[",
                component["name"].as_str().unwrap(),
                component["version"].as_str().unwrap(),
                component["license"].as_str().unwrap(),
                component["source"].as_str().unwrap(),
            )
            .unwrap();
            let documents = component["documents"].as_array().unwrap();
            assert!(!documents.is_empty(), "Each component needs its notices");
            for document in documents {
                let text = document["text"].as_str().unwrap();
                let next = texts.len();
                let index = *texts.entry(text.to_owned()).or_insert_with(|| {
                    writeln!(generated, "const TEXT_{next}: &str = {text:?};").unwrap();
                    next
                });
                writeln!(
                    entries,
                    "Document {{ name: {:?}, text: TEXT_{index} }},",
                    document["name"].as_str().unwrap()
                )
                .unwrap();
            }
            entries.push_str("] },\n");
        }
        entries.push_str("];\n");
        generated.push_str(&entries);
    }
    std::fs::write(output.join("embedded_notices.rs"), generated)
        .expect("Embedded notices source must be writable");
}

fn placeholders(text: &str) -> std::collections::BTreeSet<&str> {
    text.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
        .collect()
}

fn compile_math_palette() {
    use latex_rust::{Color, Dim, MathFont, SvgOptions};
    let font = MathFont::stix_two_math().expect("Math font must be available");
    let directory = std::path::Path::new("local_artifacts/math_palette_assets");
    std::fs::create_dir_all(directory).expect("Math palette directory must be writable");
    for (name, tools, columns, cell_width, font_size) in [
        ("functions", math_palette::FUNCTIONS, 4usize, 72usize, 12),
        ("symbols", math_palette::SYMBOLS, 6, 44, 16),
    ] {
        let width = columns * cell_width;
        let height = tools.len().div_ceil(columns) * 36;
        let mut sheet = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">"
        );
        for (index, tool) in tools.iter().enumerate() {
            let svg = latex_rust::latex_to_svg(
                tool.latex,
                &font,
                &SvgOptions {
                    font_size_pt: Dim::from_i64(font_size),
                    color: Color::rgb(238, 238, 238),
                    display: false,
                },
            )
            .unwrap_or_else(|error| panic!("{}: {error}", tool.name));
            let tree =
                resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
                    .expect("Math illustration SVG must be valid");
            let x = (index % columns * cell_width) as f32
                + (cell_width as f32 - tree.size().width()) / 2.0;
            let y = (index / columns * 36) as f32 + (36.0 - tree.size().height()) / 2.0;
            // Each nested SVG has its own definitions; prefix references so
            // glyph identifiers cannot collide across menu illustrations.
            let root = svg
                .find("<svg")
                .expect("Math illustration must have an SVG root");
            let svg = svg[root..]
                .replace("id=\"", &format!("id=\"p{index}_"))
                .replace("href=\"#", &format!("href=\"#p{index}_"))
                .replace("url(#", &format!("url(#p{index}_"));
            sheet.push_str(&svg.replacen(
                "<svg",
                &format!("<svg data-command=\"{}\" x=\"{x}\" y=\"{y}\"", tool.command),
                1,
            ));
        }
        sheet.push_str("</svg>");
        let tree = resvg::usvg::Tree::from_data(sheet.as_bytes(), &resvg::usvg::Options::default())
            .expect("Complete math palette SVG must be valid");
        let path = directory.join(format!("{name}.svg"));
        if std::fs::read_to_string(&path).ok().as_deref() != Some(&sheet) {
            std::fs::write(path, sheet).expect("Math palette must be saved");
        }
        // Raster sheets avoid the software backend downscaling an entire SVG
        // before cropping one cell. Three pixels per logical pixel also keep
        // the small glyphs crisp on high-DPI displays.
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width as u32 * 3, height as u32 * 3)
            .expect("Math palette size must be valid");
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(3.0, 3.0),
            &mut pixmap.as_mut(),
        );
        let png = pixmap.encode_png().expect("Math palette PNG must be valid");
        let path = directory.join(format!("{name}.png"));
        if std::fs::read(&path).ok().as_deref() != Some(png.as_slice()) {
            std::fs::write(path, png).expect("Math palette PNG must be saved");
        }
    }
}

fn record_build_identity() {
    fn git(arguments: &[&str]) -> Option<String> {
        let output = std::process::Command::new("git")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args(arguments)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
    // HEAD, its symbolic branch ref and the index also cover linked worktrees.
    for name in ["HEAD", "index"] {
        if let Some(path) = git(&["rev-parse", "--git-path", name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={path}");
    }
    let revision = git(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let commit = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let state = match git(&["status", "--porcelain"]) {
        Some(status) if status.is_empty() => "clean",
        Some(_) => "dirty",
        None => "unknown",
    };
    println!("cargo:rustc-env=TRY_GIT_REVISION={revision}");
    println!("cargo:rustc-env=TRY_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=TRY_GIT_STATE={state}");
}

fn compile_logo() {
    const SIZE: u32 = 32;
    let source = std::fs::read("assets/logo.svg").expect("Logo SVG must be available");
    let tree = resvg::usvg::Tree::from_data(&source, &resvg::usvg::Options::default())
        .expect("Logo SVG must be valid");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(SIZE, SIZE).expect("Logo size must be valid");
    let transform = resvg::tiny_skia::Transform::from_scale(
        SIZE as f32 / tree.size().width(),
        SIZE as f32 / tree.size().height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // Slint expects straight RGBA; resvg produces premultiplied pixels.
    let pixels: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    std::fs::write(output.join("logo_rgba.bin"), pixels).expect("Logo pixels must be writable");
    compile_desktop_icon(&tree, &output);
    compile_windows_resources(&output);
}

fn compile_windows_resources(output: &std::path::Path) {
    println!("cargo:rerun-if-env-changed=TRY_RESOURCE_COMPILER");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").expect("Cargo sets the package version");
    let major = std::env::var("CARGO_PKG_VERSION_MAJOR").unwrap();
    let minor = std::env::var("CARGO_PKG_VERSION_MINOR").unwrap();
    let patch = std::env::var("CARGO_PKG_VERSION_PATCH").unwrap();
    let script = format!(
        r#"1 ICON "try.ico"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004L
FILETYPE 0x1L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "FileDescription", "Try\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "InternalName", "try\0"
            VALUE "OriginalFilename", "try.exe\0"
            VALUE "ProductName", "Try\0"
            VALUE "ProductVersion", "{version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );
    std::fs::write(output.join("try.rc"), script).expect("Windows resources must be writable");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let compiler = std::env::var_os("TRY_RESOURCE_COMPILER")
        .unwrap_or_else(|| if msvc { "rc.exe" } else { "windres" }.into());
    let mut command = std::process::Command::new(compiler);
    command.current_dir(output);
    if msvc {
        command.args(["/nologo", "/fo", "try.res", "try.rc"]);
    } else {
        command.args([
            "--input",
            "try.rc",
            "--output",
            "try.res",
            "--output-format",
            "coff",
        ]);
    }
    let status = command
        .status()
        .expect("Windows resource compiler must be available");
    assert!(
        status.success(),
        "Windows icon and version resource compilation failed"
    );
    println!(
        "cargo:rustc-link-arg-bin=try={}",
        output.join("try.res").display()
    );
}

fn compile_desktop_icon(tree: &resvg::usvg::Tree, output: &std::path::Path) {
    // Packaging uses this PNG for macOS ICNS and Linux desktop integration.
    // It stays in OUT_DIR and does not enlarge the embedded tray icon.
    let mut desktop = resvg::tiny_skia::Pixmap::new(1024, 1024).expect("Icon size must be valid");
    resvg::render(
        tree,
        resvg::tiny_skia::Transform::from_scale(
            1024.0 / tree.size().width(),
            1024.0 / tree.size().height(),
        ),
        &mut desktop.as_mut(),
    );
    desktop
        .save_png(output.join("try_1024.png"))
        .expect("Desktop PNG must be writable");
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let images: Vec<_> = sizes
        .iter()
        .map(|size| {
            let mut pixmap =
                resvg::tiny_skia::Pixmap::new(*size, *size).expect("Icon size must be valid");
            resvg::render(
                tree,
                resvg::tiny_skia::Transform::from_scale(
                    *size as f32 / tree.size().width(),
                    *size as f32 / tree.size().height(),
                ),
                &mut pixmap.as_mut(),
            );
            pixmap.encode_png().expect("Icon PNG encoding must succeed")
        })
        .collect();
    let mut icon = vec![0, 0, 1, 0];
    icon.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (size, image) in sizes.iter().zip(&images) {
        icon.extend_from_slice(&[*size as u8, *size as u8, 0, 0]);
        icon.extend_from_slice(&1u16.to_le_bytes());
        icon.extend_from_slice(&32u16.to_le_bytes());
        icon.extend_from_slice(&(image.len() as u32).to_le_bytes());
        icon.extend_from_slice(&offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        icon.extend_from_slice(&image);
    }
    std::fs::write(output.join("try.ico"), icon).expect("Desktop icon must be writable");
}
