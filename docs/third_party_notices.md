# 第三方声明

Try 自有代码使用 MIT，Slint 1.18.1 使用 Royalty-free 2.0；其他组件保留各自许可。[分发许可说明](licensing.md)

公开构建通过 `scripts/prepare_third_party.ps1` 收集最终依赖图的许可、版权与 NOTICE 原文，以本地 `inventory.json` 记录版本、来源和 SHA-256。关于中的第三方声明仅列主要组件与许可；单文件程序内嵌经复核的完整原文，通过 `--third-party-notices` 输出，组件版本和源码入口随文本保留。Windows PowerShell 可用 `.\try.exe --third-party-notices | more` 查看。许可清单覆盖启用的普通与构建依赖，不等于实际链接库数量；原始 Cargo 元数据与本机缓存路径不会公开。

Slint 许可原文见 [Royalty-free 2.0](../third_party_licenses/slint_royalty_free_2_0.md)。MIT 授权 Try 自有代码不会改变 Slint 的署名与分发条件。当前精简关于采用公开网页署名路径，中英文 README 保留官方 badge；公开下载前核对页面已经公开且 badge 容易找到。

结构化公式输入使用 [slint-math-input 0.1.0-preview.2](https://github.com/Ethan-decoy/slint_math_input/tree/9d7290057cda3c0005450c35ac878d992ca761ba)。组件采用 MIT OR Apache-2.0，两个许可原文及其字体声明随程序内嵌；输入与排版在本地完成。

单题记忆使用固定默认参数的 FSRS 6 标量推断子集，改编自 [fsrs-rs 6.6.2 的固定提交](https://github.com/open-spaced-repetition/fsrs-rs/tree/6088949b99d44f46f5328fd7289d209b9a842b68)。保留原始版权与完整 [BSD-3-Clause](../third_party_licenses/fsrs_bsd_3_clause.txt)，声明随二进制内嵌。该子集不包含参数训练；Try 的提醒与加权选题是独立策略。内容指纹使用 `sha2 0.10.9`，其许可与传递依赖由最终清单收集。

`option-ext 0.2.0` 使用 [MPL-2.0](../third_party_licenses/option_ext_mpl_2_0.txt)，对应程序条目保留 [固定发布源码入口](https://crates.io/api/v1/crates/option-ext/0.2.0/download)。`dispatch 0.2.0` 的 MIT 声明特殊来源见 [参考说明](../third_party_licenses/dispatch_0_2_0_notice.txt)。

固定使用 `latex-rust 2.1.0`。其源码为 MIT OR Apache-2.0；本项目保留两个上游许可和 NOTICE：

- [MIT](../third_party_licenses/latex_rust_mit.txt)
- [Apache 2.0](../third_party_licenses/latex_rust_apache.txt)
- [上游 NOTICE](../third_party_licenses/latex_rust_notice.txt)

该版本内嵌 STIX Two Math 2.13，字体为 SIL Open Font License 1.1，原始字体许可及版权声明见 [STIX OFL](../third_party_licenses/stix_two_math_ofl.txt)。STIX Fonts 是 The Institute of Electrical and Electronics Engineers, Inc. 的商标。

以上文件复制自已下载的固定发布包；单文件分发将这些声明与字体许可证内嵌为可查看的文本。

日志使用 `log 0.4.34` 与 `flexi_logger 0.31.10`，均为 MIT OR Apache-2.0，保留从对应发布包原样复制并核对校验值的许可：

- [log MIT](../third_party_licenses/log_mit.txt)、[log Apache 2.0](../third_party_licenses/log_apache.txt)
- [flexi_logger MIT](../third_party_licenses/flexi_logger_mit.txt)、[flexi_logger Apache 2.0](../third_party_licenses/flexi_logger_apache.txt)

本地免打扰时间计算复用日志已经带入的 `chrono 0.4.45`，许可为 MIT OR Apache-2.0。两份许可与版权声明在上游同一文件中提供，原样保留为 [chrono 许可](../third_party_licenses/chrono_license.txt)。

版本检查使用 `semver 1.0.28`（MIT OR Apache-2.0）与 `webbrowser 1.2.4`（MIT OR Apache-2.0）。Windows 复用 `windows-sys` 调用系统 WinHTTP；macOS、Linux 使用 `ureq 3.4.2` 与 `native-tls 0.2.18`（均 MIT OR Apache-2.0），启用实际默认 Native TLS 连接器并使用系统信任证书。macOS 连接系统 Security 框架，Linux 使用 OpenSSL；AppImage 收入所需共享库，并在包内保存额外版权与来源清单。对应 Rust 依赖及传递依赖由内嵌声明收集许可原文。

AppImage 的非 Rust 共享库声明位于包内 `usr/share/doc/try/bundled_libraries/`，运行时及静态库声明位于 `usr/share/doc/try/appimage_runtime/`，补充程序的命令行声明。固定提交的运行时源码、上游构建脚本、libfuse 3.15.0 源码及补丁和 squashfuse 0.5.2 源码另以校验过的附件提供；应用仍以单个 AppImage 下载。分发产物和对应验证结果见 [发布清单](release_checklist.md)。
