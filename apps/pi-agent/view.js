(() => {
  const FONT_BODY = { fontSize: "md" };
  const FONT_CHAT = { fontSize: "lg" };
  const FONT_FILE = { fontSize: "sm" };
  const LANDSCAPE = View.viewport.orientation === "landscape";
  const CHAT_LINE_HEIGHT = 24;
  const CHAT_PREVIEW_HEIGHT = 84;
  const CHAT_PREVIEW_LINES = LANDSCAPE ? 3 : Math.max(1, Math.floor(CHAT_PREVIEW_HEIGHT * View.viewport.scale / CHAT_LINE_HEIGHT));
  const CHAT_VISIBLE_TURNS = LANDSCAPE ? 1 : 2;
  const CHAT_TEXT_WIDTH = View.viewport.width * (LANDSCAPE ? 0.42 : 0.74);
  const KEYBOARD_OUTER_PADDING = LANDSCAPE ? 12 : 24;
  const KEYBOARD_INNER_PADDING = LANDSCAPE ? 14 : 22;
  const KEYBOARD_TEXT_WIDTH = View.viewport.width - 2 * (KEYBOARD_OUTER_PADDING + KEYBOARD_INNER_PADDING) * View.viewport.scale;
  const READER_TEXT_WIDTH = View.viewport.width * (LANDSCAPE ? 0.82 : 0.76);
  const READER_LINE_HEIGHT = 24;
  const READER_CHROME_HEIGHT = 274;
  const READER_PAGE_LINES = LANDSCAPE
    ? 12
    : Math.min(36, Math.max(1, Math.floor((View.viewport.height - READER_CHROME_HEIGHT * View.viewport.scale) / READER_LINE_HEIGHT)));
  const FILE_PAGE_LINES = LANDSCAPE ? 12 : 39;
  const APP_VISIBLE_ROWS = LANDSCAPE ? 1 : 4;
  const FILE_VISIBLE_ROWS = LANDSCAPE ? 2 : 5;
  const WIFI_VISIBLE_ROWS = LANDSCAPE ? 2 : 5;
  const SYSTEM_ROOTS = new Set([".pi-agent", ".system", "apps", "data", "system"]);
  const tabs = ["chat", "files", "apps", "settings"];

  const screen = View.state("chat");
  const activeTab = View.state("chat");
  const agent = View.state("STARTING");
  const model = View.state("CODEX / UART / MAC");
  const schedulePresent = View.state(false);
  const scheduleText = View.state("NO WAKE SCHEDULED\n\nASK PI TO CREATE ONE WITH SCHEDULE.SET");
  const install = jsonState(null);
  const apps = jsonState([]);
  const appOffset = View.state(0);
  const uninstallMode = View.state(false);
  const uninstallingApp = View.state("");
  const uninstallError = View.state("");

  const filePath = View.state("");
  const fileOffset = View.state(0);
  const files = jsonState([]);
  const fileError = View.state("");
  const fileDeleteMode = View.state(false);
  const fileDeleteError = View.state("");
  const deletingFile = View.state("");
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
  const chatSlots = Array.from({ length: CHAT_VISIBLE_TURNS }, () => ({
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

  function deleteButton(target, busy, onPress) {
    return View.ActionButton({
      label: busy === target ? "..." : "X",
      tone: "danger",
      disabled: Boolean(busy),
      onPress,
      style: { width: 68, height: 68 },
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
    return View.NavigationBar({
      items: tabs.map((name) => ({
        label: name.toUpperCase(),
        onPress: () => openTab(name),
        active: activeTab.get() === name,
      })),
    });
  }

  function chatCard(slot) {
    return View.Card({
      style: { width: "full", height: LANDSCAPE ? "full" : 318, paddingX: LANDSCAPE ? 16 : 24, paddingY: LANDSCAPE ? 10 : 20 },
      children: [
        View.Pressable({
          onPress: () => openReader("YOU", slot.user.get()),
          style: { grow: LANDSCAPE ? 1 : 0, height: LANDSCAPE ? undefined : 118, direction: "column", overflow: "hidden" },
          children: [
            View.Row({ style: { height: 30, align: "center", gap: 12 }, children: [
              View.Box({ style: { width: 10, height: 10, radius: 5, background: "accent" } }),
              View.Text({ text: "YOU", style: { color: "accent", fontSize: "md", fontWeight: "bold" } }),
            ] }),
            View.Text({
              text: () => PiText.wrapPreview(slot.user.get(), FONT_CHAT, CHAT_TEXT_WIDTH, CHAT_PREVIEW_LINES),
              style: { grow: LANDSCAPE ? 1 : 0, height: LANDSCAPE ? undefined : CHAT_PREVIEW_HEIGHT, fontSize: "lg", lineHeight: CHAT_LINE_HEIGHT },
            }),
          ],
        }),
        View.Box({ style: { height: 2, marginX: 4, marginY: 14, background: "border" } }),
        View.Pressable({
          onPress: () => openReader("PI", slot.assistant.get()),
          style: { grow: LANDSCAPE ? 1 : 0, height: LANDSCAPE ? undefined : 118, direction: "column", overflow: "hidden" },
          children: [
            View.Row({ style: { height: 30, align: "center", gap: 12 }, children: [
              View.Box({ style: { width: 10, height: 10, radius: 5, background: "success" } }),
              View.Text({ text: "PI", style: { color: "success", fontSize: "md", fontWeight: "bold" } }),
            ] }),
            View.Text({
              text: () => PiText.wrapPreview(slot.assistant.get(), FONT_CHAT, CHAT_TEXT_WIDTH, CHAT_PREVIEW_LINES),
              style: { grow: LANDSCAPE ? 1 : 0, height: LANDSCAPE ? undefined : CHAT_PREVIEW_HEIGHT, fontSize: "lg", lineHeight: CHAT_LINE_HEIGHT },
            }),
          ],
        }),
      ],
    });
  }

  function chatScreen() {
    const conversation = View.Row({ style: { grow: 1, gap: LANDSCAPE ? 12 : 20 }, children: [
      View.Column({ style: { grow: 1, gap: LANDSCAPE ? 12 : 22 }, children: chatSlots.slice(0, chatCount.get()).map(chatCard) }),
      chatTurns.length > CHAT_VISIBLE_TURNS ? View.ScrollRail({ onUp: () => moveChat(CHAT_VISIBLE_TURNS), onDown: () => moveChat(-CHAT_VISIBLE_TURNS) }) : null,
    ] });
    const wake = View.Card({ style: { grow: LANDSCAPE ? 1 : 0, height: LANDSCAPE ? undefined : 204, paddingX: LANDSCAPE ? 16 : 24, paddingY: LANDSCAPE ? 12 : 20, gap: LANDSCAPE ? 12 : 24 }, children: [
      View.Text({ text: "NEXT WAKE", style: { color: "muted", fontWeight: "bold" } }),
      View.Text({
        text: scheduleText.get,
        style: { color: schedulePresent.get() ? "heading" : "muted", fontSize: "md", fontWeight: schedulePresent.get() ? "bold" : "regular" },
      }),
    ] });
    const action = View.ActionButton({
      label: "TYPE A MESSAGE",
      onPress: openPromptKeyboard,
      style: { width: "full", height: LANDSCAPE ? 56 : 80 },
    });
    const content = LANDSCAPE
      ? View.Row({ style: { grow: 1, padding: 12, gap: 12 }, children: [
        View.Column({ style: { grow: 2, basis: 0 }, children: conversation }),
        View.Column({ style: { grow: 1, basis: 0, gap: 12 }, children: [wake, action] }),
      ] })
      : View.Column({ style: { grow: 1, padding: 24, gap: 24 }, children: [conversation, wake, action] });
    return View.Screen({ children: [
      systemHeader("ESP32 PI AGENT"),
      content,
      bottomBar(),
    ] });
  }

  function fileRow(entry) {
    const path = joinPath(filePath.get(), entry.name);
    const deleting = fileDeleteMode.get() && canDeleteFile(entry);
    const busy = deletingFile.get();
    return View.Pressable({
      onPress: () => busy || deleting ? "" : openFileEntry(entry),
      style: { width: "full", height: 120, paddingX: 18, direction: "row", align: "center", gap: 20, background: "surface" },
      children: [
        View.Box({
          style: { width: 64, height: 64, align: "center", justify: "center", background: entry.kind === "dir" ? "infoSoft" : "successSoft" },
          children: View.Text({ text: entry.kind === "dir" ? "D" : "F", style: { color: entry.kind === "dir" ? "info" : "success", fontSize: "md", fontWeight: "bold" } }),
        }),
        View.Column({ style: { grow: 1, gap: 8, overflow: "hidden" }, children: [
          View.Text({ text: (entry.name + (entry.kind === "dir" ? "/" : "")).slice(0, 52), style: { fontSize: "md", fontWeight: "bold" } }),
          View.Text({ text: entry.kind === "dir" ? "FOLDER" : formatSize(entry.size), style: { color: "muted" } }),
        ] }),
        deleting ? deleteButton(path, busy, () => requestDeleteFile(entry)) : null,
      ],
    });
  }

  function filesScreen() {
    const entries = files.get();
    const offset = fileOffset.get();
    const path = filePath.get();
    const busy = deletingFile.get();
    const error = fileDeleteError.get();
    const status = error ? `DELETE FAILED  ·  ${error}` : busy ? "DELETING FILE..." : "SYSTEM FILES STAY PROTECTED";
    const fileList = View.Column({ style: { grow: 1 }, children: entries.length
      ? View.Row({ style: { grow: 1, gap: LANDSCAPE ? 12 : 20 }, children: [
        View.Column({ style: { grow: 1, gap: LANDSCAPE ? 6 : 12 }, children: entries.slice(offset, offset + FILE_VISIBLE_ROWS).map(fileRow) }),
        entries.length > FILE_VISIBLE_ROWS ? View.ScrollRail({ onUp: () => moveFiles(-FILE_VISIBLE_ROWS), onDown: () => moveFiles(FILE_VISIBLE_ROWS) }) : null,
      ] })
      : View.EmptyState({ compact: true, style: { height: "full" }, title: fileError.get }) });
    const pathBox = View.Box({ style: { height: 48, paddingX: 16, justify: "center", background: "border", overflow: "hidden" }, children:
      View.Text({ text: "/workspace" + (path ? "/" + path : ""), style: { color: "muted" } }) });
    const statusBox = View.Box({ style: { grow: LANDSCAPE ? 1 : 0, height: LANDSCAPE ? undefined : 112, paddingX: LANDSCAPE ? 16 : 24, background: "border" }, children:
      View.StatusBar({ text: status, tone: error ? "danger" : "neutral" }) });
    const actionBox = View.ActionButton({
      label: fileDeleteMode.get() ? "DONE" : "DELETE FILES",
      disabled: Boolean(busy) || (!fileDeleteMode.get() && !entries.some(canDeleteFile)),
      tone: fileDeleteMode.get() ? "neutral" : "danger",
      onPress: () => fileDeleteMode.set(!fileDeleteMode.get()),
      style: { width: "full", height: LANDSCAPE ? 56 : 80 },
    });
    const content = LANDSCAPE
      ? View.Row({ style: { grow: 1, padding: 12, gap: 12 }, children: [
        View.Column({ style: { grow: 2, basis: 0, gap: 10 }, children: [pathBox, fileList] }),
        View.Column({ style: { grow: 1, basis: 0, gap: 12 }, children: [statusBox, actionBox] }),
      ] })
      : View.Column({ style: { grow: 1, padding: 24, gap: 16 }, children: [pathBox, fileList, statusBox, actionBox] });
    return View.Screen({ children: [systemHeader("WORKSPACE FILES", path ? goUpDirectory : undefined), content, bottomBar()] });
  }

  function appRow(app) {
    const busy = uninstallingApp.get();
    return View.Pressable({
      onPress: () => uninstallMode.get() ? "" : PocketPi.navigate(app.id),
      style: { width: "full", height: 150, paddingX: 24, direction: "row", align: "center", gap: 20, background: "surface" },
      children: [
        View.Box({ style: { width: 68, height: 68, align: "center", justify: "center", background: "accentSoft" }, children:
          View.Text({ text: app.title.slice(0, 1).toUpperCase(), style: { color: "accent", fontSize: "lg", fontWeight: "bold" } }) }),
        View.Column({ style: { grow: 1, basis: 0, gap: 8, overflow: "hidden" }, children: [
          View.Text({ text: app.title, style: { fontSize: "lg", fontWeight: "bold" } }),
          View.Text({ text: app.description, style: { color: "muted", fontSize: "md" } }),
          app.scheduleEveryMinutes ? View.Text({ text: `UPDATES EVERY ${app.scheduleEveryMinutes} MINUTES`, style: { color: "muted" } }) : null,
        ] }),
        uninstallMode.get()
          ? deleteButton(app.id, busy, () => PocketPi.command("apps.uninstall", { app: app.id }))
          : View.Text({ text: "›", style: { color: "accent", fontSize: "xl", fontWeight: "bold" } }),
      ],
    });
  }

  function appsScreen() {
    const installed = apps.get();
    const offset = appOffset.get();
    const busy = uninstallingApp.get();
    const error = uninstallError.get();
    const status = error ? `UNINSTALL FAILED  ·  ${error}` : busy
      ? `UNINSTALLING ${busy.toUpperCase()}...`
      : "APP DATA STAYS ISOLATED";
    const appList = View.Column({ style: { grow: 1 }, children: installed.length
      ? View.Row({ style: { grow: 1, gap: LANDSCAPE ? 12 : 20 }, children: [
        View.Column({ style: { grow: 1, gap: 16 }, children: installed.slice(offset, offset + APP_VISIBLE_ROWS).map(appRow) }),
        installed.length > APP_VISIBLE_ROWS ? View.ScrollRail({ onUp: () => moveApps(-APP_VISIBLE_ROWS), onDown: () => moveApps(APP_VISIBLE_ROWS) }) : null,
      ] })
      : View.EmptyState({ compact: true, style: { height: "full" }, title: "NO OPTIONAL APPS INSTALLED" }) });
    const statusBox = View.Box({ style: { grow: LANDSCAPE ? 1 : 0, height: LANDSCAPE ? undefined : 112, paddingX: LANDSCAPE ? 16 : 24, background: "border" }, children:
      View.StatusBar({ text: status, tone: error ? "danger" : "neutral" }) });
    const actionBox = View.ActionButton({
      label: uninstallMode.get() ? "DONE" : "UNINSTALL APP",
      disabled: (!uninstallMode.get() && installed.length === 0) || Boolean(busy),
      tone: uninstallMode.get() ? "neutral" : "danger",
      onPress: () => uninstallMode.set(!uninstallMode.get()),
      style: { width: "full", height: LANDSCAPE ? 56 : 80 },
    });
    const content = LANDSCAPE
      ? View.Row({ style: { grow: 1, padding: 12, gap: 12 }, children: [
        View.Column({ style: { grow: 2, basis: 0, gap: 10 }, children: [
          View.Text({ text: `${installed.length} INSTALLED APPS`, style: { color: "muted", fontWeight: "bold" } }),
          appList,
        ] }),
        View.Column({ style: { grow: 1, basis: 0, gap: 12 }, children: [statusBox, actionBox] }),
      ] })
      : View.Column({ style: { grow: 1, padding: 24, gap: 16 }, children: [
        View.Text({ text: `${installed.length} INSTALLED APPS`, style: { color: "muted", fontWeight: "bold" } }),
        appList,
        statusBox,
        actionBox,
      ] });
    return View.Screen({ children: [systemHeader("APPS"), content, bottomBar()] });
  }

  function networkRow(network) {
    return View.Pressable({
      onPress: () => selectNetwork(network),
      style: { width: "full", height: LANDSCAPE ? 72 : 84, paddingX: 20, direction: "row", align: "center", gap: 12, background: "surface" },
      children: [
        View.Column({ style: { grow: 1, basis: 0, overflow: "hidden" }, children:
          View.Text({ text: network.ssid, style: { fontSize: "md", fontWeight: "bold" } }) }),
        View.Text({ text: `${network.rssiDbm} DBM  ${network.secured ? "LOCK" : "OPEN"}`, style: { color: "muted" } }),
      ],
    });
  }

  function settingsScreen() {
    const available = networks.get();
    const offset = wifiOffset.get();
    const systemSummary = View.Row({ style: { height: LANDSCAPE ? 52 : 64, paddingX: 20, align: "center", justify: "between", background: "shell" }, children: [
      View.Text({ text: "SYSTEM", style: { color: "white", fontWeight: "bold" } }),
      View.Text({ text: cpuText.get, style: { color: "accent", fontWeight: "bold" } }),
      View.Text({ text: psramText.get, style: { color: "subtle", fontWeight: "bold" } }),
    ] });
    const wifiSummary = View.Row({ style: { height: LANDSCAPE ? 132 : 154, paddingX: 20, align: "center", gap: 16, background: "surface" }, children: [
      View.Column({ style: { grow: 1, basis: 0, gap: LANDSCAPE ? 8 : 12, overflow: "hidden" }, children: [
        View.Text({ text: "WI-FI", style: { fontSize: "md", fontWeight: "bold" } }),
        View.Text({ text: wifiSsid.get, style: { color: "accent", fontSize: "md", fontWeight: "bold" } }),
        View.Text({ text: wifiDetail.get, style: { color: "muted" } }),
      ] }),
      View.ActionButton({
        label: wifiScanning.get() ? "SCANNING" : "SCAN",
        disabled: wifiScanning.get(),
        onPress: () => PocketPi.command("device.wifi.scan"),
        style: { width: LANDSCAPE ? 120 : 196, height: LANDSCAPE ? 52 : 72 },
      }),
    ] });
    const networkList = View.Column({ style: { grow: 1 }, children: available.length
      ? View.Row({ style: { grow: 1, gap: LANDSCAPE ? 12 : 20 }, children: [
        View.Column({ style: { grow: 1, gap: 8 }, children: available.slice(offset, offset + WIFI_VISIBLE_ROWS).map(networkRow) }),
        available.length > WIFI_VISIBLE_ROWS ? View.ScrollRail({ onUp: () => moveWifi(-WIFI_VISIBLE_ROWS), onDown: () => moveWifi(WIFI_VISIBLE_ROWS) }) : null,
      ] })
      : View.EmptyState({ compact: true, style: { height: "full" }, title: "NO NETWORK LIST YET", detail: "TAP SCAN TO FIND WI-FI" }) });
    const backendSummary = View.Column({ style: { height: LANDSCAPE ? 80 : 160, paddingX: 20, justify: "center", gap: LANDSCAPE ? 6 : 12, background: "surface" }, children: [
      View.Text({ text: "MODEL BACKEND", style: { color: "muted" } }),
      View.Text({ text: model.get, style: { fontSize: "md", fontWeight: "bold" } }),
      View.Text({
        text: () => PiText.wrapPreview(deviceText.get(), FONT_BODY, View.viewport.width * (LANDSCAPE ? 0.4 : 0.84), 1),
        style: { color: "muted" },
      }),
    ] });
    const deviceActions = View.Row({ style: { height: LANDSCAPE ? 56 : 80, gap: 16 }, children: [
      View.ActionButton({ label: "FORGET WI-FI", tone: "neutral", onPress: () => PocketPi.command("device.wifi.forget"), style: { grow: 1, basis: 0, height: "full" } }),
      View.ActionButton({ label: "RESTART DEVICE", tone: "danger", onPress: () => PocketPi.command("device.restart"), style: { grow: 1, basis: 0, height: "full" } }),
    ] });
    const content = LANDSCAPE
      ? View.Row({ style: { grow: 1, padding: 12, gap: 12 }, children: [
        View.Column({ style: { grow: 1, basis: 0, gap: 10 }, children: [systemSummary, wifiSummary, deviceActions] }),
        View.Column({ style: { grow: 1, basis: 0, gap: 8 }, children: [
          View.Text({ text: "AVAILABLE NETWORKS", style: { color: "muted" } }),
          networkList,
          backendSummary,
        ] }),
      ] })
      : View.Column({ style: { grow: 1, padding: 24, gap: 12 }, children: [
        systemSummary,
        wifiSummary,
        View.Text({ text: "AVAILABLE NETWORKS", style: { color: "muted" } }),
        networkList,
        backendSummary,
        deviceActions,
      ] });
    return View.Screen({ children: [systemHeader("SETTINGS"), content, bottomBar()] });
  }

  function keyboardScreen() {
    const purpose = keyboardPurpose.get();
    const limit = purpose.type === "wifi" ? 63 : 256;
    return View.Screen({ children: [
      systemHeader(purpose.type === "wifi" ? "WIFI PASSWORD" : "NEW MESSAGE", closeKeyboard),
      View.Column({ style: { grow: 1, paddingX: KEYBOARD_OUTER_PADDING, paddingTop: LANDSCAPE ? 8 : 20 }, children: [
        View.Box({ style: { grow: 1, paddingX: KEYBOARD_INNER_PADDING, paddingTop: LANDSCAPE ? 10 : 24, background: "surface" }, children:
          View.Text({
            text: () => PiText.wrapLines(
              input.get() ? (purpose.type === "wifi" ? "*".repeat(input.get().length) : input.get()) : purpose.type === "wifi" ? "ENTER NETWORK PASSWORD..." : "TYPE YOUR MESSAGE...",
              FONT_BODY,
              KEYBOARD_TEXT_WIDTH,
            ).join("\n"),
            style: { color: "heading", fontSize: "md" },
          }) }),
        View.Row({ style: { height: LANDSCAPE ? 52 : 86, paddingX: 4, align: "center", justify: "between" }, children: [
          View.Text({ text: () => `${input.get().length} / ${limit} CHARACTERS`, style: { color: "muted" } }),
          View.Pressable({ onPress: () => input.set(""), style: { width: LANDSCAPE ? 100 : 132, height: LANDSCAPE ? 42 : 58, align: "center", justify: "center", background: "border" }, children:
            View.Text({ text: "CLEAR", style: { fontWeight: "bold" } }) }),
        ] }),
      ] }),
      View.Keyboard({ layer: keyboardLayer.get(), onKey: handleKey }),
      View.ActionButton({
        label: "CLOSE KEYBOARD",
        tone: "neutral",
        onPress: closeKeyboard,
        style: {
          width: "full",
          height: LANDSCAPE ? 44 : 116,
          marginX: LANDSCAPE ? 12 : 24,
          marginY: LANDSCAPE ? 4 : 24,
        },
      }),
    ] });
  }

  function viewerScreen() {
    const current = viewer.get();
    const scrollable = Boolean(current && (current.pageIndex > 0 || current.page.hasMore));
    return View.Screen({ children: [
      systemHeader("FILE VIEWER", closeViewer),
      View.Column({ style: { grow: 1, padding: 24, gap: 12 }, children: [
        View.Box({ style: { height: 82, paddingX: 16, justify: "center", background: "surface" }, children:
          View.Text({ text: current?.path ?? "NO FILE OPEN", style: { fontSize: "md", fontWeight: "bold" } }) }),
        View.Row({ style: { grow: 1, gap: 20 }, children: [
          View.Box({ style: { grow: 1, paddingX: 20, paddingTop: 20, background: "shell" }, children:
            View.Text({ text: current?.page.text ?? "", style: { color: "border" } }) }),
          scrollable ? View.ScrollRail({ onUp: () => moveViewerPage(-1), onDown: () => moveViewerPage(1) }) : null,
        ] }),
        View.Text({
          text: current ? `PAGE ${current.pageIndex + 1}  ·  SOURCE LINES ${current.page.startSourceLine + 1}-${current.page.lastSourceLine + 1}` : "NO FILE OPEN",
          style: { color: "muted" },
        }),
      ] }),
    ] });
  }

  function readerScreen() {
    const current = reader.get();
    const offset = readerOffset.get();
    const scrollable = (current?.lines.length ?? 0) > READER_PAGE_LINES;
    return View.Screen({ children: [
      systemHeader("MESSAGE READER", closeReader),
      View.Column({ style: { grow: 1, padding: 24, gap: 12 }, children: [
        View.Box({ style: { height: 82, paddingX: 16, justify: "center", background: "surface" }, children:
          View.Text({ text: current?.author ?? "PI", style: { color: "accent", fontSize: "md", fontWeight: "bold" } }) }),
        View.Row({ style: { grow: 1, gap: 20, overflow: "hidden" }, children: [
          View.Box({ style: { grow: 1, paddingX: 20, paddingTop: 20, background: "surface" }, children:
            View.Text({ text: current ? current.lines.slice(offset, offset + READER_PAGE_LINES).join("\n") : "", style: { fontSize: "lg" } }) }),
          scrollable ? View.ScrollRail({ onUp: () => moveReader(-18), onDown: () => moveReader(18), style: { height: "full" } }) : null,
        ] }),
      ] }),
    ] });
  }

  function installScreen(detail) {
    const verb = detail.update ? "UPDATE" : "INSTALL";
    const progress = detail.update ? "UPDATING" : "INSTALLING";
    const action = detail.update ? "Update" : "Installation";
    const status = detail.state === "review" ? `REVIEW ${verb}` : detail.state === "installing" ? progress : detail.state === "success" ? `APP ${detail.update ? "UPDATED" : "INSTALLED"}` : `${verb} FAILED`;
    const title = detail.state === "review" ? `Ready to ${verb.toLowerCase()}` : detail.state === "installing" ? `${progress[0]}${progress.slice(1).toLowerCase()} App...` : detail.state === "success" ? `${action} complete` : `${action} failed`;
    const message = detail.state === "review" ? "Confirm this package on the device." : detail.state === "installing" ? `Do not operate the device until the ${verb.toLowerCase()} finishes.` : detail.state === "success" ? "The App is available to you and Pi Agent." : detail.error || `The package could not be ${detail.update ? "updated" : "installed"}.`;
    const schema = detail.update ? `SCHEMA ${detail.currentSchemaVersion} TO ${detail.schemaVersion}` : `SCHEMA ${detail.schemaVersion}`;
    const version = detail.update ? `${detail.currentVersion} TO ${detail.version}` : detail.version;
    const tone = detail.state === "failed" ? "danger" : detail.state === "success" ? "success" : detail.state === "installing" ? "warning" : "accent";
    const surface = tone === "danger" ? "dangerSoft" : tone === "success" ? "successSoft" : tone === "warning" ? "warningSoft" : "accentSoft";
    const color = tone === "danger" ? "danger" : tone === "success" ? "success" : tone === "warning" ? "warningText" : "accent";
    const intro = View.PageIntro({ eyebrow: "APP PACKAGE", title: detail.name, description: `VERSION ${version}  |  ${schema}` });
    const manifest = View.Column({ style: { grow: 1, gap: LANDSCAPE ? 6 : 16 }, children: [
      View.SectionHeading({ title: "REQUESTED ACCESS", detail: "PACKAGE MANIFEST" }),
      View.Card({ style: { grow: 1, paddingX: LANDSCAPE ? 16 : 24, paddingY: LANDSCAPE ? 12 : 24, gap: LANDSCAPE ? 8 : 20 }, children: [
        View.Text({ text: "NETWORK", style: { color: "muted", fontWeight: "bold" } }),
        View.Text({ text: detail.network.length ? detail.network.slice(0, 2).join("\n") : "NO NETWORK ACCESS", style: { fontSize: "md" } }),
        View.Box({ style: { height: 2, background: "border" } }),
        View.Text({ text: "CREDENTIALS", style: { color: "muted", fontWeight: "bold" } }),
        View.Text({ text: detail.update ? "PRESERVE INSTALLED CREDENTIALS" : detail.credentials.length ? detail.credentials.slice(0, 3).join(", ") : "NONE", style: { fontSize: "md" } }),
        View.Box({ style: { height: 2, background: "border" } }),
        View.Text({ text: "CAPABILITIES", style: { color: "muted", fontWeight: "bold" } }),
        View.Text({ text: `${detail.tools} TOOLS  |  ${detail.schedules} SCHEDULES`, style: { fontSize: "md" } }),
      ] }),
    ] });
    const result = View.Column({ style: { grow: LANDSCAPE ? 1 : 0, height: LANDSCAPE ? undefined : 150, paddingX: 24, justify: "center", gap: 12, radius: 12, background: surface }, children: [
      View.Text({ text: title, style: { color, fontSize: "lg", fontWeight: "bold" } }),
      View.Text({ text: message, style: { color: "muted" } }),
    ] });
    const actionButton = View.ActionButton({
      label: detail.state === "review" ? verb : detail.state === "installing" ? `${progress}...` : "DONE",
      disabled: detail.state === "installing",
      tone: detail.state === "review" ? "primary" : "neutral",
      onPress: () => PocketPi.command(detail.state === "review" ? "apps.install" : "apps.dismissInstall"),
      style: { width: "full", height: LANDSCAPE ? 56 : 80, marginY: LANDSCAPE ? 0 : 20 },
    });
    const content = LANDSCAPE
      ? View.Row({ style: { grow: 1, padding: 12, gap: 12 }, children: [
        View.Column({ style: { grow: 1, basis: 0 }, children: [intro, manifest] }),
        View.Column({ style: { grow: 1, basis: 0, gap: 12 }, children: [result, actionButton] }),
      ] })
      : [
        intro,
        View.Column({ style: { grow: 1, paddingX: 24, gap: 16 }, children: [manifest, result, actionButton] }),
      ];
    return View.Screen({ children: [
      View.Header({ title: status, accent: detail.state === "failed" ? "danger" : detail.state === "success" ? "ready" : "busy", metaTop: `LOCAL ${verb}`, metaBottom: "PHYSICAL CONFIRMATION" }),
      content,
      View.Box({ style: { height: LANDSCAPE ? 48 : 66 }, children: View.StatusBar({ text: "Package received over your local network", dark: true }) }),
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
    if (tab !== "files") fileDeleteMode.set(false);
    activeTab.set(tab);
    screen.set(tab);
    if (tab === "files") {
      fileDeleteError.set("");
      refreshFiles(filePath.get());
    }
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
    chatScroll = Math.min(chatScroll, Math.max(0, chatTurns.length - CHAT_VISIBLE_TURNS));
    syncChat();
  }

  function syncChat() {
    const end = chatTurns.length - chatScroll;
    const visible = chatTurns.slice(Math.max(0, end - CHAT_VISIBLE_TURNS), end);
    chatCount.set(visible.length);
    for (let index = 0; index < visible.length; index += 1) {
      chatSlots[index].user.set(visible[index].user);
      chatSlots[index].assistant.set(visible[index].assistant);
    }
  }

  function moveChat(delta) {
    chatScroll = Math.max(0, Math.min(Math.max(0, chatTurns.length - CHAT_VISIBLE_TURNS), chatScroll + delta));
    syncChat();
    return "";
  }

  function moveApps(delta) {
    appOffset.set(Math.max(0, Math.min(Math.max(0, apps.get().length - APP_VISIBLE_ROWS), appOffset.get() + delta)));
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
      fileOffset.set(Math.min(fileOffset.get(), Math.max(0, next.length - FILE_VISIBLE_ROWS)));
    } catch (error) {
      files.set([]);
      fileError.set(error instanceof Error ? error.message : String(error));
    }
  }

  function moveFiles(delta) {
    fileOffset.set(Math.max(0, Math.min(Math.max(0, files.get().length - FILE_VISIBLE_ROWS), fileOffset.get() + delta)));
    return "";
  }

  function goUpDirectory() {
    const parts = filePath.get().split("/");
    parts.pop();
    const path = parts.join("/");
    fileDeleteError.set("");
    filePath.set(path);
    fileOffset.set(0);
    refreshFiles(path);
    return "";
  }

  function openFileEntry(entry) {
    const path = joinPath(filePath.get(), entry.name);
    if (entry.kind === "dir") {
      fileDeleteError.set("");
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

  function canDeleteFile(entry) {
    return entry.kind === "file" && !SYSTEM_ROOTS.has(filePath.get().split("/", 1)[0]);
  }

  function requestDeleteFile(entry) {
    const path = joinPath(filePath.get(), entry.name);
    deletingFile.set(path);
    fileDeleteError.set("");
    return PocketPi.action("deleteFile", { path });
  }

  function finishFileDelete() {
    const path = deletingFile.get();
    if (!path) return "";
    const result = JSON.parse(fs.stat(path));
    deletingFile.set("");
    fileDeleteError.set(result.error === undefined ? "FILE IS STILL PRESENT" : "");
    refreshFiles();
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
    wifiOffset.set(Math.max(0, Math.min(Math.max(0, networks.get().length - WIFI_VISIBLE_ROWS), wifiOffset.get() + delta)));
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
    appOffset.set(Math.min(appOffset.get(), Math.max(0, (next.apps ?? []).length - APP_VISIBLE_ROWS)));
    install.set(next.install ?? null);
    uninstallingApp.set(next.uninstallingApp ?? "");
    uninstallError.set(next.uninstallError ?? "");

    const settings = next.settings ?? {};
    const wifi = settings.wifi ?? {};
    networks.set(wifi.networks ?? []);
    wifiOffset.set(Math.min(wifiOffset.get(), Math.max(0, (wifi.networks ?? []).length - WIFI_VISIBLE_ROWS)));
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
  View.mount(root, finishFileDelete);
  refreshFiles("");
})();
