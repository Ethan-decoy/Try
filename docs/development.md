# 开发与构建

## 工具链与运行

需要 Rust 1.92 或更新版本。Windows 需要完整 MSVC 与 Windows SDK；macOS 需要 Xcode Command Line Tools；Linux 需要 OpenSSL 开发头文件及窗口、字体、XDG portal 等桌面依赖。

已配置工具链时运行：

```text
cargo run --locked --release
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

Windows 辅助脚本会定位工具链、MSVC 和 SDK，并将产物留在 `local_artifacts/`：

```powershell
./scripts/build_windows.ps1 -Action build
./scripts/run_windows.ps1 -LocalProfile
./scripts/build_windows.ps1 -Action test
./scripts/build_windows.ps1 -Action clippy
```

`-LocalProfile` 使用独立开发数据。`--background` 启动到托盘，`--practice` 打开卡片，`--data-dir <目录>` 指定隔离数据目录。Windows 发布版默认在 exe 旁保存数据，普通开发构建使用系统应用数据目录。详见 [使用说明](user_guide.md)。

## 分发与检查

正式分发启用 `distribution`，先收集与最终 `Cargo.lock` 一致的许可原文，再生成内嵌声明。Windows 使用静态 C 运行库，macOS 打包为 arm64 ZIP，Linux 打包为 AppImage 并附运行时源码。命令与来源规则见 [发布说明](release_checklist.md) 和 [许可说明](licensing.md)。

默认测试不主动打开桌面窗口。原生 GUI 检查需要显式选择，使用隔离配置；例如：

```powershell
./scripts/build_windows.ps1 -Action test -NativeChecks -NativeTest all
./scripts/build_windows.ps1 -Action test -NativeChecks -NativeTest native_desktop_theme
```

`all` 运行常规测试并编译原生检查；具体测试名见 `scripts/build_windows.ps1`。选择与改动有关的检查，Windows 的结果不替代 macOS/Linux 实机验收。

## 代码与数据

`src/` 包含题库、练习、学习状态、公式、窗口和持久化逻辑；`ui/` 定义 Slint 界面；`knowledge_packs/` 提供五个领域的入门示例与三份进阶模板；`scripts/` 负责构建、分发和检查。回归测试使用独立构造的数据，不读取个人题库。

知识包使用稳定的包与题目标识。导入、编辑和参与范围配置在校验、保存成功后生效，失败保留此前数据。公式判分保留绝对值、范数和有意义的分组。

动态选题使用固定默认参数的 FSRS 6 标量子集，时间和随机源由调用方提供；知识展示在读取学习状态前分流。个人状态独立保存，不随知识包导出。上游源码与许可见 [第三方声明](third_party_notices.md)。

版本检查使用固定 GitHub Release 源，忽略草稿。自动检查默认开启，启动后延迟至少 30 秒，每 24 小时最多一次；手动检查可覆盖预算，同时只运行一个请求。缓存受大小限制，结果安静显示在关于页。

## 翻译

界面使用 [English 目录](../translations/en.json)。新增 `@tr`／`tr!` 文案时同步目录，保持占位符不变。原始目录为 JSON，构建生成的 gettext 文件留在 Cargo 输出目录；知识包内容保持作者原语言。

## 仓库内容

按文件是否服务于当前使用、构建、验证、维护或许可决定是否跟踪。

| 内容 | 保留位置 |
| --- | --- |
| 产品介绍、下载入口、使用说明、AI 制作知识包说明、JSON 格式和公开模板 | Git 仓库；README 保持简洁，详细内容链接到对应文档 |
| 源码、界面、图标源文件、翻译、工具链配置和依赖锁文件 | Git 仓库 |
| 可重复的测试、独立测试数据、构建与打包脚本、CI 配置 | Git 仓库；生成的输出另行保存 |
| 开发与发布步骤、贡献说明、版本变化 | Git 仓库；各规则在对应文档中维护 |
| 项目许可、第三方许可原文和必要声明 | Git 仓库，并按许可要求随程序分发 |
| 本地 AI 配置、私下产品设计与讨论、原始研究、AI 对话、历史方案、临时脚本、截图、单次验证结果、日志、缓存和 Git 备份 | 本地 AI 配置保持在 Git 忽略范围，其他资料存入 `local_artifacts/`；CI 结果也可保存为工作流附件 |
| 私人知识包、用户设置、学习记录、更新缓存和凭据 | 本地数据目录或 `local_artifacts/`，保持在跟踪范围之外 |
| 可下载程序、校验和及必要分发文件 | Release 附件；本地候选保存在 `local_artifacts/` |

需要公开的维护规则、必要实现理由与来源写入开发说明或相关代码注释；私下产品设计、未定方案与调查过程保存在本地。个人题库独立保存；测试数据须为测试目的单独构造，不能用私人题库替代。

清理时先确认引用和用途，再移出过期内容。检查 `.gitignore` 与待提交文件清单；忽略规则不会自动移除已跟踪文件，文件删除也不会清除旧提交中的内容。首版公开前须同时审阅将公开的提交历史。

线上保留当前产品主分支与已确认的发布标签；本地原型、检查点和历史备份分支留在本地，推送时明确选择分支或标签。

## 提交规范

初次克隆后配置仓库钩子与提交模板：

```powershell
./scripts/setup_git.ps1
```

macOS/Linux 或 Git Bash 使用 `sh scripts/setup_git.sh`。提交标题使用中文 Conventional Commits，例如 `fix(ui): 修复菜单宽度`。新路径使用小写 ASCII snake_case。

反馈和调试先在工作区整理，需要检查点时保存在 `local_artifacts/`。用户要求保存里程碑或准备发布后，将一个完整功能、修复或文档批次的实现、必要说明和验证结论一起提交；单次生成记录按 [仓库内容](#仓库内容) 保存。提交前运行与改动有关的检查，文档修改检查链接与一致性，代码修改执行相关测试、Clippy 和格式检查。

首次公开前可将已验证首版整理为一个完整提交。公开或共享主分支后，以有意义的增量提交保存后续批次，保持已有历史稳定。使用短期分支和 PR 时，可通过 Squash and merge 将同一改动的调试提交合成一个主分支提交；一个版本可以包含多个完整批次。

版本标签固定发布来源，主分支可继续维护文档。未影响程序或必要分发内容的说明、开发约定或测试数据修改，无需重建已有发布包。具体来源与校验要求见 [发布说明](release_checklist.md#构建来源)。

## 诊断

默认记录 Try 的 Info、Warning 和 Error，依赖仅记录 Warning 与 Error；`--diagnostics` 可启用 Try 的 Debug 日志。日志位于当前数据目录的 `logs/`，可能包含文件路径，分享前检查内容。用户配置、题库、学习状态与版本缓存的含义见 [使用说明](user_guide.md)。
