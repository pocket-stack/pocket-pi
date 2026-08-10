# Pocket Pi Design System inventory

状态：v0.1，刻意保持小型。这里记录哪些 UI 已经成为 Pocket Pi 全局设计语言，
避免在单个 App 中复制后再逐渐漂移。

实现位于 `apps/_shared/ui.tsx` 和 `apps/_shared/text.ts`。它只能依赖 PocketJS framework primitives，不能
调用 ESP-IDF、Pocket Pi Rust 私有接口、网络或 SQLite。App-specific data binding
和交互仍由各 App 自己拥有。

## v0.1 tokens

| Token group | 当前内容 | 使用范围 |
| --- | --- | --- |
| `type` | title、section、body、bodyStrong、caption | Pi Agent、Robinhood |
| `space` | 24px screen gutter、card padding、stack gap | Pi Agent、Robinhood |
| semantic colors | slate shell/surfaces、orange primary、green ready/positive、red error | shared components |
| dynamic prose glyphs | curly quotes、en/em dash、ellipsis、bullet | model/provider text |

当前统一使用 PocketJS bundle 内的 Inter font atlas；v0.1 只统一字号、字重和行文
层级，不新增字体或 native font API。`text.ts` 的 glyph seed 只让常见动态英文标点
进入现有 subset atlas，不引入第二套字体或 runtime font loader。

## v0.1 components

| Component | 责任 | 明确不负责 |
| --- | --- | --- |
| `PocketHeader` | 112px system header、title、back/status affordance、两行 metadata | App data、navigation side effects |
| `SectionHeading` | section title、optional detail/action label | list/query behavior |
| `ActionButton` | primary/disabled visual state、minimum touch surface supplied by caller | 执行动作 |
| `wrapLines` / `wrapPreview` | 用 PocketJS `measureText` 给动态文本插入显式换行，并缓存测量结果 | rich text、Markdown、滚动状态 |

Card 在 v0.1 只记录为视觉 recipe：`rounded-xl + white + slate border + shadow`。
当前 PocketJS/Solid Guest 对跨模块传递 arbitrary children 的 generic wrapper 仍会
产生额外 runtime 风险，因此 App 直接在自己的 `View` literal 上使用这组样式；不
为“组件化”保留一层不稳定 wrapper。

## 仍然属于 App 的组件

- Pi Agent conversation turn、Bottom Navigation、keyboard、Files row；
- Robinhood 20-point chart、time-range selector、account selector、portfolio metric、
  activity row、position row；
- Exa search-history projection。

只有至少两个 App 需要、语义稳定、并且能完全建立在 PocketJS public UI contract
之上的组件，才进入 Design System。

## 演进规则

1. 每次抽出或删除 component/token，都更新本 inventory 和 consumer。
2. v1 仍由 TS/TSX 编译为各 App 的 `app.js`/`app.pak`。
3. 未来 declarative View schema 只能组合 inventory 中明确注册的稳定组件；在
   schema 和校验边界被单独设计前，不在当前 runtime 中加入 interpreter。
4. Marketplace 或独立 SDK 真正出现后，再把该目录提取为版本化组件包；当前不
   预先实现 package registry、compatibility resolver 或 runtime download。
