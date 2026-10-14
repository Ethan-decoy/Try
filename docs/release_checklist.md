# 发布与平台范围

当前发布为 [`0.1.0` 预发布版](https://github.com/Ethan-decoy/Try/releases/tag/v0.1.0)。实际下载以 [Releases](https://github.com/Ethan-decoy/Try/releases) 的附件和说明为准。

| 平台 | 分发文件 | 范围 |
| --- | --- | --- |
| Windows x64 | `try.exe` | 单文件便携版，运行后在旁边创建 `data/` |
| macOS arm64 | `try_macos_arm64_0.1.0.zip` | 实验性，已通过同源构建与隔离启动检查，待完整桌面验收 |
| Linux x64 | `try_linux_x64_0.1.0.AppImage` | 实验性，已通过同源构建与隔离启动检查，待完整桌面验收 |

macOS 仅支持 arm64，尚未完成 Developer ID 签名与公证。Linux 的托盘、Wayland、FUSE 挂载和登录自启需要在实际桌面环境验收。缺少当前源码的验证包时，不以历史附件代替。

## 构建来源

正式 Release 关联固定版本标签，例如 `v0.1.0`；标签指向已审阅的发布提交，各平台附件从该提交的干净检出构建。发布后保持标签与版本来源稳定，主分支可继续更新。打包脚本默认核对当前检出的提交，不能在源码已经改变的工作区给旧程序重新标记来源。

`--distribution-info` 输出内嵌版本、提交、源码状态、便携模式、运行库和声明摘要；打包脚本核对提交与 `clean` 状态。`checksums.sha256` 只列实际提供的程序文件，并与服务器摘要核对。草稿审阅期间记录候选提交，真正发布时确认标签、源码与各平台附件对应。

未影响程序或必要分发内容的文档、开发约定和测试数据修改，不要求已构建程序追随最新 `main`，也不要求重建已有发布包。程序行为或分发内容变化后，重新准备候选；已发布的行为变化通过下一版本交付。

Windows 的分发命令为：

```powershell
./scripts/prepare_third_party.ps1 -FetchUpstream
./scripts/build_windows.ps1 -Action build -Distribution
./scripts/prepare_windows_release.ps1 -OutputDirectory local_artifacts/releases/windows_x64_0_1_0_release
./scripts/check_windows_startup.ps1 -Portable -Executable ./local_artifacts/releases/windows_x64_0_1_0_release/try.exe
```

已有输出目录拒绝覆盖；本地审阅未提交修改时可显式使用 `-ReviewCandidate`，该产物保留 `dirty` 标记。macOS/Linux 启用 `distribution` 构建，再运行 `scripts/package_macos.sh`／`scripts/package_linux.sh`；Unix 检查同样核对来源和声明。

完整步骤与相关测试见 [开发说明](development.md)。源码与测试的保留范围、生成物与私人数据的位置见 [仓库内容](development.md#仓库内容)。

## 发布条件

- 先通过相关功能检查，再从干净源码构建，完成包、架构、声明与隔离启动校验。
- 软件包只包含程序与必要分发内容，个人题库、设置、学习记录和日志留在本地。
- 公开下载前确认 README／下载页的 Made with Slint badge 可见。[许可说明](licensing.md)
- 仓库公开与 Release 发布分别由维护者决定；提交组织与首次公开前的历史审阅按 [开发约定](development.md#提交规范) 执行。

公开仓库的 GitHub Actions 已启用，检查 `main` 和 Pull Request，也可从固定版本标签手动启动三平台构建。构建产物先校验并整理到 Release 草稿，正式发布另行确认。
