# 给其他 AI 的知识包制作模板

把本文件完整交给负责整理内容的 AI，并填写下面的制作要求。AI 输出的 JSON 保存为 UTF-8 `.json` 文件，再从 Try 主窗口知识包列表上方的导入入口选择该文件。无需修改 Try 的本地 `state.json`。

本模板对应当前 Try 的 `schema_version: 1`，包含知识包标签。下方公式示例与 [微积分模板](../knowledge_packs/authoring_template.json) 内容相同；另有 [高阶英语词汇模板](../knowledge_packs/vocabulary_template.json) 和 [Rust 模板](../knowledge_packs/rust_template.json)，每份 12 题。三份模板分别展示一个主题，制作新包时替换示例内容，使用新的知识包 id。更新已有包时沿用原知识包和题目 id，未要求修改的标签也保持原值。

## 本次制作要求

- 主题／参考材料：填写要练习的内容，或附上原始资料。
- 目标水平与范围：例如大学微积分的必要公式、进阶英语词汇、Rust 稳定标准库的常用写法。主题可以深入，题目仍保持短提示与短答案。
- 知识包名称：填写适合这份内容的简短名称，例如「微积分」「单词」「常用写法」。
- 知识包标签（可选）：填写与名称搭配合适的短标签，例如名称「单词」配标签「英语」，名称「微积分」配标签「公式」。按内容选择，让两者的信息互补；全包题目共用。未指定时省略，更新已有包时保留原标签。
- 题目数量：填写目标数量。
- 作答内容：填空／选择／判断／同一知识点支持多种方式。未指定时只生成填空，不额外生成其他方式。
- 答案接受范围：填写同义词、大小写变体、等价表达式等需要接受的写法。未指定时使用明确、简短的标准答案。

## AI 执行说明

你的任务是把用户指定的知识整理为 Try 可以直接导入的知识包。遵守下述固定结构，不创建另一种格式，不根据其他题库软件的格式猜测字段。

1. 正常完成时只输出一个合法 JSON 对象，不带 Markdown 代码围栏、前言、注释、尾随逗号或解释。若原始资料含有会影响正确答案的歧义，先澄清该处，不能把猜测写成标准答案。
2. 顶层必填字段只有 `format`、`schema_version`、`id`、`name`、`cards`；可以添加 `description` 和单个字符串 `tag`。`tag` 属于知识包顶层，不放入每道题，不使用 `tags` 数组；具体规则见下方「知识包标签」。`format` 固定为字符串 `"try"`，`schema_version` 固定为数字 `1`。图片资源扩展仅按下方图片题章节添加。
3. 每道题必填 `prompt` 和 `answer`。有有效题面图片时，Try 0.5.0 及更新版本允许 `prompt` 为 `""` 且省略 `prompt_latex`，用于纯图辨认；不要添加无意义的提示句或不可见字符占位。无图题面与答案必须非空。推荐同时生成稳定的 `id`：以小写英文字母开头，只含小写字母、数字和下划线，最多 80 个字符；同一包内题目 id 不能重复。知识包 id 使用同样规则。更新内容或调整顺序时保留原 id，新增知识点使用新 id。
4. 围绕用户兴趣，将内容拆成看到提示就能直接联想到答案的知识点，例如词义、符号名称、常见公式结果、语法写法或固定对应关系。每题只练一个关系，提供足够语境以使答案明确，标准答案简短；尽可能省去临时计算和推导。数学直接练习公式本身与必要条件，不生成临时求值题或证明题。英语词义题直接使用单词作为提示，例如 `exacerbate` → `加剧`，有歧义时才补充必要语境，并用别名接受准确的常见同义表达。Rust 练习具体场景对应的稳定 API、类型或运算符，写清借用／移动、惰性／立即计算等影响答案的条件；方法名直接写普通文字。不添加解释、推导、评分、学习计划或与题目无关的信息。
5. 普通文字直接写入 `prompt`、`answer`，不强行转换成数学。数学题同时提供一致的普通文字和 LaTeX：`prompt_latex` 对应题面，`answer_latex` 对应标准答案；没有数学的内容省略这些字段。
6. 可选字段只在有用时添加。省略无用的空字符串、空数组和 `null`。不要添加 `type`、`mode`、`enabled`、`options`、`correct_index`、`explanation`、`tags`、`show_pack_tags`、`settings`、图片地址或自定义字段；当前导入器拒绝未知字段。图片题只在用户使用 Try 0.2.0 或更新版本并提供图片时，按下方「图片题制作」处理。评分、熟练度、选题权重和 FSRS 状态由 Try 根据作答独立维护，也不写入知识包。
7. 生成完逐项自查字段、标签长度与字符、JSON 转义、标识唯一性、答案与选项的语义正确性。下面的示例用于解释格式，不是用户要求生成的题目，不保留无关示例题。标题与标签应匹配本次主题；同一包内的题目围绕该主题组织。

## 可选内容的生成规则

### 知识包标签

`tag` 是整个知识包共用的一个可选字符串，`name` 是知识包名称；侧栏显示为「标签　名称」。两者可按内容自由搭配，以简短、清楚、信息互补为准。例如标签为「英语」时，名称可以是「单词」，不必再写「英语 · 单词」。名称本身已足够清楚时，也可以省略标签。

| 标签 `tag` | 标题 `name` | 对应模板 |
| --- | --- | --- |
| 公式 | 微积分 | [可导入 JSON](../knowledge_packs/authoring_template.json) |
| 英语 | 进阶单词 | [可导入 JSON](../knowledge_packs/vocabulary_template.json) |
| Rust | 常用写法 | [可导入 JSON](../knowledge_packs/rust_template.json) |

按本次制作要求填写，更新已有包时保留原值；只有用户要求修改或移除标签时才修改。

标签最多 24 个 Unicode 字符，生成时去除首尾空白。不使用换行、制表符、其他控制字符或 Unicode 行／段分隔符（U+2028、U+2029）。不需要标签时省略 `tag`；已有包要移除标签时省略该字段或写 `"tag": ""`，不要写 `null`、数组或对象。

Try 会在侧栏包名前显示标签，练习卡片也显示同一个标签；卡片靠屏幕左侧时放在右上角，靠右侧时放在左上角。显示开关与位置由软件处理，不在 JSON 中指定，也不把标签拼进每道题的题面或答案。修改标签不改变题目 id，不重置个人学习记录。

### 填空与答案别名

普通填空只有 `id`、`prompt`、`answer` 即可。`aliases` 是字符串数组，列出额外可以判为正确的答案写法，不是提示、解释或错误选项。

普通文字只去掉答案首尾空白，大小写和内部空白仍参与匹配。例如希望同时接受 `CPU`、`cpu` 时，需要明确提供后一种别名。

英语别名只列本题词义的准确同义表达，不混入其他词义。Rust 方法题可接受方法名及有意义的调用写法；需要闭包或参数的方法不把无参调用伪装成可运行代码。

数学输入会归一化常见书写形式，`√3/2`、`sqrt(3)/2` 与 `\frac{\sqrt{3}}{2}` 不需要全部重复列为别名。Try 不做通用代数化简或近似计算：希望 `1/2` 同时接受 `0.5`，或希望其他等价表达式被接受时，需要明确提供别名；近似值只在用户允许时添加。

### 选择内容

标准答案仍放在 `answer`，数学标准答案的排版放在 `answer_latex`。`distractors` 必须恰好包含两个错误选项，每项必填 `text`，数学选项可以加 `latex`。程序将标准答案和两项干扰项组成三个选项，并在出题时打乱顺序。

不要另外添加完整 `options`、字母标签或正确选项序号。三个选项在内容上必须只有一个正确；干扰项不能与标准答案或别名相同，不能用另一种等价数学写法充当错误选项。

### 判断内容

`true_false` 必填 `statement`、`is_true`、`correction`。陈述必须是可以判断真假的完整句子或等式，不带待填问号；`is_true` 是布尔值 `true` 或 `false`，不是字符串。数学陈述可以加 `statement_latex`。

当前格式仍要求兼容字段 `correction`。新生成的文件统一写正确符号：`is_true: true` 时写 `"✓"`，`is_true: false` 时写 `"✕"`。省略 `correction_latex`，不额外编写修正陈述、解释或推导。Try 根据 `is_true` 判断并显示反馈符号，不用 `correction` 决定对错。

如果同一知识点还提供填空或选择内容，保留原来的 `prompt`、`answer`；判断陈述只放在 `true_false`。例如标准知识是 `|x| < 1 时，(arcsin x)′ = 1/√(1-x²)`，可以额外提供为假的判断陈述 `|x| < 1 时，(arcsin x)′ = 1/(1+x²)`，但不能把错误陈述替换成主知识点。

如果这道题只制作判断内容，当前统一卡片结构仍要求 `prompt`、`answer`：将 `prompt` 设为与 `statement` 相同的陈述，将 `answer` 设为与 `is_true` 对应的 `"✓"` 或 `"✕"`；数学题面同时复制到 `prompt_latex`。这部分重复是当前格式的兼容要求。

### 数学排版

LaTeX 字段只放表达式正文，不套 `$...$`、`$$...$$`、`\(...\)` 或整个 TeX 文档，不使用自定义宏、宏包和外部文件。优先使用常见命令，例如 `\frac{a}{b}`、`\sqrt{x}`、`x^{2}`、`a_{n}`、`\sin`、`\cos`、`\tan`、`\log_{2}(x)`、`\ln(x)`、`\pi`、`\sum`、`\int` 和 `\lim`。

混排中文与公式时，可在表达式中用 `\text{中文}` 表示普通文字。普通文字字段与对应 LaTeX 必须表达相同内容；文本回退中不要直接保留未经处理的 LaTeX 命令。

JSON 中每个反斜线必须转义：实际 LaTeX `\frac{1}{2}` 在输出 JSON 中写为 `"\\frac{1}{2}"`；实际 `\log_{2}(x)` 写为 `"\\log_{2}(x)"`。当前排版器支持有限的数学语法，不能假定完整 LaTeX 文档的任何命令都可导入；复杂语法应先用小包确认。

### 图片题制作

v0.1.0 不支持图片题。默认仍生成能直接导入的文字／公式 JSON；需要图片时，先制作短提示与明确答案，例如「图中是哪个函数？」→ `sin(x)`，再由用户在 Try 0.2.0 或更新版本的编辑页添加 PNG／JPEG，保存后导出完整知识包。图片中不要直接标出待回答的名称；坐标、尺度、必要条件须足以区分正确答案。

自动制作含图文件需要实际文件处理能力：题目的可选字段 `prompt_image` 是 Try 规范化后 PNG 的 64 位小写 SHA-256；顶层 `media` 以相同标识为键，保存完整 PNG 的标准 Base64。不能用远程网址、本机文件路径、猜测的哈希或省略号代替资源。无法实际规范化、计算和校验时，省略图片字段，交给 Try 编辑页附图。不要输出看似完整却无法导入的资源数据。

同图题目可复用同一资源；导出文件必须完整携带所有引用，不能附带无用资源。限制及结构见 [知识包格式](knowledge_pack_format.md#题面图片与可携带资源)。图片只扩展题面，答案、别名、选择内容与判断内容仍遵守本模板的原有规则。只有用户明确要求制作含图包时才添加这些字段。

## 完整示例：公式 · 微积分

12 道题覆盖求导规则、常用原函数、分部积分、微积分基本定理和基本极限，其中三题还提供选择与判断内容。普通文字写法见 [英语 · 进阶单词模板](../knowledge_packs/vocabulary_template.json) 与 [Rust · 常用写法模板](../knowledge_packs/rust_template.json)。

按题保留分母非零、定义域等必要条件；三角函数使用弧度，求原函数的答案保留常数 C。分部积分式两侧已使用不定积分，模板沿用通常省略显式 C 的写法，并允许补写 C。

```json
{
  "format": "try",
  "schema_version": 1,
  "id": "calculus_template",
  "name": "微积分",
  "tag": "公式",
  "description": "常用求导、积分与极限公式。变量取实数，n 为实常数，三角函数使用弧度；u、v、f、g 在所需区间可微。求原函数时保留任意常数 C；分部积分按两侧不定积分的恒等关系书写。",
  "cards": [
    {
      "id": "power_derivative",
      "prompt": "(xⁿ)′ = ?（x > 0）",
      "answer": "nx^(n-1)",
      "prompt_latex": "\\frac{d}{dx}x^{n}=?\\quad(x>0)",
      "answer_latex": "nx^{n-1}",
      "aliases": [
        "n*x^(n-1)"
      ]
    },
    {
      "id": "product_rule",
      "prompt": "(uv)′ = ?",
      "answer": "u'v+uv'",
      "prompt_latex": "(uv)'=?",
      "answer_latex": "u'v+uv'",
      "aliases": [
        "u'*v+u*v'",
        "uv'+u'v"
      ]
    },
    {
      "id": "quotient_rule",
      "prompt": "(u/v)′ = ?（v ≠ 0）",
      "answer": "(u'v-uv')/v^2",
      "prompt_latex": "\\left(\\frac{u}{v}\\right)'=?\\quad(v\\ne0)",
      "answer_latex": "\\frac{u'v-uv'}{v^{2}}",
      "aliases": [
        "(u'*v-u*v')/v^2"
      ]
    },
    {
      "id": "chain_rule",
      "prompt": "(f(g(x)))′ = ?",
      "answer": "f'(g(x))g'(x)",
      "prompt_latex": "\\frac{d}{dx}f(g(x))=?",
      "answer_latex": "f'(g(x))g'(x)",
      "aliases": [
        "f'(g(x))*g'(x)"
      ]
    },
    {
      "id": "log_absolute_derivative",
      "prompt": "(ln|x|)′ = ?（x ≠ 0）",
      "answer": "1/x",
      "prompt_latex": "\\frac{d}{dx}\\ln\\left|x\\right|=?\\quad(x\\ne0)",
      "answer_latex": "\\frac{1}{x}",
      "distractors": [
        {
          "text": "1/|x|",
          "latex": "\\frac{1}{\\left|x\\right|}"
        },
        {
          "text": "x",
          "latex": "x"
        }
      ],
      "true_false": {
        "statement": "x ≠ 0 时，(ln|x|)′ = 1/|x|",
        "statement_latex": "\\frac{d}{dx}\\ln\\left|x\\right|=\\frac{1}{\\left|x\\right|}\\quad(x\\ne0)",
        "is_true": false,
        "correction": "✕"
      }
    },
    {
      "id": "arcsine_derivative",
      "prompt": "(arcsin x)′ = ?（|x| < 1）",
      "answer": "1/sqrt(1-x^2)",
      "prompt_latex": "\\frac{d}{dx}\\arcsin(x)=?\\quad(\\left|x\\right|<1)",
      "answer_latex": "\\frac{1}{\\sqrt{1-x^{2}}}",
      "distractors": [
        {
          "text": "−1/sqrt(1-x^2)",
          "latex": "-\\frac{1}{\\sqrt{1-x^{2}}}"
        },
        {
          "text": "1/(1+x^2)",
          "latex": "\\frac{1}{1+x^{2}}"
        }
      ],
      "true_false": {
        "statement": "|x| < 1 时，(arcsin x)′ = 1/√(1-x²)",
        "statement_latex": "\\frac{d}{dx}\\arcsin(x)=\\frac{1}{\\sqrt{1-x^{2}}}\\quad(\\left|x\\right|<1)",
        "is_true": true,
        "correction": "✓"
      }
    },
    {
      "id": "arctangent_derivative",
      "prompt": "(arctan x)′ = ?",
      "answer": "1/(1+x^2)",
      "prompt_latex": "\\frac{d}{dx}\\arctan(x)=?",
      "answer_latex": "\\frac{1}{1+x^{2}}"
    },
    {
      "id": "power_antiderivative",
      "prompt": "∫ xⁿ dx = ?（n ≠ −1，x > 0）",
      "answer": "x^(n+1)/(n+1)+C",
      "prompt_latex": "\\int x^{n}\\,dx=?\\quad(n\\ne-1,\\ x>0)",
      "answer_latex": "\\frac{x^{n+1}}{n+1}+C",
      "aliases": [
        "x^(n+1)/(n+1)+c"
      ]
    },
    {
      "id": "reciprocal_antiderivative",
      "prompt": "∫ 1/x dx = ?（x ≠ 0）",
      "answer": "ln|x|+C",
      "prompt_latex": "\\int\\frac{1}{x}\\,dx=?\\quad(x\\ne0)",
      "answer_latex": "\\ln\\left|x\\right|+C",
      "aliases": [
        "ln(abs(x))+C",
        "ln|x|+c",
        "ln(abs(x))+c"
      ]
    },
    {
      "id": "integration_by_parts",
      "prompt": "∫ u dv = ?",
      "answer": "uv-∫ v du",
      "prompt_latex": "\\int u\\,dv=?",
      "answer_latex": "uv-\\int v\\,du",
      "aliases": [
        "uv-∫vdu",
        "uv-∫ v du+C",
        "uv-∫vdu+C"
      ]
    },
    {
      "id": "fundamental_theorem_derivative",
      "prompt": "f 连续，(∫ₐˣ f(t) dt)′ = ?",
      "answer": "f(x)",
      "prompt_latex": "\\text{f 连续：}\\frac{d}{dx}\\int_{a}^{x}f(t)\\,dt=?",
      "answer_latex": "f(x)"
    },
    {
      "id": "sine_limit",
      "prompt": "lim(x→0) sin(x)/x = ?",
      "answer": "1",
      "prompt_latex": "\\lim_{x\\to0}\\frac{\\sin(x)}{x}=?",
      "answer_latex": "1",
      "distractors": [
        {
          "text": "0",
          "latex": "0"
        },
        {
          "text": "∞",
          "latex": "\\infty"
        }
      ],
      "true_false": {
        "statement": "lim(x→0) sin(x)/x = 0",
        "statement_latex": "\\lim_{x\\to0}\\frac{\\sin(x)}{x}=0",
        "is_true": false,
        "correction": "✕"
      }
    }
  ]
}
```

## 输出限制与导入后的行为

- 知识包名称为 1 至 80 个字符，介绍最多 2000 个字符；每包最多 2000 道题，文字与包信息不超过 2 MiB。含图 JSON 另受资源限制，普通包仍不超过 2 MiB。制作内容时请至少生成一道有效题；软件也允许用户删除后保留空知识包。
- 无图题面、答案、别名、干扰项文本、判断陈述和非空 LaTeX 字段各为 1 至 1000 个字符；有图题面的文字可以留空，最多 1000 个字符。每题最多 32 个别名，不使用空白占位。
- 导入时会检查 JSON、字段、限制及数学语法，但不自动推导事实或数学是否正确；AI 应先检查内容。
- 新题默认使用填空。`distractors`、`true_false` 表示这道题具备对应内容，不会自动切换其当前作答方式。导入后在主窗口矩阵单题或批量选择填空、选择、判断。已有同 id 题目会保留原本机方式；缺少相应内容时恢复填空。
- 参与范围、当前答题方式、出题间隔、停留时间、暂停、知识展示模式和标签显示偏好不是知识包内容，不能通过添加这些字段配置它们。个人学习记录由 Try 在本机保存，不随知识包导出。
