# Pi Design component inventory

状态：v0.3。Pi Design 是 Pocket Pi Apps 的全局视觉语言；各 App 不再各自维护一套
Header、按钮、空状态或字号层级。

Pi Agent 的 build-time TSX recipes 位于 `apps/pi-agent/ui.tsx`；普通 Source App
使用 `system/view-sdk.js` 中的 `View.*` components。`apps/pi-agent/text.ts` 只服务
Pi Agent 的动态文本。两层都不拥有网络、SQLite、navigation 或业务状态。

## Foundations

| Foundation | 包含 | 当前规则 |
| --- | --- | --- |
| type hierarchy | app title、page title、heading、label、body、caption | 只保留 shared components 实际消费的 class recipe；ESP 主阅读文本由 14/16px 提升到 16/18px，24px title 保持不变 |
| spacing | 24px screen gutter、card padding、stack gap | 直接写在当前 concrete component recipe 中，不导出无人消费的 token map |
| surfaces | card、selected card、row、muted、shell recipes | arbitrary children 的容器保持 literal View；当前没有单独的 unused surface registry |
| status tones | neutral、info、success、warning、danger | 由 runtime View SDK 的 `Badge`/`StatusBar` 统一表达 |
| dynamic glyphs | curly quotes、non-breaking/en/em dash、ellipsis、bullet | 进入现有 Inter subset atlas |

PocketJS 当前只提供 12 / 14 / 16 / 18 / 20 / 24 / 36px 的 baked font
slots。Pi Design 的“稍微放大”因此采用相邻 slot：caption/label 14→16px，
body 16→18px；不会引入 runtime font loader 或第二套字体。

## Components

| Component | 包含什么 | 不包含什么 | 当前使用者 |
| --- | --- | --- | --- |
| `PocketHeader` / `View.Header` | 112px shell header、back/status affordance、App title、两行 metadata | App data loading | Pi Agent / ordinary Apps |
| `PageIntro` / `View.PageIntro` | eyebrow、page title、一行说明 | 页面 query、filter、scroll | Pi Agent / ordinary Apps |
| `SectionHeading` / `View.SectionHeading` | section title、optional detail、optional `VIEW ALL` affordance | list data | Pi Agent / ordinary Apps |
| `ActionButton` / `View.ActionButton` | primary/neutral/danger/disabled visual state | action execution、loading state machine | Pi Agent / ordinary Apps |
| `View.Badge` | 短 label 的 status surface + text | 长状态文案、业务判断 | ordinary Apps |
| `View.EmptyState` | optional icon、title、detail、regular/compact layout | empty 条件、retry action | ordinary Apps |
| `View.MetricCard` | metric label、formatted value、optional tone | 数值计算、currency formatting | ordinary Apps |
| `StatusBar` / `View.StatusBar` | 单行 runtime status、neutral/error tone | log history、progress model、retry | Pi Agent / ordinary Apps |
| `ScrollButtons` / `View.ScrollButton` | UP/DN controls 的统一文字和视觉 | offset、pagination | Pi Agent / ordinary Apps |
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
4. Pi Agent 继续编译为 firmware-embedded System Bundle；普通 App 直接发布
   `view.js`，并在 runtime 中使用固定的 View SDK API。
