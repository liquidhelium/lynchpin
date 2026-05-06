// ============================================================
// typterm terminal export — 全样式测试文件
// 每个区块对应 convert.rs 中一类元素的处理逻辑
// 运行方式（从项目根目录）：
//   cargo run --example cli -- tests/test.typ
//
// ✅ 已验证正常工作：
//   标题（1-4级颜色）、段落、强制换行、粗体/斜体/下划线/删除线/上划线、
//   高亮、上标/下标、智能引号、链接（cyan+下划线）、引用块、行内代码、
//   无序列表（markup 语法 - 和函数调用 #list）、
//   有序列表（markup 语法 + 和函数调用 #enum(start:N)）、
//   术语列表（markup 语法 / 和函数调用 #terms）、
//   嵌套样式、H/V 间距、Box 容器、Block 容器、分页符、文字大小写变换
// ============================================================

// ── 1. 标题（Heading）─────────────────────────────────────
// level 1 → Yellow + Bold
= 一级标题：黄色加粗

// level 2 → Cyan + Bold
== 二级标题：青色加粗

// level 3 → Green + Bold
=== 三级标题：绿色加粗

// level 4+ → Blue + Bold
==== 四级标题：蓝色加粗

// // ── 2. 普通段落（Paragraph / ParbreakElem）────────────────
// 这是一段普通文本，没有任何样式修饰。
// 文字中间换行后在同一段落内显示（软换行）。

// 这是第二段，与上面的段落之间有段落间距（ParbreakElem）。

// // ── 3. 强制换行（LinebreakElem）────────────────────────────
// 第一行 \
// 第二行（通过反斜杠触发 LinebreakElem）

// // ── 4. 行内样式（StrongElem / EmphElem / etc.）────────────
// *粗体文字*：StrongElem → Attribute::Bold

// _斜体文字_：EmphElem → Attribute::Italic

// #underline[下划线文字]：UnderlineElem → Attribute::Underlined

// #strike[删除线文字]：StrikeElem → Attribute::CrossedOut

// #overline[上划线文字]：OverlineElem → Attribute::OverLined

// #highlight[高亮文字（黄色背景）]：HighlightElem → background Yellow

// // ── 5. 上标与下标（SubElem / SuperElem）────────────────────
// 化学式：H#sub[2]O（SubElem，原样渲染）
// 数学：x#super[2] + y#super[2]（SuperElem，原样渲染）

// // ── 6. 智能引号（SmartQuoteElem）───────────────────────────
// 他说："你好，世界！"（double → `"`）
// 她回答：'没问题。'（single → `'`）

// // ── 7. 无序列表（ListElem）─────────────────────────────────
// // 列表项前缀 "• " 以 Cyan 颜色显示
// - 苹果
// - 香蕉
// - 樱桃

// // ── 8. 有序列表（EnumElem）─────────────────────────────────
// // 数字 "N. " 以 Cyan 颜色显示，自动递增
// + 第一步：打开终端
// + 第二步：运行命令
// + 第三步：查看输出

// // 显式指定起始编号
// #enum(start: 5)[五号项][六号项][七号项]

// // ── 9. 术语列表（TermsElem）────────────────────────────────
// // term 以 Bold 显示，后接 ": " 及描述
// / 粗体: Bold 属性——使文字加粗
// / 斜体: Italic 属性——使文字倾斜
// / 高亮: Background Yellow——使文字背景变黄

// // ── 10. 链接（LinkElem）────────────────────────────────────
// // 链接文字以 Underlined + Cyan 显示
// 访问 #link("https://typst.app")[Typst 官网]（带显示文本）。

// 裸 URL（body = url 本身）：#link("https://github.com/typst/typst")

// // ── 11. 引用块（QuoteElem）─────────────────────────────────
// // 包裹在 `"…"` 中，以 Italic + DarkGrey 显示
// #quote[这是一段引用的内容，以斜体深灰色渲染。]

// #quote[
//   多行引用：第一行。
//   第二行仍属于同一引用块。
// ]

// // ── 12. 行内代码 / 代码块（RawElem）───────────────────────
// // inline raw → Green，不换行；block raw → Green，前后各加换行
// 行内代码：`let x = 42;`（绿色）

// 代码块（block raw）：

// ```rust
// fn main() {
//     let msg = "Hello, terminal!";
//     println!("{msg}");
// }
// ```

// ```python
// def greet(name: str) -> str:
//     return f"Hello, {name}!"

// print(greet("typterm"))
// ```

// 无语言标注的代码块：

// ```
// plain text code block
// line two
// ```

// // ── 13. 嵌套行内样式（Nested inline styles）────────────────
// *粗体中包含 _粗体+斜体_ 再回到粗体*

// _斜体中包含 #underline[斜体+下划线] 再回到斜体_

// #underline[*#strike[下划线+粗体+删除线（三层嵌套）]*]

// #highlight[#underline[高亮+下划线]]

// // ── 14. 水平 / 垂直间距（HElem / VElem）───────────────────
// 左边 #h(1em) 右边（HElem 非零间距 → 输出一个空格）

// #h(0pt) 零宽 HElem（不输出空格）

// 上面

// #v(1em)

// 下面（VElem → 换行）

// // ── 15. Box 容器（BoxElem）─────────────────────────────────
// // Box body 的内容直接递归渲染，不添加额外换行
// 行内 #box[_box 中的斜体_] 继续正文。

// // ── 16. Block 容器（BlockElem）─────────────────────────────
// // Block body 后自动追加一个换行
// #block[block 包裹的文字，后面追加换行]
// block 后的正文。

// // ── 17. 分页符（PagebreakElem）─────────────────────────────
// // 等效于 "\n\n"（双换行）
// #pagebreak()
// 分页符后的内容。

// // ── 18. 文字大小写变换（TextElem::case）─────────────────────
// #upper[uppercase text via case transform]
// #lower[LOWERCASE TEXT VIA CASE TRANSFORM]

// // ── 19. 综合段落（混合所有样式）───────────────────────────
// = 综合测试段落

// 这段文字*加粗*、_斜体_、#underline[下划线]、#strike[删除线]
// 交替出现，附有行内代码 `foo()` 和
// #link("https://example.com")[示例链接]（青色下划线）。

// 带样式的列表：

// - #highlight[高亮项]
// - *粗体* 与 _斜体_ 混合
// - `代码` + #underline[underline]

// 术语+嵌套样式：

// / 术语一: _斜体_ 描述，带 #highlight[高亮]
// / 术语二: *粗体* 描述，带 #strike[删除线]

// #quote[
//   引用中的 *粗体*、`代码` 与 #link("https://example.com")[链接]。
// ]
