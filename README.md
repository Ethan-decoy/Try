<h1><img src="assets/logo.svg" width="32" height="32" alt="" align="absmiddle" /> Try</h1>

[English](readme_en.md) · 简体中文

**A little practice, now and then.**

Try 是一个轻量桌面练习工具，让你随手反复熟悉感兴趣的知识，形成尽可能省去临时思考与推导的“肌肉记忆”。

按设定间隔出现一道小题，支持填空、选择、判断或直接看答案，答完继续手头的事情。

## 下载

从 [最新版本（v0.4.1 预发布版）](https://github.com/Ethan-decoy/Try/releases/tag/v0.4.1) 下载对应平台的文件。

| 平台 | 下载文件 | 状态 |
| --- | --- | --- |
| Windows x64 | [try.exe](https://github.com/Ethan-decoy/Try/releases/download/v0.4.1/try.exe) | 便携版 |
| macOS arm64 | [try_macos_arm64_0.4.1.zip](https://github.com/Ethan-decoy/Try/releases/download/v0.4.1/try_macos_arm64_0.4.1.zip) | 实验性 |
| Linux x64 | [try_linux_x64_0.4.1.AppImage](https://github.com/Ethan-decoy/Try/releases/download/v0.4.1/try_linux_x64_0.4.1.AppImage) | 实验性 |

Windows 将 `try.exe` 放在可写文件夹中双击运行，关闭主窗口后留在托盘。首次运行会在旁边创建 `data/`，请保留该文件夹。支持在关于页下载更新，校验完成后重启应用；`0.2.1` 及更早版本需先手动升级一次。

macOS 仅支持 arm64。macOS/Linux 为实验性版本，macOS 尚未完成公证。详细操作见 [使用说明](docs/user_guide.md)。

## 让 AI 制作知识包

1. 将 [AI 制作模板](docs/knowledge_pack_ai_template.md) 的全文交给 AI，并说明想练习的主题、知识包名称、标签和题目数量。
2. 让 AI 只输出合法 JSON，将结果保存为 UTF-8 的 `.json` 文件。
3. 在 Try 主窗口导入该文件，即可开始练习。

例如：

> 按模板生成 20 道微积分必要公式填空题，名称“微积分”，标签“公式”。保留必要条件，每题只保留短提示与明确答案，只输出 JSON。

也可以先导入 [微积分公式](knowledge_packs/authoring_template.json)、[高阶英语词汇](knowledge_packs/vocabulary_template.json) 或 [Rust 常用写法](knowledge_packs/rust_template.json)，每份 12 题。字段说明见 [知识包格式](docs/knowledge_pack_format.md)。

## 反馈

问题与建议请提交到 [Issues](https://github.com/Ethan-decoy/Try/issues)。

## 许可

Try 自有代码与 Logo 采用 [MIT](license)，第三方组件遵循各自许可，见 [第三方声明](docs/third_party_notices.md)。

<a href="https://slint.dev">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/slint-ui/slint/v1.18.1/logo/MadeWithSlint-logo-dark.svg" />
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/slint-ui/slint/v1.18.1/logo/MadeWithSlint-logo-light.svg" />
    <img src="https://raw.githubusercontent.com/slint-ui/slint/v1.18.1/logo/MadeWithSlint-logo-light.svg" width="144" alt="Made with Slint" />
  </picture>
</a>
