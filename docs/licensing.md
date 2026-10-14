# Try 分发许可 / Licensing

Try 自有代码与自有 Logo 使用根目录 [MIT 许可证](../license)，版权标识为 Ethan-decoy。第三方源码、字体与用户导入的知识包保留各自许可，不能统一标成 MIT。

Slint 固定为 `1.18.1`，在 Try 桌面应用中选用 [Royalty-free 2.0](../third_party_licenses/slint_royalty_free_2_0.md)。精简关于后，署名选择该许可的公开网页路径：中英文 README 已放置官方 Made with Slint badge，首个下载公开前须确认 README／下载页面确实公开且 badge 容易找到。仅有私有仓库中的 badge 尚不满足这个公开条件。改变分发方式、做嵌入式产品或单独提供 Slint 时重新确认其许可范围。[Slint 固定版本许可与署名选项](https://github.com/slint-ui/slint/blob/v1.18.1/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md)

依赖清单及许可全文由 `scripts/prepare_third_party.ps1` 从最终 Cargo 图生成。原文件逐字复制并校验 SHA-256；发布 crate 没有携带许可文件的组件，从 `.cargo_vcs_info.json` 对应的官方上游固定提交收集。`prepare_embedded_notices.ps1` 复核清单与锁文件、原文校验值后，为单文件分发生成内嵌文本。关于底部只显示“MIT 许可”，第三方声明简短列出主要组件与许可；完整 MIT、字体版权、许可及 NOTICE 继续随 exe 内嵌。`--third-party-notices` 将组件版本、许可、源码入口和各份原文写入标准输出，不启动图形界面，也不创建数据或日志；Windows PowerShell 可用 `.\try.exe --third-party-notices | more` 查看。清单与生成物留在 `local_artifacts/`，本机缓存路径、Cargo 元数据和 API 目录缓存不嵌入程序。

`dispatch 0.2.0` 的原始 manifest 明确声明 MIT、作者为 Steven Sheldon，但固定源码没有单独许可文件。收集器同时保留 manifest 与 [明确标注来源的 MIT 参考声明](../third_party_licenses/dispatch_0_2_0_notice.txt)，不将本项目整理的说明冒充上游原文。其来源与校验值记录在清单中。[固定 manifest](https://github.com/SSheldon/rust-dispatch/blob/82d6c7a5b75dc0c71c3f46f87bb6c16a476f7748/Cargo.toml)、[MIT 标准文本](https://spdx.org/licenses/MIT.html)

`option-ext 0.2.0` 使用 [MPL-2.0](../third_party_licenses/option_ext_mpl_2_0.txt)。本项目不修改该组件；单文件程序在对应条目和分发说明中保留它的许可及 [固定版本源码下载入口](https://crates.io/api/v1/crates/option-ext/0.2.0/download)，接收者可以免费获取原始发布源码。早期目录包曾直接附带源码，当前单文件分发使用明确的源码获取说明。[MPL 2.0 第 3.2 节](https://www.mozilla.org/en-US/MPL/2.0/#distribution-of-executable-form)

数学显示保留 latex-rust 的许可与 NOTICE，以及 STIX Two Math 的 SIL Open Font License 和原版权声明。其他依赖按清单附带声明；带 `AND` 的 SPDX 条件不能按 `OR` 任意省略。[第三方声明](third_party_notices.md)

Try’s own source and logo are MIT licensed. Slint 1.18.1 is used under its Royalty-free 2.0 license, with attribution through the official Made with Slint badge on a public webpage. Before making downloads public, verify that the README/download page is public and the badge is easy to find. Other components keep their own licenses, including STIX Two Math under SIL OFL and option-ext under MPL-2.0. About shows a short MIT footer and third-party notice list. The `--third-party-notices` command prints the complete embedded copyrights, licenses, NOTICE texts and versioned source locations without initializing the app or its data. Authored/imported packs retain their authors’ rights.

许可证来源整理与构建检查不等于逐文件法律审计。重新发布修改版本时，应同步实际依赖、原许可文本和署名，尤其是独立知识包的授权。
