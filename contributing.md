# Contributing to Try / 参与 Try

Try keeps practice brief and comfortable. Discuss substantial new features before implementing them. Read the [user guide](docs/user_guide_en.md) and [development conventions](docs/development.md). The [repository-content policy](docs/development.md#仓库内容) defines what belongs in Git, local storage and release assets.

Use Rust 1.92 or later. Run the relevant logic tests and Clippy, and check formatting:

```text
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Windows uses `scripts/build_windows.ps1` to configure MSVC/SDK. Native GUI checks are explicitly selected and use isolated profiles; ordinary tests do not launch desktop windows. macOS/Linux desktop behavior is still unverified, so report the desktop, architecture and window system with platform fixes.

Run `scripts/setup_git.ps1` on Windows or `sh scripts/setup_git.sh` elsewhere after cloning. Commit titles follow `type(scope): 中文描述`, for example `fix(ui): 修复英文菜单宽度`. Use Chinese descriptions as the project’s established policy. Do not commit generated artifacts, user data or logs. New project paths use lowercase ASCII snake_case.

Follow the [commit and release policy](docs/release_policy.md) for coherent batches, Conventional Commits, SemVer version selection and Keep a Changelog entries. Add notable changes under `Unreleased` in [changelog.md](changelog.md). Published releases use a fixed tag; later documentation changes can continue on the main branch without rebuilding existing release assets.

UI translations use one [English catalog](translations/en.json). Add matching entries with new `@tr` / `tr!` messages. Keep placeholders unchanged and preserve authored questions/answers. See [translation guidance](docs/development.md#翻译).

Bug reports should include the Try version, OS, reproduction steps and a screenshot made with sample data when helpful. Inspect logs before sharing: errors can include file paths. Keep credentials, personal packs and private screenshots out of issues and pull requests.

提交前阅读使用说明与开发约定；保持轻量、极简的既定方向。使用中文 Conventional Commits，不提交运行数据和生成物。报告问题时提供版本、系统和复现过程；截图使用示例数据，日志共享前检查路径和个人内容。已确定的源码与依赖许可见 [许可说明](docs/licensing.md)。
