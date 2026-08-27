# View

`view.js` declares a retained, fixed Pocket Pi UI. It has no DOM. Compose
`View` recipes, mount one render function, and emit Actions from pressables.

## Complete small View

```js
const expenses = View.state([]);

PocketPi.projection.many(
  "SELECT id, label, cents FROM expenses ORDER BY id DESC LIMIT 6",
  {},
  (rows) => expenses.set(rows),
);

View.mount(() => View.Screen({ children: [
  View.Header({
    title: "EXPENSE LOG",
    metaTop: "LOCAL",
    metaBottom: "LOCAL",
    onBack: () => PocketPi.navigate("pi-agent"),
  }),
  View.Column({
    style: { grow: 1, padding: 24, gap: 16 },
    children: expenses.get().length
      ? expenses.get().map((expense) => View.Card({
        children: View.Row({
          style: { justify: "between", align: "center" },
          children: [
            View.Text({ text: expense.label }),
            View.Text({
              text: `$${(expense.cents / 100).toFixed(2)}`,
              style: { fontWeight: "bold" },
            }),
          ],
        }),
      }))
      : View.EmptyState({
        title: "NO EXPENSES YET",
        detail: "ASK PI AGENT TO RECORD ONE",
      }),
  }),
] }));
```

## State and layout

- `View.state(value)` returns `{get, set, update}`. Call `get()` while rendering;
  `set(next)` and `update(patch)` trigger reconciliation.
- Use `Row` or `Column` for multiple flow children. A plain `Box` with multiple
  flow children must set `style.direction` explicitly.
- Numeric geometry uses design units and is scaled once by the SDK. Never
  multiply it by `View.viewport.scale`.
- Prefer flex fields (`grow`, `basis`, `align`, `justify`) and shared components
  over board-specific coordinates.
- `View.viewport.orientation` is `landscape` or `portrait`; use it only when the
  composition genuinely needs two arrangements.

## Typography and styles

`fontSize` accepts exactly `sm`, `md`, `lg`, or `xl`. `fontWeight` accepts
`regular` or `bold`. `View.Text` defaults to `md regular`. There are no numeric
font sizes.

The fixed font supports printable ASCII plus `‘ ’ “ ” — … · • ‹ › ↑ ↓ →`;
there is no emoji or icon fallback. Use View components or ASCII labels for
controls. All displayed text, including projected data, must use this set.

Named colors are: `canvas`, `surface`, `shell`, `shellMuted`, `text`, `heading`,
`muted`, `subtle`, `border`, `disabled`, `white`, `accent`, `accentSoft`, `info`,
`infoSoft`, `success`, `successSoft`, `warning`, `warningText`, `warningSoft`,
`danger`, `dangerSoft`, and `dangerOnDark`.

Common style keys and values:

- size: `width`, `height`, `minWidth`, `minHeight`, `maxWidth`, `maxHeight`;
  dimensions are numbers or `"full"`
- spacing: `padding`, `paddingX/Y`, individual padding, `margin`, `marginX/Y`,
  individual margin, `gap`
- flex: `direction: row|column`, `justify: start|center|end|between|around`,
  `align: start|center|end|stretch`, `grow`, `shrink`, `basis`, `wrap`
- placement: `position: relative|absolute`, `top/right/bottom/left`, `zIndex`
- paint: `background`, `borderColor`, `borderWidth`, `radius`, `opacity`, `shadow`
- text: `color`, `fontSize`, `fontWeight`, `textAlign: left|center|right`,
  `lineHeight`, `tracking`
- clipping/visibility: `overflow: visible|hidden`, `display: flex|none`

The SDK rejects unsupported keys and values during `app.validate`. Search
`pocketpi.d.ts` before inventing one.

## Components

Primitives: `Box`, `Row`, `Column`, `Text`, `Pressable`.

Shared components: `Screen`, `Card`, `Header`, `PageIntro`, `SectionHeading`,
`ActionButton`, `Checkbox`, `Badge`, `EmptyState`, `MetricCard`, `StatusBar`,
`NavigationBar`, `ScrollButton`, `ScrollRail`, `Keyboard`, `Sparkline`.

Use primitives for App-specific composition and shared components for their
named role. `Pressable` requires `onPress`; `ActionButton` requires `label` and
`onPress` unless disabled. `Checkbox` is controlled: pass its current `checked`
value and return the domain Action from `onChange`. Press targets and button
label sizing are enforced by the SDK.

## Input and returned Actions

`PocketPi.action()` creates a command; it does not run the Action by itself.
The press callback and every helper it calls must return that command.

```js
const draft = View.state("");
const layer = View.state("lower");

function save() {
  const text = draft.get().trim();
  if (!text) return "";
  const command = PocketPi.action("createEntry", { text });
  draft.set("");
  return command;
}

function onKey(key) {
  if (key === "Enter") return save();
  if (key === "Backspace") draft.set(draft.get().slice(0, -1));
  else if (key === "Mode") layer.set(layer.get() === "symbols" ? "lower" : "symbols");
  else if (key === "Shift") layer.set(layer.get() === "upper" ? "lower" : "upper");
  else draft.set(draft.get() + key);
  return "";
}

function Editor() {
  return View.Column({ children: [
    View.Text({ text: draft.get() || "TYPE AN ENTRY" }),
    View.Keyboard({ layer: layer.get(), onKey }),
    View.ActionButton({ label: "SAVE", disabled: !draft.get().trim(), onPress: save }),
  ] });
}
```

`Keyboard` sends printable characters plus `Backspace`, `Enter`, `Mode`, and
`Shift`. Store the current layer in View state when switching layers.

## Validate what the Agent can see

Successful `app.validate` returns `screenText`: a spatial character map plus
the exact visible text, real layout bounds, pressable numbers, and any clipped
or offscreen text. Treat it as the rendered UI observation for a text-only
model. Fix overlap, clipping, missing labels and hierarchy, then validate again.
