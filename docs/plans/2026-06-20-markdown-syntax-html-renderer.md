---
date: 2026-06-20
status: completed
---

# 计划：markdown-syntax HTML 渲染器

> **说明：** 通过 `/code-plan` 产出、Design Gate 对抗式评审通过、并由用户拍板两个绑定决策（渲染器置于 opt-in `html` feature 之后；测试采用每节点已知输出 gate）。本文件记录 HTML renderer 的实施路径；当前公开接口契约以 `README.md` 和 `src/html/` 为准，当前 conformance 数字以 `tests/html_conformance/CONFORMANCE.md` 为准。
>
> **来源：** 衍生自 2026-06-20 用户请求（“为 markdown-syntax 添加 html render 功能，并评估测试侧调整”）。本仓库无对应 requirement 文档——需求即会话内请求，故不强行创建 requirement；如需要可补 `requirements/`。

## 澄清状态

两个绑定决策已由用户拍板，决定本计划的范围：

1. **渲染器置于何处** → **B：opt-in `html` cargo feature**（非默认）。该选择同时授权改写 README 中“HTML 渲染不在本 crate”的公开契约（窄化为“不在默认构建中”）。
2. **公开契约测试** → **每节点已知输出 gate**（新建 `tests/html_regressions.rs`，硬编码 expected-HTML）。**不**加 conformance 阈值 gate——bench 维持为 measurement，不把 CI 耦合到既有 parser 缺陷。

其余内部形状（`Result` vs `String`、`HtmlOptions` 字段形态、命名）已由证据定论，见“设计决策”。

## 背景与问题

`markdown-syntax` 是 `no_std + alloc`、零依赖、默认 feature 为空的 Markdown 语法 crate：提供 parse → AST、AST → 规范 Markdown（`to_markdown`）。为了度量 parser 正确性，仓库已在 `tests/html_conformance/renderer/` 内建了一个**完整、忠实的 CommonMark/GFM 参考渲染器**（8 文件、约 1960 行，覆盖全部 19 个 `Block` + 30 个 `Inline` 分支）。实施前观测到的当前一致率是 **98.05%（2216/2260）**、CommonMark 规范 **98.93%（645/652）**。44 个残余失败是既有 parser 缺陷或已记录的 oracle/renderer 长尾项，不是本次移植回归。

因此本任务不是“从零写渲染器”，而是 **promote（提升）+ 去 test-only 化 + 契约改写**：把这份已验证的测试渲染器搬进 `src/`，作为 opt-in 公开能力，并消除“测试私有渲染器 / 公开渲染器”双份漂移。

## 目标

在 `markdown-syntax` 内交付一个**公开、safe-by-default、`no_std + alloc`、零依赖**的 AST→HTML 渲染器，方式是 **promote 现有测试渲染器**——让 conformance bench 成为 shipped 代码的活覆盖，并在**字节一致**的 98.05%/98.93% 一致率下落地。

## 范围

- **纳入：**
  - 新 `src/html/` 模块（从 `tests/html_conformance/renderer/` 提升，`std→alloc` 移植）；非默认 `html` cargo feature；
  - `HtmlOptions` / `HtmlError` / 两个小枚举 + `to_html` / `to_html_with_options` 公开函数（validate-first，镜像 `to_markdown`）；
  - 把 conformance bench 指向公开 API，并**删除**私有渲染器目录；
  - README + doc-comment 契约改写；
  - 新 `tests/html_regressions.rs` 每节点已知输出 gate（已签字）。
- **边界变更分类：** README 公开契约改写 = **已由用户授权（决策 1）**。Workspace/package 边界 = **不变**（B 路径）。Parser / AST / serializer = **不动**。

## 非目标

- 修复既有 parser 缺陷（既有、独立；渲染器本就正确）。
- 可插拔净化策略 / 自定义 allowlist / rewrite hook（仅做 safe-by-default 转义 + 协议 allowlist）。
- 语法高亮、TOC、DOM、编辑器模型、MDX-SWC 求值。
- 不可失败变体 `to_html_unchecked`（后续）。发布（crate 当前 `publish = false`）。

## 关键上下文（实施前先读）

`src/lib.rs`、`src/ast.rs`、`src/serialize.rs:28-80`（镜像的 API 形态）、`src/html/`、`tests/html_conformance/{types,runner}.rs`、`tests/html_conformance.rs`、`tests/html_regressions.rs`、`README.md`、`tests/html_conformance/CONFORMANCE.md`。完整调研证据见本计划会话的 4-agent 输出（promotion-readiness / regression-contract / design-critique / bench-reconciliation）。

## 设计决策（Design Gate，对抗式）

载荷决策 = **渲染器置于何处、如何暴露**。对抗式权衡三个实质不同方案（APOSD 映射）：

- **A — default-on `src/html` 模块**：调用方零配置，但被 **empty-default + zero-dep 不变量**击败（crate 的头号卖点，`Cargo.toml [features] default=[]`）。会把约 1900 行 HTML/XSS-emit 代码静默链进每个只想要 parse/serialize 的精简 `no_std` 消费者。高 change-amplification + 高 unknown-unknown。
- **C — 新建 sibling crate `markdown-html`**：README 字面最契合，但**触发 CLAUDE.md workspace 边界规则**，且把 bench 逼入两难（反向 dev-dep 依赖自己的消费者，或保留私有渲染器副本→重新引入 bench 本欲消除的漂移）。亦与“加到 markdown-syntax”相悖。
- **B — opt-in `html` feature** ✓：同时满足三条硬约束（empty-default、zero-dep、`no_std`），README 从“不在本 crate”**窄化**为“不在默认构建中”，无边界变更，并使测试渲染器与 shipped 渲染器**字节一致**（消除最大漂移风险）。

子决策（已由证据定论）：

- **promote 而非 rebuild**：渲染器忠实、自洽（`DefMap` / `FootnoteContext` 无 test-only 耦合）；rebuild 会浪费且重引入漂移。
- **`HtmlOptions` = 5 个原样 bool + 2 个小枚举**（正交形态，agent D）而非折叠枚举——对 headline 的字节保留风险最低、对渲染器的 diff 最小。
- **`Result<String, HtmlError>` + validate-first gate**：与 `to_markdown` 对齐（渲染器是 total——无 `UnsupportedNode` 分支；agent C 的 MDX 顾虑属于被 fixture 排除的 case，非渲染器分支）。

## 方案

把测试渲染器提升进 `src/html/`，置于 `#[cfg(feature = "html")]` 之后，做三处变换：

1. **`std→alloc`**：`std::{string,vec,format,collections} → alloc::*`（机械、字节中性；完整 file:line 清单已知，例如 `escape.rs:6`、`blocks.rs:4-6`、`inlines.rs:3-4`、`footnotes.rs:16-19,478`、`refs.rs:4-5`、`tables.rs:3-4,41,61,63`）。
2. **去 test-only 耦合**：丢弃 `crate::types::{Category, RenderConfig}`。`RenderConfig` 1:1 折进 `HtmlOptions`；safe-raw-html、tasklist 属性顺序、以及 GFM/cmark-gfm 未知 URI scheme denylist 的 oracle convention 由公开选项承载。
3. **bench 重指**：bench 调 `to_html_with_options`，删除私有渲染器目录。bench 的字节相等（98.05%/98.93%）即本次移植的正确性 oracle。

safe-by-default 的输出转义 + 协议 allowlist **在范围内**（与渲染不可分）；可配置净化**策略**仍在范围外。

## 工作序列

- **Slice 0 — baseline（改前）。** 跑 `cargo test --test html_conformance -- --nocapture`，冻结 98.05%/98.93% + 失败 dump。*目的：字节保留 oracle；区分既有失败与移植回归。*
- **Slice 1 — `src/html` 模块 + feature + 导出（不可分单切）。** 建 `src/html/{mod,blocks,inlines,footnotes,refs,tables,escape}.rs`；套用 `std→alloc`；两处 category 点改为新枚举；定义 `HtmlOptions`（5 bool：`allow_dangerous_html`/`allow_dangerous_protocol`/`allow_any_img_src`/`gfm_tagfilter`/`tasklist_checkable` + `safe_raw_html_form: SafeRawHtmlForm{EscapeText,OmitPlaceholder}` + `tasklist_attr_order: TasklistAttrOrder{DisabledFirst,CheckedFirst}`，`#[non_exhaustive]`，`Default` = 全 false + `EscapeText`/`DisabledFirst`）、`HtmlError{InvalidDocument(Vec<ValidationDiagnostic>)}`、`to_html`/`to_html_with_options`（validate-first）；`Cargo.toml [features] html = []`；`src/lib.rs` 加 `#[cfg(feature="html")] pub mod html;` + 再导出。*证明：`cargo build`（默认）字节不变；`cargo build --features html` 编译通过；默认 `cargo test` 全绿。*
- **Slice 2 — 重指 bench + 删私有渲染器（不可分单切，依赖 Slice 1）。** `tests/html_conformance.rs`：删 `mod renderer`、改写模块 doc。**删除** `tests/html_conformance/renderer/`（8 文件）。`types.rs`：删 `RenderConfig`（保留 `Category` 作 bench 路由标签）。`runner.rs`：从相同 token 构造 `HtmlOptions`（5 flag 1:1）+ 由 `t.category` 派生 oracle convention 选项；调 `to_html_with_options`。*证明（关键 anchor）：`cargo test --features html --test html_conformance` headline == 98.05%（2216/2260）、CommonMark == 98.93%（645/652）、`corpus_counts_match`（652）绿、失败 dump == Slice 0。注意：bench 此后需 `--features html` 才编译——需文档化。*
- **Slice 3 — README + CONFORMANCE.md 契约改写（与 Slice 2 并行）。** 套用 README 改写（行 3-9、45-46、54-57、64-67）：引入 opt-in 渲染器、声明 safe-by-default、窄化为“不在默认构建中”、确认 `no_std` 对 `--all-features` 仍成立；更新 CONFORMANCE.md 行 26-28。
- **Slice 4 — `tests/html_regressions.rs`（已签字）。** 每节点族已知输出 `#[test]`（硬编码 expected HTML、零依赖），覆盖全部 Block/Inline 族 + 两种 category 约定 + autolink 不对称 dest。**不含** conformance 阈值 gate。
- **Slice 5 — memory 清理。** 更新 `MEMORY.md` 中已过时的 markdown-syntax 条目（旧的 no-renderer 契约文字）。

## 验收与回归证据

- `cargo build`（默认）——crate 表面与今天字节一致（无 `html` 符号；`no_std`+zero-dep+empty-default 保留）。← **回归：精简默认不变量**
- `cargo build --features html`——`to_html`/`to_html_with_options`/`HtmlOptions`/`HtmlError`/`SafeRawHtmlForm`/`TasklistAttrOrder` 公开。← forward
- `cargo test --features html --test html_conformance`——**headline 98.05% / CommonMark 98.93%，失败 dump == Slice 0**，`corpus_counts_match` 652。← **forward + 字节保留证明**
- `cargo test`（默认）——所有 parse/serialize/validate/fixtures 测试全绿、不变。← **回归：既有行为**
- README/CONFORMANCE/docs 中无未窄化的旧 no-renderer 契约文字。← **回归：契约一致性**
- `cargo test --features html --test html_regressions` 全绿。← forward

**回归缺口：** 当前对任何公开 HTML 输出契约**零覆盖**（bench 是 measurement、不断言）。缺口由 Slice 4 关闭。

## 风险与陷阱

- **Headline 漂移（最高）**——遏制：Slice 0 baseline + Slice 2 字节相等 gate。5 个 `HtmlOptions` 默认**必须**全 false（= `RenderConfig::new`）；`std→alloc` 必须字节中性；两枚举必须逐字节复现两处约定点（agent D 的硬编码约定清单——`<hr />`、code block 尾部 `\n`、`language-`/`data-math-style`、转义顺序 `& < > "`、houdini href 字节集、协议 allowlist、footnote 形态——即移植 checklist）。
- **feature-gate 泄漏**——遏制：`cargo build`（默认）编译干净即证明无非 gate 的 `src/` 文件引用 `html::`。
- **bench 此后需 `--features html`**——遏制：文档化 + 更新运行命令 + gate 测试目标。
- **扩展节点渲染意外**（MDX/directive/shortcode）——遏制：Slice 4 每节点测试；非 headline 风险（这些 case 被 fixture 排除）。
- **memory 过时**——遏制：Slice 5。

## 检查点

- Slice 2 前：Slice 0 baseline 已捕获。Slice 4 前：Slice 2 headline 确认字节一致。README 改写前：决策 1 已授权（✓）。

## 停止条件

Slice 0–3 落地；headline 在 98.05%/98.93% 字节一致；默认 feature 构建与今天字节一致；README 已改写。Slice 4 已签字。Slice 5（memory）完成或确认无目标文件。

## 暂停条件

- **(P1)** README 契约改写——需用户授权（决策 1，**已授权 ✓**）。
- **(P2)** 加测试（Slice 4）——CLAUDE.md，需用户签字（决策 2，**已签字 ✓**）。
- **(P3)** 若 Slice 2 headline 漂移且二分指向真实的 渲染器-vs-私有 差异（非机械移植伪影）——暂停，决定接受 re-baseline 还是修复。

## 后续（非本计划范围）

- 把“opt-in `html` feature、promote 测试渲染器、否决 default-on 与 sibling crate”这一架构结论 promote 为 `decisions/` 记录（候选；用户未要求，故未创建）。
- 用 `/code-tasking` 把本计划转为依赖序原子任务图。
