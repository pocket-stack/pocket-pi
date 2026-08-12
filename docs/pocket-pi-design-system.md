# Pi Design component inventory

状态：v0.2。Pi Design 是 Pocket Pi 的全局视觉语言；内置 App 不再各自维护一套
Header、按钮、空状态或字号层级。

实现位于 `apps/_shared/ui.tsx` 和 `apps/_shared/text.ts`。共享层只能依赖
PocketJS framework primitives，不能调用 ESP-IDF、Pocket Pi Rust 私有接口、
网络、SQLite 或 App navigation。所有业务数据和 side effect 仍由 App 提供。

## Foundations

| Foundation | 包含 | 当前规则 |
| --- | --- | --- |
| `type` | app title、page title、heading、label、body、caption | ESP 主阅读文本由 14/16px 提升到 16/18px；24px title 保持不变 |
| `space` | 24px screen gutter、card padding、stack gap | 720x1280 screen density |
| `surface` | card、selected card、row、muted、shell recipes | arbitrary children 的容器保持 literal View，不增加不稳定 wrapper |
| `Tone` | neutral、info、success、warning、danger | 统一背景色、文字色和状态语义 |
| dynamic glyphs | curly quotes、en/em dash、ellipsis、bullet | 进入现有 Inter subset atlas |

PocketJS 当前只提供 12 / 14 / 16 / 18 / 20 / 24 / 36px 的 baked font
slots。Pi Design 的“稍微放大”因此采用相邻 slot：caption/label 14→16px，
body 16→18px；不会引入 runtime font loader 或第二套字体。

## Components

| Component | 包含什么 | 不包含什么 | 当前使用者 |
| --- | --- | --- | --- |
| `PocketHeader` | 112px shell header、back/status affordance、App title、两行 metadata | navigation side effect、App data loading | Pi Agent、Exa、Robinhood |
| `PageIntro` | eyebrow、page title、一行说明 | 页面 query、filter、scroll | Exa |
| `SectionHeading` | section title、optional detail、optional `VIEW ALL` affordance | list data、tap behavior | Robinhood |
| `ActionButton` | primary/neutral/danger/disabled visual state、统一 18px label | action execution、loading state machine | Pi Agent、Robinhood |
| `statusBadge` recipe | 短 label 的 neutral/info/success/warning/danger surface + text classes | 长状态文案、业务判断 | Exa result、Robinhood account status |
| `EmptyState` | optional icon、title、detail、regular/compact layout | empty 条件、retry action | Exa、Robinhood |
| `MetricCard` | metric label、formatted value、optional positive/negative tone | 数值计算、currency formatting、time range | Robinhood value/cash/buying power |
| `StatusBar` | 单行 runtime status、neutral/error tone、light/dark surface | log history、progress model、retry | Exa SQLite state、Robinhood refresh state |
| `ScrollButtons` | UP/DN controls 的统一文字和视觉 | offset、pagination、tap hit testing | Pi Agent、Exa、Robinhood |
| `wrapLines` / `wrapPreview` / `wrapTextPage` | `measureText` 缓存、显式换行、preview ellipsis、按字符游标只物化可见文件页 | rich text、Markdown、scroll state | Pi Agent dynamic text / Files viewer |

## App-owned components

这些组件仍含明确业务语义，不进入 Pi Design：

- Pi Agent conversation turn、Bottom Navigation、keyboard、Files row、App row；
- Robinhood 20-point chart、time-range selector、account picker、activity row、
  position row、P&L projection；
- Exa search-history query、result projection 和 retention state。

如果一个模式仅在一个 App 中出现，或需要知道 Robinhood account / Exa result
schema，它先留在 App。只有语义稳定、可复用且完全建立在 PocketJS public UI
contract 上，才进入 Pi Design。

## Evolution rules

1. 每次新增、删除或改变 component/token，都同步更新本 inventory 和 consumers。
2. 共享 component 是纯展示函数：props in，native View/Text tree out。
3. App 自己决定数据、loading/error 条件、tap hitbox、navigation 和 side effects。
4. v1 仍由 TS/TSX 编译进各 App 的 `app.js` / `app.pak`；Marketplace/SDK 出现后
   再提取为版本化 package，不预先实现 registry 或 runtime download。
