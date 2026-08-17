(() => {
  const FONT_BODY = { fontSize: "lg" };
  const FONT_CHAT = { fontSize: "xl" };
  const FONT_FILE = { fontSize: "body" };
  const CHAT_TEXT_WIDTH = 536;
  const READER_TEXT_WIDTH = 544;
  const READER_PAGE_LINES = 36;
  const FILE_PAGE_LINES = 39;
  const tabs = ["chat", "files", "apps", "settings"];

  const screen = View.state("chat");
  const activeTab = View.state("chat");
  const agent = View.state("STARTING");
  const model = View.state("CODEX / UART / MAC");
  const schedulePresent = View.state(false);
  const scheduleText = View.state("NO WAKE SCHEDULED\n\nASK PI TO CREATE ONE WITH SCHEDULE.SET");
  const install = jsonState(null);
  const apps = jsonState([]);
  const uninstallMode = View.state(false);
  const uninstallingApp = View.state("");
  const uninstallError = View.state("");

  const filePath = View.state("");
  const fileOffset = View.state(0);
  const files = jsonState([]);
  const fileError = View.state("");
  const viewer = View.state(null);
  const reader = View.state(null);
  const readerOffset = View.state(0);

  const input = View.state("");
  const keyboardLayer = View.state("lower");
  const keyboardPurpose = View.state({ type: "prompt" });

  const wifiOffset = View.state(0);
  const networks = jsonState([]);
  const wifiSsid = View.state("NOT CONNECTED");
  const wifiDetail = View.state("SCAN AND SELECT A NETWORK");
  const wifiScanning = View.state(false);
  const cpuText = View.state("CPU --%");
  const psramText = View.state("PSRAM --%  ·  -- FREE");
  const deviceText = View.state("FIRMWARE 0.1.0  ·  WORKSPACE FREE --");

  const chatCount = View.state(1);
  const chatSlots = [0, 1].map(() => ({
    user: View.state("TYPE A MESSAGE"),
    assistant: View.state("BOOTING PI AGENT..."),
  }));
  let chatTurns = [{ user: "TYPE A MESSAGE", assistant: "BOOTING PI AGENT..." }];
  let chatScroll = 0;

  function jsonState(initial) {
    const value = View.state(initial);
    let signature = JSON.stringify(initial);
    return Object.freeze({
      get: value.get,
      set(next) {
        const nextSignature = JSON.stringify(next);
        if (nextSignature !== signature) {
          signature = nextSignature;
          value.set(next);
        }
      },
    });
  }

  function systemHeader(title, onBack) {
    const status = agent.get();
    return View.Header({
      title,
      onBack,
      accent: status === "FAULTED" ? "danger" : status === "THINKING" ? "busy" : "ready",
    });
  }

  function bottomBar() {
    return View.Row({
      style: { height: 108, paddingX: 10, paddingY: 16, gap: 10, background: "shell" },
      children: tabs.map((name) => View.Pressable({
        onPress: () => openTab(name),
        style: {
          grow: 1, basis: 0, height: "full", align: "center", justify: "center",
          background: activeTab.get() === name ? "accent" : "shellMuted",
        },
        children: View.Text({ text: name.toUpperCase(), style: { color: "white", fontWeight: "bold" } }),
      })),
    });
  }

  function chatCard(slot) {
    return View.Card({
      style: { width: "full", height: 318, paddingX: 24, paddingY: 20 },
      children: [
        View.Pressable({
          onPress: () => openReader("YOU", slot.user.get()),
          style: { height: 118, direction: "column" },
          children: [
            View.Row({ style: { height: 30, align: "center", gap: 12 }, children: [
              View.Box({ style: { width: 10, height: 10, radius: 5, background: "accent" } }),
              View.Text({ text: "YOU", style: { color: "accent", fontSize: "lg", fontWeight: "bold" } }),
            ] }),
            View.Text({
              text: () => PiText.wrapPreview(slot.user.get(), FONT_CHAT, CHAT_TEXT_WIDTH, 3),
              style: { height: 84, paddingTop: 12, fontSize: "xl" },
            }),
          ],
        }),
        View.Box({ style: { height: 2, marginX: 4, marginY: 14, background: "border" } }),
        View.Pressable({
          onPress: () => openReader("PI", slot.assistant.get()),
          style: { height: 118, direction: "column" },
          children: [
            View.Row({ style: { height: 30, align: "center", gap: 12 }, children: [
              View.Box({ style: { width: 10, height: 10, radius: 5, background: "success" } }),
              View.Text({ text: "PI", style: { color: "success", fontSize: "lg", fontWeight: "bold" } }),
            ] }),
            View.Text({
              text: () => PiText.wrapPreview(slot.assistant.get(), FONT_CHAT, CHAT_TEXT_WIDTH, 3),
              style: { height: 84, paddingTop: 12, fontSize: "xl" },
            }),
          ],
        }),
      ],
    });
  }

  function chatScreen() {
    return View.Screen({ children: [
      systemHeader("ESP32 PI AGENT"),
      View.Column({ style: { grow: 1, padding: 24, gap: 24 }, children: [
        View.Row({ style: { grow: 1, gap: 20 }, children: [
          View.Column({ style: { grow: 1, gap: 22 }, children: chatSlots.slice(0, chatCount.get()).map(chatCard) }),
          View.ScrollRail({ onUp: () => moveChat(2), onDown: () => moveChat(-2) }),
        ] }),
        View.Card({ style: { height: 204, paddingX: 24, paddingY: 20, gap: 24 }, children: [
          View.Text({ text: "NEXT WAKE", style: { color: "muted", fontWeight: "bold" } }),
          View.Text({
            text: scheduleText.get,
            style: { color: schedulePresent.get() ? "heading" : "muted", fontSize: "lg", fontWeight: schedulePresent.get() ? "bold" : "regular" },
          }),
        ] }),
        View.Box({ style: { height: 80 }, children: View.ActionButton({ label: "TYPE A MESSAGE", onPress: openPromptKeyboard }) }),
      ] }),
      bottomBar(),
    ] });
  }

  function fileRow(entry) {
    return View.Pressable({
      onPress: () => openFileEntry(entry),
      style: { width: "full", height: 92, paddingX: 18, direction: "row", align: "center", gap: 20, background: "surface" },
      children: [
        View.Box({
          style: { width: 48, height: 48, align: "center", justify: "center", background: entry.kind === "dir" ? "infoSoft" : "successSoft" },
          children: View.Text({ text: entry.kind === "dir" ? "D" : "F", style: { color: entry.kind === "dir" ? "info" : "success", fontSize: "lg", fontWeight: "bold" } }),
        }),
        View.Column({ style: { grow: 1, gap: 8 }, children: [
          View.Text({ text: (entry.name + (entry.kind === "dir" ? "/" : "")).slice(0, 52), style: { fontSize: "lg", fontWeight: "bold" } }),
          View.Text({ text: entry.kind === "dir" ? "FOLDER" : formatSize(entry.size), style: { color: "muted" } }),
        ] }),
      ],
    });
  }

  function filesScreen() {
    const entries = files.get();
    const offset = fileOffset.get();
    const path = filePath.get();
    return View.Screen({ children: [
      systemHeader("WORKSPACE FILES", path ? goUpDirectory : undefined),
      View.Column({ style: { grow: 1, padding: 24, gap: 12 }, children: [
        View.Box({ style: { height: 48, paddingX: 16, justify: "center", background: "border" }, children:
          View.Text({ text: "/workspace" + (path ? "/" + path : ""), style: { color: "muted" } }) }),
        View.Row({ style: { grow: 1, gap: 20 }, children: [
          entries.length
            ? View.Column({ style: { grow: 1, gap: 12 }, children: entries.slice(offset, offset + 8).map(fileRow) })
            : View.EmptyState({ compact: true, title: fileError.get }),
          entries.length > 8 ? View.ScrollRail({ onUp: () => moveFiles(-4), onDown: () => moveFiles(4) }) : null,
        ] }),
      ] }),
      bottomBar(),
    ] });
  }

  function appRow(app) {
    const busy = uninstallingApp.get();
    return View.Pressable({
      onPress: () => uninstallMode.get() ? "" : PocketPi.navigate(app.id),
      style: { width: "full", height: 150, paddingX: 24, direction: "row", align: "center", gap: 20, background: "surface" },
      children: [
        View.Row({ style: { grow: 1, align: "center", gap: 20 }, children: [
          View.Box({ style: { width: 68, height: 68, align: "center", justify: "center", background: "accentSoft" }, children:
            View.Text({ text: app.title.slice(0, 1).toUpperCase(), style: { color: "accent", fontSize: "xl", fontWeight: "bold" } }) }),
          View.Column({ style: { grow: 1, gap: 8 }, children: [
            View.Text({ text: app.title, style: { fontSize: "xl", fontWeight: "bold" } }),
            View.Text({ text: app.description, style: { color: "muted", fontSize: "lg" } }),
            app.scheduleEveryMinutes ? View.Text({ text: `UPDATES EVERY ${app.scheduleEveryMinutes} MINUTES`, style: { color: "muted" } }) : null,
          ] }),
        ] }),
        uninstallMode.get()
          ? View.Box({ style: { width: 68, height: 68 }, children: View.ActionButton({
            label: busy === app.id ? "..." : "X", tone: "danger", disabled: Boolean(busy),
            onPress: () => PocketPi.command("apps.uninstall", { app: app.id }),
          }) })
          : View.Text({ text: "›", style: { color: "accent", fontSize: "title", fontWeight: "bold" } }),
      ],
    });
  }

  function appsScreen() {
    const installed = apps.get();
    const busy = uninstallingApp.get();
    const error = uninstallError.get();
    const status = error ? `UNINSTALL FAILED  ·  ${error}` : busy
      ? `UNINSTALLING ${busy.toUpperCase()}...`
      : "APP DATA STAYS ISOLATED.  PI AGENT CAN USE EACH APP'S TOOLS.";
    return View.Screen({ children: [
      systemHeader("APPS"),
      View.Column({ style: { grow: 1, padding: 24, gap: 16 }, children: [
        View.Text({ text: `${installed.length} INSTALLED APPS`, style: { color: "muted", fontWeight: "bold" } }),
        View.Column({ style: { grow: 1, gap: 16 }, children:
          installed.length ? installed.map(appRow) : View.EmptyState({ compact: true, title: "NO OPTIONAL APPS INSTALLED" }) }),
        View.Box({ style: { height: 112, paddingX: 24, background: "border" }, children:
          View.StatusBar({ text: status, tone: error ? "danger" : "neutral" }) }),
        View.Box({ style: { height: 80 }, children:
          View.ActionButton({
            label: uninstallMode.get() ? "DONE" : "UNINSTALL APP",
            disabled: (!uninstallMode.get() && installed.length === 0) || Boolean(busy),
            tone: uninstallMode.get() ? "neutral" : "danger",
            onPress: () => uninstallMode.set(!uninstallMode.get()),
          }) }),
      ] }),
      bottomBar(),
    ] });
  }

  function networkRow(network) {
    return View.Pressable({
      onPress: () => selectNetwork(network),
      style: { width: "full", height: 84, paddingX: 20, direction: "row", align: "center", justify: "between", background: "surface" },
      children: [
        View.Text({ text: network.ssid, style: { fontSize: "lg", fontWeight: "bold" } }),
        View.Text({ text: `${network.rssiDbm} DBM  ${network.secured ? "LOCK" : "OPEN"}`, style: { color: "muted" } }),
      ],
    });
  }

  function settingsScreen() {
    const available = networks.get();
    const offset = wifiOffset.get();
    return View.Screen({ children: [
      systemHeader("SETTINGS"),
      View.Column({ style: { grow: 1, padding: 24, gap: 12 }, children: [
        View.Row({ style: { height: 64, paddingX: 20, align: "center", justify: "between", background: "shell" }, children: [
          View.Text({ text: "SYSTEM", style: { color: "white", fontWeight: "bold" } }),
          View.Text({ text: cpuText.get, style: { color: "accent", fontWeight: "bold" } }),
          View.Text({ text: psramText.get, style: { color: "subtle", fontWeight: "bold" } }),
        ] }),
        View.Row({ style: { height: 154, paddingX: 20, align: "center", gap: 16, background: "surface" }, children: [
          View.Column({ style: { grow: 1, gap: 12 }, children: [
            View.Text({ text: "WI-FI", style: { fontSize: "lg", fontWeight: "bold" } }),
            View.Text({ text: wifiSsid.get, style: { color: "accent", fontSize: "lg", fontWeight: "bold" } }),
            View.Text({ text: wifiDetail.get, style: { color: "muted" } }),
          ] }),
          View.Box({ style: { width: 196, height: 72 }, children:
            View.ActionButton({ label: wifiScanning.get() ? "SCANNING" : "SCAN", disabled: wifiScanning.get(), onPress: () => PocketPi.command("device.wifi.scan") }) }),
        ] }),
        View.Text({ text: "AVAILABLE NETWORKS", style: { color: "muted" } }),
        View.Row({ style: { grow: 1, gap: 20 }, children: [
          available.length
            ? View.Column({ style: { grow: 1, gap: 8 }, children: available.slice(offset, offset + 5).map(networkRow) })
            : View.EmptyState({ compact: true, title: "NO NETWORK LIST YET", detail: "TAP SCAN TO FIND WI-FI" }),
          available.length > 5 ? View.ScrollRail({ onUp: () => moveWifi(-4), onDown: () => moveWifi(4) }) : null,
        ] }),
        View.Column({ style: { height: 160, paddingX: 20, justify: "center", gap: 12, background: "surface" }, children: [
          View.Text({ text: "MODEL BACKEND", style: { color: "muted" } }),
          View.Text({ text: model.get, style: { fontSize: "lg", fontWeight: "bold" } }),
          View.Text({ text: deviceText.get, style: { color: "muted" } }),
        ] }),
        View.Row({ style: { height: 80, gap: 16 }, children: [
          View.Box({ style: { grow: 1, basis: 0, height: "full" }, children: View.ActionButton({ label: "FORGET WI-FI", tone: "neutral", onPress: () => PocketPi.command("device.wifi.forget") }) }),
          View.Box({ style: { grow: 1, basis: 0, height: "full" }, children: View.ActionButton({ label: "RESTART DEVICE", tone: "danger", onPress: () => PocketPi.command("device.restart") }) }),
        ] }),
      ] }),
      bottomBar(),
    ] });
  }

  function keyboardScreen() {
    const purpose = keyboardPurpose.get();
    const limit = purpose.type === "wifi" ? 63 : 256;
    return View.Screen({ children: [
      systemHeader(purpose.type === "wifi" ? "WIFI PASSWORD" : "NEW MESSAGE", closeKeyboard),
      View.Column({ style: { grow: 1, paddingX: 24, paddingTop: 20 }, children: [
        View.Box({ style: { grow: 1, paddingX: 22, paddingTop: 24, background: "surface" }, children:
          View.Text({
            text: () => input.get() ? (purpose.type === "wifi" ? "*".repeat(input.get().length) : input.get()) : purpose.type === "wifi" ? "ENTER NETWORK PASSWORD..." : "TYPE YOUR MESSAGE...",
            style: { color: "heading", fontSize: "lg" },
          }) }),
        View.Row({ style: { height: 86, paddingX: 4, align: "center", justify: "between" }, children: [
          View.Text({ text: () => `${input.get().length} / ${limit} CHARACTERS`, style: { color: "muted" } }),
          View.Pressable({ onPress: () => input.set(""), style: { width: 132, height: 58, align: "center", justify: "center", background: "border" }, children:
            View.Text({ text: "CLEAR", style: { fontWeight: "bold" } }) }),
        ] }),
      ] }),
      View.Keyboard({ layer: keyboardLayer.get(), onKey: handleKey }),
      View.Box({ style: { height: 164, paddingX: 24, paddingY: 24 }, children:
        View.ActionButton({ label: "CLOSE KEYBOARD", tone: "neutral", onPress: closeKeyboard }) }),
    ] });
  }

  function viewerScreen() {
    const current = viewer.get();
    return View.Screen({ children: [
      systemHeader("FILE VIEWER", closeViewer),
      View.Row({ style: { grow: 1, padding: 24, gap: 20 }, children: [
        View.Column({ style: { grow: 1, gap: 12 }, children: [
          View.Box({ style: { height: 82, paddingX: 16, justify: "center", background: "surface" }, children:
            View.Text({ text: current?.path ?? "NO FILE OPEN", style: { fontSize: "lg", fontWeight: "bold" } }) }),
          View.Box({ style: { grow: 1, paddingX: 20, paddingTop: 20, background: "shell" }, children:
            View.Text({ text: current?.page.text ?? "", style: { color: "border" } }) }),
          View.Text({
            text: current ? `PAGE ${current.pageIndex + 1}  ·  SOURCE LINES ${current.page.startSourceLine + 1}-${current.page.lastSourceLine + 1}` : "NO FILE OPEN",
            style: { color: "muted" },
          }),
        ] }),
        View.ScrollRail({ onUp: () => moveViewerPage(-1), onDown: () => moveViewerPage(1) }),
      ] }),
    ] });
  }

  function readerScreen() {
    const current = reader.get();
    const offset = readerOffset.get();
    return View.Screen({ children: [
      systemHeader("MESSAGE READER", closeReader),
      View.Row({ style: { grow: 1, padding: 24, gap: 20 }, children: [
        View.Column({ style: { grow: 1, gap: 12 }, children: [
          View.Box({ style: { height: 82, paddingX: 16, justify: "center", background: "surface" }, children:
            View.Text({ text: current?.author ?? "PI", style: { color: "accent", fontSize: "lg", fontWeight: "bold" } }) }),
          View.Box({ style: { grow: 1, paddingX: 20, paddingTop: 20, background: "surface" }, children:
            View.Text({ text: current ? current.lines.slice(offset, offset + READER_PAGE_LINES).join("\n") : "", style: { fontSize: "xl" } }) }),
        ] }),
        View.ScrollRail({ onUp: () => moveReader(-18), onDown: () => moveReader(18) }),
      ] }),
    ] });
  }

  function installScreen(detail) {
    const status = detail.state === "review" ? "REVIEW APP" : detail.state === "installing" ? "INSTALLING" : detail.state === "success" ? "APP INSTALLED" : "INSTALL FAILED";
    const title = detail.state === "review" ? "Ready to install" : detail.state === "installing" ? "Installing App..." : detail.state === "success" ? "Installation complete" : "Installation failed";
    const message = detail.state === "review" ? "Confirm this package on the device." : detail.state === "installing" ? "Do not operate the device until installation finishes." : detail.state === "success" ? "The App is available to you and Pi Agent." : detail.error || "The package could not be installed.";
    const tone = detail.state === "failed" ? "danger" : detail.state === "success" ? "success" : detail.state === "installing" ? "warning" : "accent";
    const surface = tone === "danger" ? "dangerSoft" : tone === "success" ? "successSoft" : tone === "warning" ? "warningSoft" : "accentSoft";
    const color = tone === "danger" ? "danger" : tone === "success" ? "success" : tone === "warning" ? "warningText" : "accent";
    return View.Screen({ children: [
      View.Header({ title: status, accent: detail.state === "failed" ? "danger" : detail.state === "success" ? "ready" : "busy", metaTop: "LOCAL INSTALL", metaBottom: "PHYSICAL CONFIRMATION" }),
      View.PageIntro({ eyebrow: "APP PACKAGE", title: detail.name, description: `VERSION ${detail.version}  ·  ${detail.tools} TOOLS  ·  ${detail.schedules} SCHEDULES` }),
      View.Column({ style: { grow: 1, paddingX: 24, gap: 16 }, children: [
        View.SectionHeading({ title: "REQUESTED ACCESS", detail: "PACKAGE MANIFEST" }),
        View.Card({ style: { grow: 1, paddingX: 24, paddingY: 24, gap: 20 }, children: [
          View.Text({ text: "NETWORK", style: { color: "muted", fontWeight: "bold" } }),
          View.Text({ text: detail.network.length ? detail.network.slice(0, 2).join("\n") : "NO NETWORK ACCESS", style: { fontSize: "lg" } }),
          View.Box({ style: { height: 2, background: "border" } }),
          View.Text({ text: "CREDENTIALS", style: { color: "muted", fontWeight: "bold" } }),
          View.Text({ text: detail.credentials.length ? detail.credentials.slice(0, 3).join(", ") : "NONE", style: { fontSize: "lg" } }),
          View.Box({ style: { height: 2, background: "border" } }),
          View.Text({ text: "CAPABILITIES", style: { color: "muted", fontWeight: "bold" } }),
          View.Text({ text: `${detail.tools} TOOLS  ·  ${detail.schedules} SCHEDULES`, style: { fontSize: "lg" } }),
        ] }),
        View.Column({ style: { height: 150, paddingX: 24, justify: "center", gap: 12, radius: 12, background: surface }, children: [
          View.Text({ text: title, style: { color, fontSize: "xl", fontWeight: "bold" } }),
          View.Text({ text: message, style: { color: "muted" } }),
        ] }),
        View.Box({ style: { height: 120, paddingY: 20 }, children: View.ActionButton({
          label: detail.state === "review" ? "INSTALL" : detail.state === "installing" ? "INSTALLING..." : "DONE",
          disabled: detail.state === "installing",
          tone: detail.state === "review" ? "primary" : "neutral",
          onPress: () => PocketPi.command(detail.state === "review" ? "apps.install" : "apps.dismissInstall"),
        }) }),
      ] }),
      View.Box({ style: { height: 66 }, children: View.StatusBar({ text: "Package received over your local network", dark: true }) }),
    ] });
  }

  function root() {
    const installDetail = install.get();
    if (installDetail) return installScreen(installDetail);
    if (screen.get() === "chat") return chatScreen();
    if (screen.get() === "files") return filesScreen();
    if (screen.get() === "apps") return appsScreen();
    if (screen.get() === "settings") return settingsScreen();
    if (screen.get() === "keyboard") return keyboardScreen();
    if (screen.get() === "viewer") return viewerScreen();
    return readerScreen();
  }

  function openTab(tab) {
    if (tab !== "apps") uninstallMode.set(false);
    activeTab.set(tab);
    screen.set(tab);
    if (tab === "files") refreshFiles(filePath.get());
    return "";
  }

  function updateChat(messages) {
    const turns = [];
    for (const message of messages ?? []) {
      if (message.role === "user") turns.push({ user: message.text, assistant: "THINKING..." });
      else if (!turns.length) turns.push({ user: "TYPE A MESSAGE", assistant: message.text || "THINKING..." });
      else turns[turns.length - 1].assistant = message.text || "THINKING...";
    }
    chatTurns = turns.length ? turns : [{ user: "TYPE A MESSAGE", assistant: "BOOTING PI AGENT..." }];
    chatScroll = Math.min(chatScroll, Math.max(0, chatTurns.length - 2));
    syncChat();
  }

  function syncChat() {
    const end = chatTurns.length - chatScroll;
    const visible = chatTurns.slice(Math.max(0, end - 2), end);
    chatCount.set(visible.length);
    for (let index = 0; index < visible.length; index += 1) {
      chatSlots[index].user.set(visible[index].user);
      chatSlots[index].assistant.set(visible[index].assistant);
    }
  }

  function moveChat(delta) {
    chatScroll = Math.max(0, Math.min(Math.max(0, chatTurns.length - 2), chatScroll + delta));
    syncChat();
    return "";
  }

  function openReader(author, text) {
    reader.set({ author, lines: PiText.wrapLines(text, FONT_CHAT, READER_TEXT_WIDTH) });
    readerOffset.set(0);
    screen.set("reader");
    return "";
  }

  function closeReader() {
    reader.set(null);
    readerOffset.set(0);
    screen.set("chat");
    return "";
  }

  function moveReader(delta) {
    const lineCount = reader.get()?.lines.length ?? 0;
    readerOffset.set(Math.max(0, Math.min(Math.max(0, lineCount - READER_PAGE_LINES), readerOffset.get() + delta)));
    return "";
  }

  function formatSize(size) {
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
    return `${(size / (1024 * 1024)).toFixed(1)} MB`;
  }

  function joinPath(parent, name) {
    return parent ? `${parent}/${name}` : name;
  }

  function readDirectory(path) {
    const result = [];
    let offset = 0;
    for (;;) {
      const page = JSON.parse(fs.list(path, offset));
      if (page.error !== undefined) throw new Error(page.error);
      result.push(...page.entries);
      offset += page.entries.length;
      if (page.eof) return result;
      if (!page.entries.length) throw new Error("directory listing did not advance");
    }
  }

  function decodeBase64(value) {
    const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    value = value.replace(/=+$/, "");
    const bytes = new Uint8Array(Math.floor(value.length * 3 / 4));
    let bits = 0;
    let buffer = 0;
    let offset = 0;
    for (const character of value) {
      buffer = (buffer << 6) | alphabet.indexOf(character);
      bits += 6;
      if (bits >= 8) {
        bits -= 8;
        bytes[offset++] = (buffer >> bits) & 255;
      }
    }
    return bytes;
  }

  function decodeUtf8(bytes) {
    let text = "";
    for (let index = 0; index < bytes.length;) {
      const first = bytes[index++];
      if (first < 128) {
        text += String.fromCharCode(first);
        continue;
      }
      const count = first < 224 ? 1 : first < 240 ? 2 : 3;
      let code = first & (count === 1 ? 31 : count === 2 ? 15 : 7);
      for (let offset = 0; offset < count; offset += 1) code = (code << 6) | (bytes[index++] & 63);
      if (code < 65536) text += String.fromCharCode(code);
      else {
        code -= 65536;
        text += String.fromCharCode(55296 + (code >> 10), 56320 + (code & 1023));
      }
    }
    return text;
  }

  function readTextFile(path) {
    const chunks = [];
    let offset = 0;
    for (;;) {
      const page = JSON.parse(fs.read(path, offset, 65536));
      if (page.error !== undefined) throw new Error(page.error);
      const chunk = decodeBase64(page.data.$b);
      chunks.push(chunk);
      offset += chunk.length;
      if (page.eof) {
        if (chunks.length === 1) return decodeUtf8(chunk);
        const bytes = new Uint8Array(offset);
        let writeOffset = 0;
        for (const value of chunks) {
          bytes.set(value, writeOffset);
          writeOffset += value.length;
        }
        return decodeUtf8(bytes);
      }
      if (!chunk.length) throw new Error("file read did not advance");
    }
  }

  function refreshFiles(path = filePath.get()) {
    try {
      const next = readDirectory(path);
      files.set(next);
      fileError.set(next.length ? "" : "THIS DIRECTORY IS EMPTY");
      fileOffset.set(Math.min(fileOffset.get(), Math.max(0, next.length - 8)));
    } catch (error) {
      files.set([]);
      fileError.set(error instanceof Error ? error.message : String(error));
    }
  }

  function moveFiles(delta) {
    fileOffset.set(Math.max(0, Math.min(Math.max(0, files.get().length - 8), fileOffset.get() + delta)));
    return "";
  }

  function goUpDirectory() {
    const parts = filePath.get().split("/");
    parts.pop();
    const path = parts.join("/");
    filePath.set(path);
    fileOffset.set(0);
    refreshFiles(path);
    return "";
  }

  function openFileEntry(entry) {
    const path = joinPath(filePath.get(), entry.name);
    if (entry.kind === "dir") {
      filePath.set(path);
      fileOffset.set(0);
      refreshFiles(path);
      return "";
    }
    try {
      const text = readTextFile(path);
      const start = { offset: 0, sourceLine: 0 };
      viewer.set({ path: `/workspace/${path}`, text, pageIndex: 0, pageStarts: [start], page: filePage(text, start) });
      screen.set("viewer");
    } catch (error) {
      fileError.set(error instanceof Error ? error.message : String(error));
    }
    return "";
  }

  function filePage(text, start) {
    return PiText.wrapTextPage(text, FONT_FILE, READER_TEXT_WIDTH, start.offset, start.sourceLine, FILE_PAGE_LINES);
  }

  function moveViewerPage(direction) {
    const current = viewer.get();
    if (!current) return "";
    const nextIndex = current.pageIndex + direction;
    if (nextIndex < 0 || (direction > 0 && !current.page.hasMore)) return "";
    const starts = current.pageStarts.slice();
    if (!starts[nextIndex]) starts[nextIndex] = { offset: current.page.nextOffset, sourceLine: current.page.nextSourceLine };
    viewer.set({ ...current, pageIndex: nextIndex, pageStarts: starts, page: filePage(current.text, starts[nextIndex]) });
    return "";
  }

  function closeViewer() {
    viewer.set(null);
    screen.set("files");
    return "";
  }

  function openPromptKeyboard() {
    keyboardPurpose.set({ type: "prompt" });
    input.set("");
    keyboardLayer.set("lower");
    screen.set("keyboard");
    return "";
  }

  function closeKeyboard() {
    input.set("");
    keyboardLayer.set("lower");
    screen.set(activeTab.get());
    return "";
  }

  function handleKey(key) {
    const purpose = keyboardPurpose.get();
    const limit = purpose.type === "wifi" ? 63 : 256;
    if (key === "Mode") {
      keyboardLayer.set(keyboardLayer.get() === "symbols" ? "lower" : "symbols");
      return "";
    }
    if (key === "Shift") {
      keyboardLayer.set(keyboardLayer.get() === "upper" ? "lower" : "upper");
      return "";
    }
    if (key === "Backspace") {
      input.set(input.get().slice(0, -1));
      return "";
    }
    if (key !== "Enter") {
      if (input.get().length < limit) input.set(input.get() + key);
      return "";
    }
    const value = input.get().trim();
    if (!value) return "";
    closeKeyboard();
    return purpose.type === "wifi"
      ? PocketPi.command("device.wifi.connect", { ssid: purpose.ssid, password: value })
      : PocketPi.command("agent.submit", { prompt: value });
  }

  function moveWifi(delta) {
    wifiOffset.set(Math.max(0, Math.min(Math.max(0, networks.get().length - 5), wifiOffset.get() + delta)));
    return "";
  }

  function selectNetwork(network) {
    if (!network.secured) return PocketPi.command("device.wifi.connect", { ssid: network.ssid, password: "" });
    keyboardPurpose.set({ type: "wifi", ssid: network.ssid });
    input.set("");
    keyboardLayer.set("lower");
    screen.set("keyboard");
    return "";
  }

  function updateFacts(next) {
    agent.set(next.agent ?? "STARTING");
    model.set(next.model ?? "UNKNOWN");
    updateChat(next.messages);
    const schedule = next.schedule ?? {};
    schedulePresent.set(Boolean(schedule.name));
    scheduleText.set(schedule.name
      ? PiText.wrapPreview(`${schedule.name}  ${schedule.next ?? ""}\n\n${schedule.prompt ?? ""}`, FONT_BODY, CHAT_TEXT_WIDTH, 5)
      : "NO WAKE SCHEDULED\n\nASK PI TO CREATE ONE WITH SCHEDULE.SET");
    apps.set(next.apps ?? []);
    install.set(next.install ?? null);
    uninstallingApp.set(next.uninstallingApp ?? "");
    uninstallError.set(next.uninstallError ?? "");

    const settings = next.settings ?? {};
    const wifi = settings.wifi ?? {};
    networks.set(wifi.networks ?? []);
    wifiOffset.set(Math.min(wifiOffset.get(), Math.max(0, (wifi.networks ?? []).length - 5)));
    wifiSsid.set(wifi.connectedSsid || "NOT CONNECTED");
    wifiDetail.set(wifi.ipAddress
      ? `IP ${wifi.ipAddress}  RSSI ${wifi.rssiDbm ?? "--"} DBM`
      : wifi.status || "SCAN AND SELECT A NETWORK");
    wifiScanning.set(Boolean(wifi.scanning));
    const telemetry = settings.telemetry ?? {};
    cpuText.set(`CPU ${telemetry.cpuPercent ?? "--"}%`);
    psramText.set(`PSRAM ${telemetry.psramUsedPercent ?? "--"}%  ·  ${telemetry.psramFree ?? "--"} FREE`);
    deviceText.set(`FIRMWARE ${settings.firmwareVersion ?? "0.1.0"}  ·  WORKSPACE FREE ${settings.workspaceFree ?? "--"}`);
    return "";
  }

  PocketPi.defineSystem({
    update: updateFacts,
    telemetryVisible() {
      return install.get() === null && screen.get() === "settings";
    },
  });
  View.mount(root);
  refreshFiles("");
})();
