//! Notices are generated at build time; reading them does not require sidecar files.

pub struct Document {
    pub name: &'static str,
    pub text: &'static str,
}

pub struct Component {
    pub name: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub source: &'static str,
    pub documents: &'static [Document],
}

include!(concat!(env!("OUT_DIR"), "/embedded_notices.rs"));

pub fn information() -> serde_json::Value {
    serde_json::json!({
        "format": "try_distribution_info",
        "version": env!("CARGO_PKG_VERSION"),
        "source_commit": env!("TRY_GIT_COMMIT"),
        "source_state": env!("TRY_GIT_STATE"),
        "portable": cfg!(all(target_os = "windows", feature = "portable")),
        "runtime_static": cfg!(target_feature = "crt-static"),
        "notices_complete": NOTICES_COMPLETE,
        "lock_sha256": NOTICE_LOCK_SHA256,
        "components": COMPONENTS.len(),
    })
}

pub fn write_notices(mut output: impl std::io::Write) -> std::io::Result<()> {
    for component in COMPONENTS {
        writeln!(
            output,
            "{} {}\nLicense: {}\nSource: {}",
            component.name, component.version, component.license, component.source,
        )?;
        for document in component.documents {
            writeln!(output, "\n---------- {} ----------\n", document.name)?;
            output.write_all(document.text.as_bytes())?;
            writeln!(output)?;
        }
        writeln!(output)?;
    }
    Ok(())
}
