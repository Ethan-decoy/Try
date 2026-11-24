# 分发与验证

下载文件及各平台的支持状态见 [README 的下载说明](../README.md#下载)。

## 构建来源

正式 Release 关联固定版本标签。标签指向已审阅的发布提交，各平台附件从该提交的干净检出构建。打包脚本核对当前检出的提交。

`--distribution-info` 输出内嵌版本、提交、源码状态、便携模式、运行库和声明摘要。打包脚本核对提交与 `clean` 状态。`checksums.sha256` 列出各平台程序文件及其摘要，可用于核对下载内容。

未影响程序或必要分发内容的文档、开发约定和测试数据修改，不要求已构建程序追随最新 `main`，也不要求重建已有发布包。程序行为或分发内容变化后，重新准备候选；已发布的行为变化通过下一版本交付。

Windows 的分发命令为：

```powershell
./scripts/prepare_third_party.ps1 -FetchUpstream
./scripts/build_windows.ps1 -Action build -Distribution
./scripts/prepare_windows_release.ps1 -OutputDirectory local_artifacts/releases/windows_x64_candidate
./scripts/check_windows_startup.ps1 -Portable -Executable ./local_artifacts/releases/windows_x64_candidate/try.exe
python scripts/check_in_app_update.py --executable ./local_artifacts/releases/windows_x64_candidate/try.exe --output ./local_artifacts/releases/windows_x64_candidate/update_install_check.json
```

已有输出目录拒绝覆盖；本地审阅未提交修改时可显式使用 `-ReviewCandidate`，该产物保留 `dirty` 标记。macOS/Linux 启用 `distribution` 构建，再运行 `scripts/package_macos.sh`／`scripts/package_linux.sh`；Unix 检查同样核对来源和声明。

完整步骤与相关测试见 [开发说明](development.md)。源码与测试的保留范围、生成物与私人数据的位置见 [仓库内容](development.md#仓库内容)。

## 分发验证

- 核对 `Cargo.toml`、锁文件中的本项目版本、标签和内嵌版本。
- 从干净源码构建，运行相关测试，核对软件包的架构、许可声明与校验和。
- 在隔离数据目录中验证启动、升级和失败回退，核对设置、题库、图片和学习记录。
- macOS 和 Linux 的托盘、焦点和文件对话框仍待实际桌面验收。Linux 还需核对 Wayland、FUSE 挂载和登录自启。
- 软件包包含程序和必要分发内容。第三方许可详见[许可说明](licensing.md)。

公开仓库的 GitHub Actions 检查 `main` 和 Pull Request，也支持从固定版本标签手动验证三平台构建。
