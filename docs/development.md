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

`src/app.rs` 负责应用生命周期、托盘和共享资源。其私有模块放在 `src/app/`，按主窗口、知识包、题目编辑、图片、公式接入、练习窗口、设置和更新组织。功能通过 `App` 内部协作，题库、判分、学习调度、存储和更新安装等领域逻辑仍由独立库模块负责。

`src/structured_math.rs` 负责结构化编辑器的宿主接入；其剪贴板适配与整段输入共用表达式规范化，保留组件的结构、选区与撤销语义。`src/text_history.rs` 只维护普通文字的有界撤销记录。原生检查按功能放在 `src/native_checks/`，`mod.rs` 保留共享窗口与输入驱动；测试通过当前可见的输入框和 ƒ 入口验证练习，不保留已退役的公式面板入口。

知识包使用稳定的包与题目标识。导入、编辑和参与范围配置在校验、保存成功后生效，失败保留此前数据。公式判分保留绝对值、范数和有意义的分组。

动态选题使用固定默认参数的 FSRS 6 标量子集，时间和随机源由调用方提供；知识展示在读取学习状态前分流。个人状态独立保存，不随知识包导出。上游源码与许可见 [第三方声明](third_party_notices.md)。

关闭「弹出时自动聚焦」后，自动提醒显示完整题板，不请求系统焦点，出现或悬停不建立本次学习呈现记录；首次点击或从系统托盘进入时开始记录，点击选项的同一次操作即可作答。Windows 的输入保护在题板存续期间通过 [SetWinEventHook](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook) 订阅前台窗口、鼠标捕获及光标显隐事件，使用 out-of-context 回调，不注入其他进程；在 UI 循环合并查询 `GetCursorInfo` 和 `GetGUIThreadInfo`。其他前台窗口隐藏光标、捕获鼠标或查询失败时，使用 Winit 的 `set_cursor_hittest(false)` 穿透鼠标，释放后恢复点击；题板本身已激活时保留正常输入。切回其他进程的置顶窗口后，以 `SetWindowPos` 和 `SWP_NOACTIVATE` 恢复题板层级，不移动、缩放或激活题板，保留本程序图片预览等窗口的层级。窗口收起即释放订阅，不轮询，不监听按键或游戏暂停菜单。游戏自绘光标或其他输入方式仍需实际验证。

Windows 在提醒到期时调用 [SHQueryUserNotificationState](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shqueryusernotificationstate)；`QUNS_ACCEPTS_NOTIFICATIONS` 允许提醒，`QUNS_BUSY` 只允许不自动聚焦的题板，以兼容无边框全屏窗口。系统报告独占 Direct3D 全屏、演示、锁屏、未知状态或查询失败时继续等待下一个提醒间隔，不补弹积压。macOS/Linux 暂无对应的游戏鼠标保护与全屏查询。自动提醒失败仅记日志并重排提醒，避免用错误对话框打断其他应用。`native_practice_focus` 用独立窗口检查实际输入去向、题板可见性、隐藏／释放光标、鼠标捕获、悬停、直接点击作答、系统托盘激活、环境阻止与实际计时器错误路径。

静态题面由 `src/media.rs` 管理：源 PNG／JPEG 在后台规范化为有界 PNG，以内容哈希保存到数据目录的 `media/`。库和草稿只记录资源标识，显示时只解码当前题；收起窗口释放像素。有图时文字题面可为空，列表保留带序号的图片题条目，练习与知识展示不保留文字高度和间距；判断模式仍显示其独立陈述。无图时继续要求非空题面，移除唯一题面图片时必须补回文字。

`Store` 负责将含图包组装为单个 JSON 或从中安装资源，`media` 传输字段不进入 `state.json`。导入、编辑失败保留原题库；清理只处理已识别且未被库、当前草稿或待确认导入引用的自有文件。修改图片或文字仍按原规则改变学习指纹。若明确只删除同一图片的冗余提示并希望保留记忆，须基于修改前后的完整题目核对每种题型的旧指纹，再显式迁移匹配的进度、作答记录与呈现标记；不复用不匹配的旧指纹，不仅复制原记录文件，也不全局忽略题面。格式与配额见 [知识包格式](knowledge_pack_format.md#题面图片与可携带资源)。Windows 可用 `./scripts/build_windows.ps1 -Action test -NativeChecks -NativeTest native_picture_questions` 验证隔离题库中的编辑、取消、保存、纯图与含文字布局、放大与资源清理；生成的截图留在 `local_artifacts/`。

版本检查使用固定 GitHub Release 源，忽略草稿。预发布通道的 REST API 请求受限时，沿用系统代理检查同一仓库的公开 Atom 订阅源，并确认 `checksums.sha256` 已可匿名访问；只有标签、没有公开分发附件的候选不会提示更新。订阅源限制为 1 MiB／10000 个 XML 节点，最多探测三个版本的附件，不跟随下载重定向；正式版本通道保留 API 的预发布过滤规则。两条路径均失败时保留受限错误，关于页显示稍后重试。自动检查默认开启，启动后延迟至少 30 秒，每 24 小时最多一次；手动检查可覆盖预算，同时只运行一个检查任务。缓存受大小限制，结果安静显示在关于页。显式网络验证可分别运行 `update_check::tests::live_release_check` 与 `update_check::tests::live_rate_limited_api_fallback`，后者模拟 API 403 后通过实际传输检查公开源，不反复请求已耗尽的 API 额度。

Windows 窗口激活直接调用 [SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow)，避免 Winit 0.30 的 `focus_window()` 为取得前台权限而模拟 Alt 按下／释放，误触其他软件的全局快捷键。系统拒绝激活时仅请求窗口注意提示，保留系统焦点限制；自动提醒的聚焦开关与非聚焦输入保护继续生效。

## 应用内更新

`updater` 按固定仓库、版本标签及平台文件名下载校验文件和程序，不使用令牌。`update_transport` 共用系统 HTTPS／代理路径，流式下载上限 128 MiB，只跟随白名单 GitHub HTTPS 主机的重定向；元数据和 XML 继续保持独立上限。下载在后台执行，进度计时器仅在传输期间运行，取消、失败或正常退出会释放未交接的暂存目录。

`update_install` 用当前程序的临时副本执行一次性交接，Windows 等待原进程退出，Unix 在原数据锁与窗口释放后接收交接。Windows 替换 exe，macOS 校验并替换完整 app，Linux 使用原始 AppImage 路径并管理其子进程组。暂存目录位于安装位置旁边以支持同文件系统重命名。内部 job 限定目录、来源摘要、平台、随机凭据和配置备份名单；重启显式传入原数据目录，并保留原启动路径的形式。

新进程读入题库、建立窗口与托盘并进入事件循环后确认启动。确认前保留旧程序和三份 JSON 备份，推迟首次媒体清理；确认后回收暂存文件。安装位置旁的 `.try_update.lock` 使用文件锁串行化不同数据配置的更新，替换前再次核对原程序摘要。启动失败则停止本次新进程、在取得数据锁后恢复旧程序及 JSON，再启动旧版；恢复受阻时保留备份并显示位置。用户点击「下载并重启」开始更新，下载校验完成后自动交接退出，无需第二次确认。已有编辑阻止开始下载；下载期间新开的题目、公式、知识包元数据编辑或待确认导入会暂缓交接，保存或取消后由编辑状态事件继续，等待期间不轮询。取消与下载完成通过同一状态锁串行化，取消已校验的暂存也不会重启。自动检查仍只提示，不自行开始下载。

`native_update_check` 覆盖中英文进度、取消、窗口重建、失败、编辑保护、下载后自动退出交接和关于页固定几何；`native_update_edit_guard` 验证下载期间的多种编辑阻止重启、等待不轮询和编辑结束后自动继续。`python scripts/check_in_app_update.py --executable <程序> --output <记录.json>` 在临时目录中验证实际替换、重启、配置／题库／图片保留及失败回退；macOS 传入 app，Linux 另以 `--helper` 指定 AppImage 内原生程序。Windows 使用 `./scripts/build_windows.ps1 -Action build -Distribution -Example update_driver` 构建独立交接样例，再给检查脚本传入 `--driver local_artifacts/target/x86_64-pc-windows-msvc/release/examples/update_driver.exe`，可同时验证父进程的真实暂存、握手与退出。该检查已接入三平台打包流程。生成的 job、截图、备份与记录不进入源码仓库。

## 公式输入接入

从 `0.4.0` 起，公式输入通过 `src/structured_math.rs` 连接独立的 `slint-math-input`：编辑、导航、结构选区、排版与历史由该模块管理，Try 提供主题、内容校验与应用策略。题目和练习卡片的 ƒ 在当前字段下方展开公式键盘，直接编辑排版后的公式，不创建独立窗口，也不显示 LaTeX 查看按钮。卡片随内容增高，高内容使用内部滚动；题目编辑器应用后写回选中内容，取消保留原稿。练习作答在同一个输入控件中直接编辑公式，ƒ 仅开合键盘；Enter 核对答案，未完成的结构提示补齐。普通文字继续使用原生输入和输入法。按钮中的说明使用原生文字与字体回退，界面从同一固定依赖内嵌 STIX Two Math，补全混合标签里的数学符号；数学预览将 Unicode 上下标简写转换为对应结构，避免把缺字方框画进图标。可运行 `./scripts/build_windows.ps1 -Action test -NativeChecks -NativeTest native_structured_math` 检查接入；ƒ 可以展开或收起键盘并保留未完成草稿；收起不应用或判题。卡片尺寸变化时保留角落或手动位置，临时边界避让不写成记忆位置。

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

macOS/Linux 或 Git Bash 使用 `sh scripts/setup_git.sh`。提交、版本选择与变更日志统一按[提交与发布规范](release_policy.md)执行。新路径使用小写 ASCII snake_case。

提交前运行与改动有关的检查：文档修改检查链接与一致性，代码修改执行相关测试、Clippy 和格式检查。单次生成记录按[仓库内容](#仓库内容)保存。

版本标签与程序来源的核对方式见[构建来源](release_checklist.md#构建来源)。

## 诊断

默认记录 Try 的 Info、Warning 和 Error，依赖仅记录 Warning 与 Error；`--diagnostics` 可启用 Try 的 Debug 日志。日志位于当前数据目录的 `logs/`，可能包含文件路径，分享前检查内容。用户配置、题库、学习状态与版本缓存的含义见 [使用说明](user_guide.md)。
