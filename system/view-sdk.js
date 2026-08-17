(() => {
  if (!globalThis.ui) throw new Error("Pocket Pi View SDK: ui surface is unavailable");
  if (!globalThis.PocketPi) throw new Error("Pocket Pi View SDK: System Framework is unavailable");
  if (globalThis.View) throw new Error("Pocket Pi View SDK: View is already installed");
  const ui = globalThis.ui;

  const ROOT = 1;
  const NODE_VIEW = 0;
  const NODE_TEXT = 1;
  const FULL = -1;
  const PROP = Object.freeze({
    width: 1, height: 2, minWidth: 3, minHeight: 4, maxWidth: 5, maxHeight: 6,
    paddingTop: 8, paddingRight: 9, paddingBottom: 10, paddingLeft: 11,
    marginTop: 12, marginRight: 13, marginBottom: 14, marginLeft: 15,
    gap: 16, direction: 17, justify: 18, align: 19, grow: 20, shrink: 21, basis: 22,
    wrap: 23, position: 24, top: 25, right: 26, bottom: 27, left: 28,
    display: 29, overflow: 30, zIndex: 31, hitPass: 32,
    background: 64, radius: 68, opacity: 69, borderColor: 70, borderWidth: 71,
    shadow: 72, color: 96, fontSlot: 97, textAlign: 98, lineHeight: 99,
    tracking: 100, translateX: 128, translateY: 129, scale: 130, rotate: 131,
    scaleX: 132, scaleY: 133, originX: 134, originY: 135,
    arcStart: 140, arcSweep: 141, arcWidth: 142,
  });

  const abgr = (hex) => {
    const value = Number.parseInt(hex.slice(1), 16);
    return (0xff000000 | (value & 0xff00) | ((value & 0xff) << 16) | ((value >>> 16) & 0xff)) >>> 0;
  };
  const colors = Object.freeze({
    canvas: abgr("#f8fafc"), surface: abgr("#ffffff"), shell: abgr("#020617"), shellMuted: abgr("#1e293b"),
    text: abgr("#020617"), heading: abgr("#0f172a"), muted: abgr("#64748b"),
    subtle: abgr("#94a3b8"), border: abgr("#f1f5f9"), disabled: abgr("#e2e8f0"),
    white: abgr("#ffffff"), accent: abgr("#ea580c"), accentSoft: abgr("#ffedd5"),
    info: abgr("#4338ca"), infoSoft: abgr("#e0e7ff"),
    success: abgr("#059669"), successSoft: abgr("#d1fae5"),
    warning: abgr("#fbbf24"), warningText: abgr("#b45309"), warningSoft: abgr("#fef3c7"),
    danger: abgr("#ef4444"), dangerSoft: abgr("#fee2e2"), dangerOnDark: abgr("#fca5a5"),
  });
  const fontSlots = Object.freeze({
    regular: Object.freeze({ body: 2, lg: 3, xl: 4 }),
    bold: Object.freeze({ body: 9, lg: 10, xl: 11, title: 12 }),
  });

  let rootNode = null;
  let renderView = null;
  let dirty = false;
  let pressables = new Map();
  let parents = new Map();
  let pressed = null;
  let activeEffect = null;

  function clearEffect(effect) {
    for (const subscribers of effect.sources) subscribers.delete(effect);
    effect.sources.clear();
  }

  function track(effect, read) {
    clearEffect(effect);
    const previous = activeEffect;
    activeEffect = effect;
    try {
      return read();
    } finally {
      activeEffect = previous;
    }
  }

  const rootEffect = { sources: new Set(), run() { dirty = true; } };

  function fail(message) {
    throw new Error(`Pocket Pi View SDK: ${message}`);
  }

  function children(value, output = []) {
    if (Array.isArray(value)) {
      for (const child of value) children(child, output);
    } else if (value !== undefined && value !== null && value !== false) {
      output.push(typeof value === "string" || typeof value === "number" ? Text(String(value)) : value);
    }
    return output;
  }

  function element(type, props, content) {
    return { type, props: props ?? {}, children: children(content) };
  }

  function Box(props = {}) {
    const recipe = element(NODE_VIEW, props, props.children);
    if (props.style?.direction === undefined && recipe.children.length > 1) {
      let flowChildren = 0;
      for (const child of recipe.children) {
        if (child.props.style?.position !== "absolute" && ++flowChildren > 1) {
          fail("multiple flow children require Row, Column, or an explicit direction");
        }
      }
    }
    return recipe;
  }

  function Row(props = {}) {
    return Box({ ...props, style: { direction: "row", ...props.style } });
  }

  function Column(props = {}) {
    return Box({ ...props, style: { direction: "column", ...props.style } });
  }

  function Text(props = {}) {
    if (typeof props === "string" || typeof props === "number") props = { text: String(props) };
    const content = props.text ?? props.children ?? "";
    return element(NODE_TEXT, {
      ...props,
      text: typeof content === "function" ? content : String(content),
      style: { color: "text", fontSize: "body", ...props.style },
    });
  }

  function Pressable(props = {}) {
    if (typeof props.onPress !== "function") fail("Pressable requires onPress");
    return Box(props);
  }

  function state(initial) {
    let value = initial;
    const subscribers = new Set();
    const get = () => {
      if (activeEffect) {
        subscribers.add(activeEffect);
        activeEffect.sources.add(subscribers);
      }
      return value;
    };
    const set = (next) => {
      const resolved = typeof next === "function" ? next(value) : next;
      if (!Object.is(value, resolved)) {
        value = resolved;
        for (const effect of [...subscribers]) effect.run();
      }
      return value;
    };
    return Object.freeze({
      get,
      set,
      update(patch) {
        if (!value || typeof value !== "object" || Array.isArray(value)) fail("state.update requires object state");
        return set({ ...value, ...patch });
      },
    });
  }

  function color(value) {
    if (typeof value === "number") return value >>> 0;
    if (Object.hasOwn(colors, value)) return colors[value];
    fail(`unknown color ${String(value)}`);
  }

  function dimension(value) {
    if (value === "full") return FULL;
    if (typeof value !== "number" || !Number.isFinite(value)) fail(`invalid dimension ${String(value)}`);
    return value;
  }

  function enumValue(kind, value) {
    const values = {
      direction: { row: 0, column: 1 },
      justify: { start: 0, center: 1, end: 2, between: 3, around: 4 },
      align: { start: 0, center: 1, end: 2, stretch: 3 },
      position: { relative: 0, absolute: 1 },
      display: { flex: 0, none: 1 },
      overflow: { visible: 0, hidden: 1 },
      textAlign: { left: 0, center: 1, right: 2 },
    }[kind];
    if (values && Object.hasOwn(values, value)) return values[value];
    fail(`invalid ${kind} ${String(value)}`);
  }

  function edges(style, start, value) {
    style[start] = value;
    style[start + 1] = value;
    style[start + 2] = value;
    style[start + 3] = value;
  }

  function nativeStyle(value) {
    const style = {};
    let fontSize;
    let bold = false;
    for (const [name, item] of Object.entries(value ?? {})) {
      if (item === undefined || item === null) continue;
      if (name === "padding") edges(style, PROP.paddingTop, item);
      else if (name === "paddingX") { style[PROP.paddingLeft] = item; style[PROP.paddingRight] = item; }
      else if (name === "paddingY") { style[PROP.paddingTop] = item; style[PROP.paddingBottom] = item; }
      else if (name === "margin") edges(style, PROP.marginTop, item);
      else if (name === "marginX") { style[PROP.marginLeft] = item; style[PROP.marginRight] = item; }
      else if (name === "marginY") { style[PROP.marginTop] = item; style[PROP.marginBottom] = item; }
      else if (name === "width" || name === "height" || name === "minWidth" || name === "minHeight" || name === "maxWidth" || name === "maxHeight") style[PROP[name]] = dimension(item);
      else if (name === "background" || name === "color" || name === "borderColor") style[PROP[name]] = color(item);
      else if (["direction", "justify", "align", "position", "display", "overflow", "textAlign"].includes(name)) style[PROP[name]] = enumValue(name, item);
      else if (name === "wrap" || name === "hitPass") style[PROP[name]] = item ? 1 : 0;
      else if (name === "fontSize") fontSize = item;
      else if (name === "fontWeight") {
        if (item !== "regular" && item !== "bold") fail(`unknown font weight ${String(item)}`);
        bold = item === "bold";
      }
      else if (Object.hasOwn(PROP, name)) style[PROP[name]] = item;
      else fail(`unsupported style ${name}`);
    }
    if (fontSize !== undefined || bold) {
      const slot = fontSlots[bold ? "bold" : "regular"][fontSize ?? "body"];
      if (slot === undefined) fail(`unknown font size ${String(fontSize)}`);
      style[PROP.fontSlot] = slot;
    }
    return style;
  }

  function measureText(text, style = {}) {
    if (typeof ui.measureText !== "function") fail("text measurement is unavailable");
    const slot = nativeStyle(style)[PROP.fontSlot] ?? fontSlots.regular.body;
    return ui.measureText(String(text), slot);
  }

  function prepare(recipe) {
    if (!recipe || (recipe.type !== NODE_VIEW && recipe.type !== NODE_TEXT)) fail("render must return a View recipe");
    recipe.nativeStyle = nativeStyle(recipe.props.style);
    for (const child of recipe.children) prepare(child);
    return recipe;
  }

  function applyStyle(node, style) {
    for (const prop in style) ui.setProp(node, Number(prop), style[prop]);
  }

  function bindText(node, source) {
    if (node.textEffect) clearEffect(node.textEffect);
    node.textSource = source;
    const apply = (value) => {
      const text = String(value ?? "");
      if (node.text === text) return;
      (node.text === null ? ui.setText : ui.replaceText ?? ui.setText)(node.id, text);
      node.text = text;
    };
    if (typeof source !== "function") {
      node.textEffect = null;
      apply(source);
      return;
    }
    const effect = { sources: new Set(), run() { apply(track(effect, source)); } };
    node.textEffect = effect;
    effect.run();
  }

  function dispose(node) {
    if (node.textEffect) clearEffect(node.textEffect);
    for (const child of node.children) dispose(child);
  }

  function destroy(node) {
    dispose(node);
    ui.destroyNode(node.id);
  }

  function materialize(recipe, nextPressables, nextParents, created) {
    const id = ui.createNode(recipe.type);
    if (id <= 0) fail("ui node allocation failed");
    applyStyle(id, recipe.nativeStyle);
    const node = {
      id,
      type: recipe.type,
      style: recipe.nativeStyle,
      text: null,
      textSource: null,
      textEffect: null,
      children: [],
    };
    created.push(node);
    if (recipe.type === NODE_TEXT) bindText(node, recipe.props.text);
    if (recipe.props.onPress) nextPressables.set(id, {
      onPress: recipe.props.onPress,
      background: recipe.nativeStyle[PROP.background],
      opacity: recipe.nativeStyle[PROP.opacity] ?? 1,
    });
    for (const child of recipe.children) {
      const childNode = materialize(child, nextPressables, nextParents, created);
      node.children.push(childNode);
      nextParents.set(childNode.id, id);
      ui.insertBefore(id, childNode.id, 0);
    }
    return node;
  }

  function styleWasRemoved(before, next) {
    for (const prop in before) {
      if (!Object.hasOwn(next, prop)) return true;
    }
    return false;
  }

  function reconcile(parent, current, recipe, nextPressables, nextParents, created) {
    if (current.type !== recipe.type || styleWasRemoved(current.style, recipe.nativeStyle)) {
      const replacement = materialize(recipe, nextPressables, nextParents, created);
      ui.insertBefore(parent, replacement.id, current.id);
      destroy(current);
      return replacement;
    }

    for (const prop in recipe.nativeStyle) {
      const value = recipe.nativeStyle[prop];
      if (!Object.is(current.style[prop], value)) ui.setProp(current.id, Number(prop), value);
    }
    if (recipe.type === NODE_TEXT && current.textSource !== recipe.props.text) bindText(current, recipe.props.text);
    if (recipe.props.onPress) nextPressables.set(current.id, {
      onPress: recipe.props.onPress,
      background: recipe.nativeStyle[PROP.background],
      opacity: recipe.nativeStyle[PROP.opacity] ?? 1,
    });

    const nextChildren = [];
    const count = Math.max(current.children.length, recipe.children.length);
    for (let index = 0; index < count; index += 1) {
      const child = current.children[index];
      const childRecipe = recipe.children[index];
      if (child && childRecipe) {
        const nextChild = reconcile(current.id, child, childRecipe, nextPressables, nextParents, created);
        nextChildren.push(nextChild);
        nextParents.set(nextChild.id, current.id);
      } else if (childRecipe) {
        const nextChild = materialize(childRecipe, nextPressables, nextParents, created);
        nextChildren.push(nextChild);
        nextParents.set(nextChild.id, current.id);
        ui.insertBefore(current.id, nextChild.id, 0);
      } else if (child) {
        destroy(child);
      }
    }
    current.style = recipe.nativeStyle;
    current.children = nextChildren;
    return current;
  }

  function renderIfDirty() {
    if (!dirty || !renderView) return;
    const nextPressables = new Map();
    const nextParents = new Map();
    const created = [];
    try {
      const recipe = prepare(track(rootEffect, renderView));
      if (rootNode) rootNode = reconcile(ROOT, rootNode, recipe, nextPressables, nextParents, created);
      else {
        rootNode = materialize(recipe, nextPressables, nextParents, created);
        ui.insertBefore(ROOT, rootNode.id, 0);
      }
      pressables = nextPressables;
      parents = nextParents;
      if (pressed) {
        const current = pressables.get(pressed.id);
        if (current) {
          const next = pressedState(pressed.id, current);
          if (pressed.prop !== next.prop) ui.setProp(pressed.id, pressed.prop, pressed.base);
          ui.setProp(next.id, next.prop, next.value);
          pressed = next;
        } else {
          pressed = null;
        }
      }
      dirty = false;
    } catch (error) {
      for (let index = created.length - 1; index >= 0; index -= 1) {
        if (created[index].textEffect) clearEffect(created[index].textEffect);
        ui.destroyNode(created[index].id);
      }
      throw error;
    }
  }

  function pressableAt(x, y) {
    const hitTest = ui.hitTestBounds ?? ui.hitTest;
    if (typeof hitTest !== "function") fail("ui hit testing is unavailable");
    let node = hitTest(x, y);
    while (node) {
      if (pressables.has(node)) return node;
      node = parents.get(node) ?? 0;
    }
    return 0;
  }

  function darken(value) {
    const channel = (shift) => Math.round(((value >>> shift) & 0xff) * 0.82) << shift;
    return ((value & 0xff000000) | channel(0) | channel(8) | channel(16)) >>> 0;
  }

  function pressedState(id, pressable) {
    return pressable.background === undefined
      ? { id, prop: PROP.opacity, base: pressable.opacity, value: pressable.opacity * 0.65 }
      : { id, prop: PROP.background, base: pressable.background, value: darken(pressable.background) };
  }

  function pointerDown(x, y) {
    pointerUp();
    const id = pressableAt(x, y);
    if (!id) return "";
    pressed = pressedState(id, pressables.get(id));
    ui.setProp(pressed.id, pressed.prop, pressed.value);
    return "";
  }

  function pointerUp() {
    if (!pressed) return "";
    ui.setProp(pressed.id, pressed.prop, pressed.base);
    pressed = null;
    return "";
  }

  function pressAt(x, y) {
    const id = pressableAt(x, y);
    return id ? pressables.get(id).onPress() ?? "" : "";
  }

  function mount(render) {
    if (renderView) fail("View already mounted");
    if (typeof render !== "function") fail("View.mount requires a render function");
    renderView = render;
    dirty = true;
    PocketPi.defineView({
      tick() { renderIfDirty(); return ""; },
      dataChanged() { renderIfDirty(); return ""; },
      pointerDown,
      pointerUp,
      tap(x, y) { return pressAt(x, y); },
    });
    renderIfDirty();
  }

  const tone = (name) => ({
    neutral: ["border", "muted"], info: ["infoSoft", "info"],
    success: ["successSoft", "success"], warning: ["warningSoft", "warningText"],
    danger: ["dangerSoft", "danger"],
  }[name ?? "neutral"] ?? fail(`unknown tone ${name}`));

  function Screen(props = {}) {
    return Column({ ...props, style: { width: "full", height: "full", background: "canvas", overflow: "hidden", ...props.style } });
  }

  function Card(props = {}) {
    return Column({ ...props, style: { background: "surface", borderColor: "border", borderWidth: 1, radius: 12, shadow: 1, ...props.style } });
  }

  function Header(props = {}) {
    const leading = props.onBack
      ? Pressable({ onPress: props.onBack, style: { width: 42, height: 64, align: "center", justify: "center" }, children: Text({ text: "‹", style: { color: "white", fontSize: "title", fontWeight: "bold" } }) })
      : Box({ style: { width: 34, height: 34, radius: 8, background: props.accent === "busy" ? "warning" : props.accent === "danger" ? "danger" : props.accent === "none" ? "shellMuted" : "success" } });
    return Row({
      style: { height: 112, paddingX: 24, align: "center", justify: "between", background: "shell" },
      children: [
        Row({ style: { grow: 1, align: "center", gap: 16 }, children: [leading, Text({ text: props.title, style: { color: "white", fontSize: "title", fontWeight: "bold" } })] }),
        Column({ style: { align: "end", gap: 8 }, children: [
          Text({ text: props.metaTop ?? "", style: { color: "subtle", fontWeight: "bold" } }),
          Text({ text: props.metaBottom ?? "", style: { color: "muted" } }),
        ] }),
      ],
    });
  }

  function PageIntro(props = {}) {
    return Column({ style: { height: 166, paddingX: 24, paddingTop: 24, gap: 12 }, children: [
      Text({ text: props.eyebrow, style: { color: props.tone === "info" ? "info" : "accent", fontWeight: "bold" } }),
      Text({ text: props.title, style: { fontSize: "title", fontWeight: "bold" } }),
      Text({ text: props.description, style: { color: "muted", fontSize: "lg" } }),
    ] });
  }

  function SectionHeading(props = {}) {
    const detail = props.action ? (props.detail ? `${props.detail}  ·  VIEW ALL  ›` : "VIEW ALL  ›") : props.detail ?? "";
    return Row({ style: { height: 44, paddingX: 4, align: "center", justify: "between" }, children: [
      Text({ text: props.title, style: { fontSize: "xl", fontWeight: "bold", color: "heading" } }),
      Text({ text: detail, style: { color: "muted", fontWeight: "bold" } }),
    ] });
  }

  function ActionButton(props = {}) {
    const background = props.disabled ? "disabled" : props.tone === "danger" ? "dangerSoft" : props.tone === "neutral" ? "border" : "accent";
    const textColor = props.disabled ? "muted" : props.tone === "danger" ? "danger" : props.tone === "neutral" ? "heading" : "white";
    const content = Text({ text: props.label, style: { color: textColor, fontSize: "lg", fontWeight: "bold" } });
    const style = { width: "full", height: "full", align: "center", justify: "center", radius: 12, background };
    return props.disabled ? Box({ style, children: content }) : Pressable({ onPress: props.onPress, style, children: content });
  }

  function Badge(props = {}) {
    const [background, textColor] = tone(props.tone);
    return Box({ style: { paddingX: 12, paddingY: 8, radius: 8, background }, children: Text({ text: props.label, style: { color: textColor, fontWeight: "bold" } }) });
  }

  function EmptyState(props = {}) {
    return Card({ style: { width: "full", height: props.compact ? 150 : 430, paddingX: props.compact ? 20 : 48, align: "center", justify: "center", ...props.style }, children: [
      props.icon ? Box({ style: { width: 88, height: 88, align: "center", justify: "center", radius: 12, background: props.tone === "info" ? "infoSoft" : "border" }, children: Text({ text: props.icon, style: { color: props.tone === "info" ? "info" : "muted", fontSize: "title", fontWeight: "bold" } }) }) : null,
      Text({ text: props.title, style: { marginTop: props.icon ? 28 : 0, color: props.icon ? "heading" : "muted", fontSize: props.icon ? "title" : "lg", fontWeight: "bold" } }),
      props.detail ? Text({ text: props.detail, style: { marginTop: 16, color: "muted", fontSize: "lg" } }) : null,
    ] });
  }

  function MetricCard(props = {}) {
    return Card({ style: { width: "full", height: "full", paddingX: 20, paddingY: 16, gap: 12 }, children: [
      Text({ text: props.label, style: { color: "muted", fontWeight: "bold" } }),
      Text({ text: props.value, style: { color: props.tone === "success" ? "success" : props.tone === "danger" ? "danger" : "heading", fontSize: "xl", fontWeight: "bold" } }),
    ] });
  }

  function StatusBar(props = {}) {
    return Row({ style: { width: "full", height: "full", paddingX: props.dark ? 24 : 0, align: "center", background: props.dark ? "shell" : undefined }, children:
      Text({ text: props.text, style: { color: props.tone === "danger" ? (props.dark ? "dangerOnDark" : "danger") : (props.dark ? "subtle" : "muted") } }) });
  }

  function ScrollButton(props = {}) {
    return Pressable({ ...props, style: { width: 68, height: 132, align: "center", justify: "center", radius: 12, background: "accentSoft", ...props.style }, children:
      Text({ text: props.direction === "up" ? "UP" : "DN", style: { color: "accent", fontWeight: "bold" } }) });
  }

  function ScrollRail(props = {}) {
    if (typeof props.onUp !== "function" || typeof props.onDown !== "function") fail("ScrollRail requires onUp and onDown");
    return Column({ style: { width: 68, justify: "between", ...props.style }, children: [
      ScrollButton({ direction: "up", onPress: props.onUp }),
      ScrollButton({ direction: "down", onPress: props.onDown }),
    ] });
  }

  const keyboardLayers = Object.freeze({
    lower: Object.freeze(["qwertyuiop", "asdfghjkl", "zxcvbnm"]),
    upper: Object.freeze(["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"]),
    symbols: Object.freeze(["1234567890", "-/:;()$&@", ".,?!'\"+"]),
  });

  function Keyboard(props = {}) {
    if (!Object.hasOwn(keyboardLayers, props.layer)) fail(`unknown keyboard layer ${String(props.layer)}`);
    if (typeof props.onKey !== "function") fail("Keyboard requires onKey");
    const key = (label, value, style) => Pressable({
      onPress: () => props.onKey(value),
      style: { height: 120, align: "center", justify: "center", background: "surface", ...style },
      children: Text({ text: label, style: { color: "heading", fontSize: "xl", fontWeight: "bold" } }),
    });
    const rows = keyboardLayers[props.layer].map((row, index) => Row({
      style: { height: 140, gap: 8 },
      children: [
        ...[...row].map((character) => key(character, character, { grow: 1 })),
        index === 2 ? key("DEL", "Backspace", { width: 104, background: "border" }) : null,
      ],
    }));
    const mode = key(props.layer === "symbols" ? "ABC" : "123", "Mode", { width: 92, height: 156, background: "border" });
    const space = key("SPACE", " ", { width: 300, height: 156, background: "border" });
    const enter = key("ENTER", "Enter", { width: 112, height: 156, background: "success" });
    const trailing = props.layer === "symbols"
      ? [key(".", ".", { width: 68, height: 156, background: "border" }), key("?", "?", { width: 68, height: 156, background: "border" })]
      : [key("SHIFT", "Shift", { width: 144, height: 156, background: "border" })];
    return Column({ style: { width: "full", height: 596 }, children: [
      ...rows,
      Row({ style: { height: 176, gap: 8 }, children: [mode, space, ...trailing, enter] }),
    ] });
  }

  globalThis.View = Object.freeze({
    api: 1,
    colors,
    state,
    measureText,
    mount,
    Box,
    Row,
    Column,
    Text,
    Pressable,
    Screen,
    Card,
    Header,
    PageIntro,
    SectionHeading,
    ActionButton,
    Badge,
    EmptyState,
    MetricCard,
    StatusBar,
    ScrollButton,
    ScrollRail,
    Keyboard,
  });
})();
