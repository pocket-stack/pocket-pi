import { batch, createSignal, For, Show } from "solid-js";
import { Text, View } from "@pocketjs/framework/components";
import { mount } from "@pocketjs/framework";
import { readFileSync, readdirSync, type DirEntry } from "@pocketjs/framework/fs";
import { ActionButton, PocketHeader } from "../_shared/ui";
import { wrapLines, wrapPreview } from "../_shared/text";

const FONT_BODY = 2;
const FONT_CAPTION = 1;
const CHAT_TEXT_WIDTH = 536;
const READER_TEXT_WIDTH = 544;
const READER_PAGE_LINES = 46;

type Message = { role: string; text: string };
type Network = { ssid: string; rssiDbm: number; secured: boolean };
type Projection = {
  agent?: string;
  model?: string;
  messages?: Message[];
  schedule?: { name?: string | null; prompt?: string; next?: string; everyMinutes?: number | null };
  settings?: {
    wifi?: {
      connectedSsid?: string | null;
      ipAddress?: string | null;
      rssiDbm?: number | null;
      scanning?: boolean;
      networks?: Network[];
      status?: string;
    };
    firmwareVersion?: string;
    workspaceFree?: string;
  };
};

type Tab = "chat" | "files" | "apps" | "settings";
type Screen = Tab | "keyboard" | "viewer" | "reader";
type KeyboardPurpose = { type: "prompt" } | { type: "wifi"; ssid: string };
type FileEntry = { name: string; kind: "file" | "dir"; size: number };
type Reader = { author: "YOU" | "PI"; text: string };

const [projection, setProjection] = createSignal<Projection>({
  agent: "STARTING",
  model: "CODEX / UART / MAC",
  messages: [{ role: "assistant", text: "BOOTING PI AGENT..." }],
});
const [screen, setScreen] = createSignal<Screen>("chat");
const [activeTab, setActiveTab] = createSignal<Tab>("chat");
const [chatScroll, setChatScroll] = createSignal(0);
const [filePath, setFilePath] = createSignal("");
const [fileOffset, setFileOffset] = createSignal(0);
const [files, setFiles] = createSignal<FileEntry[]>([]);
const [fileError, setFileError] = createSignal("");
const [viewer, setViewer] = createSignal<{ path: string; text: string; lineOffset: number } | null>(null);
const [reader, setReader] = createSignal<Reader | null>(null);
const [readerOffset, setReaderOffset] = createSignal(0);
const [input, setInput] = createSignal("");
const [keyboardMode, setKeyboardMode] = createSignal<"letters" | "numbers">("letters");
const [uppercase, setUppercase] = createSignal(false);
const [keyboardPurpose, setKeyboardPurpose] = createSignal<KeyboardPurpose>({ type: "prompt" });
const [wifiOffset, setWifiOffset] = createSignal(0);
const fileCache = new Map<string, FileEntry[]>();

const letterRows = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];
const numberRows = ["1234567890", "-/:;()$&@", ".,?!'\"+"];
const tabNames: Tab[] = ["chat", "files", "apps", "settings"];

function formatSize(size: number): string {
  if (size < 1024) return size + " B";
  if (size < 1024 * 1024) return (size / 1024).toFixed(1) + " KB";
  return (size / (1024 * 1024)).toFixed(1) + " MB";
}

function joinPath(parent: string, name: string): string {
  return parent ? parent + "/" + name : name;
}

function refreshFiles(path = filePath(), force = false) {
  const cached = fileCache.get(path);
  if (!force && cached) {
    setFiles(cached);
    setFileError("");
    setFileOffset((value) => Math.min(value, Math.max(0, cached.length - 8)));
    return;
  }
  try {
    const next = (readdirSync(path, { withFileTypes: true }) as DirEntry[]).map((entry) => ({
      name: entry.name,
      kind: entry.isDirectory() ? "dir" as const : "file" as const,
      size: entry.size,
    }));
    fileCache.set(path, next);
    setFiles(next);
    setFileError("");
    setFileOffset((value) => Math.min(value, Math.max(0, next.length - 8)));
  } catch (error) {
    setFiles([]);
    setFileError(error instanceof Error ? error.message : String(error));
  }
}

function chatTurns(): Array<{ user: string; assistant: string }> {
  const out: Array<{ user: string; assistant: string }> = [];
  for (const message of projection().messages ?? []) {
    if (message.role === "user") {
      out.push({ user: message.text, assistant: "THINKING..." });
    } else if (out.length === 0) {
      out.push({ user: "TYPE A MESSAGE", assistant: message.text || "THINKING..." });
    } else {
      out[out.length - 1].assistant = message.text || "THINKING...";
    }
  }
  return out.length ? out : [{ user: "TYPE A MESSAGE", assistant: "BOOTING PI AGENT..." }];
}

function visibleTurns() {
  const all = chatTurns();
  const max = Math.max(0, all.length - 2);
  const scroll = Math.min(chatScroll(), max);
  const end = all.length - scroll;
  return all.slice(Math.max(0, end - 2), end);
}

function Header(props: { title: string }) {
  return (
    <PocketHeader
      title={props.title}
      accent={projection().agent === "FAULTED" ? "danger" : projection().agent === "THINKING" ? "busy" : "ready"}
    />
  );
}

function ScrollButtons(props: { top: string; bottom: string }) {
  return (
    <>
      <View class={props.top}><Text class="text-lg text-orange-600 font-bold">UP</Text></View>
      <View class={props.bottom}><Text class="text-lg text-orange-600 font-bold">DN</Text></View>
    </>
  );
}

function BottomBar() {
  return (
    <View class="h-[108] px-[10] py-4 flex-row gap-[10] bg-slate-950">
      <For each={tabNames}>{(name) => (
        <View class={activeTab() === name
          ? "w-[165] h-[76] items-center justify-center bg-orange-600"
          : "w-[165] h-[76] items-center justify-center bg-slate-900"}>
          <Text class="text-sm text-white font-bold">{name.toUpperCase()}</Text>
        </View>
      )}</For>
    </View>
  );
}

function ChatScreen() {
  const schedule = () => projection().schedule;
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      <Header title="ESP32 PI AGENT" />
      <View class="h-[686] px-6 pt-7 flex-col gap-[22]">
        <For each={visibleTurns()}>{(turn) => (
          <View class="w-[584] h-[298] px-6 py-5 flex-col rounded-xl shadow bg-white border-slate-100">
              <View class="h-[28] flex-row items-center gap-3">
                <View class="w-[10] h-[10] rounded bg-orange-500" />
                <Text class="text-sm text-orange-600 font-bold">YOU</Text>
              </View>
              <Text class="h-[76] pt-3 text-base text-slate-900">{wrapPreview(turn.user, FONT_BODY, CHAT_TEXT_WIDTH, 3)}</Text>
              <View class="h-[2] mx-1 my-4 bg-slate-100" />
              <View class="h-[28] flex-row items-center gap-3">
                <View class="w-[10] h-[10] rounded bg-emerald-500" />
                <Text class="text-sm text-emerald-700 font-bold">PI</Text>
              </View>
              <Text class="h-[84] pt-3 text-base text-slate-900">{wrapPreview(turn.assistant, FONT_BODY, CHAT_TEXT_WIDTH, 3)}</Text>
          </View>
        )}</For>
        <ScrollButtons
          top="absolute left-[628] top-[28] w-[68] h-[132] items-center justify-center bg-orange-100"
          bottom="absolute left-[628] top-[512] w-[68] h-[132] items-center justify-center bg-orange-100"
        />
      </View>
      <View class="h-[228] mx-6 px-6 py-5 flex-col rounded-xl shadow bg-white border-slate-100">
          <Text class="text-sm text-slate-600 font-bold">NEXT WAKE</Text>
          <Show when={schedule()?.name} fallback={
            <Text class="pt-6 text-base text-slate-500">{"NO WAKE SCHEDULED\n\nASK PI TO CREATE ONE WITH SCHEDULE.SET"}</Text>
          }>
            <Text class="pt-6 text-base text-slate-900 font-bold">
              {String(schedule()?.name) + "  " + String(schedule()?.next ?? "") + "\n\n" + String(schedule()?.prompt ?? "")}
            </Text>
          </Show>
      </View>
      <View class="h-[146] px-6 pt-7 flex-col bg-slate-50">
        <View class="h-[80]"><ActionButton label="TYPE A MESSAGE" /></View>
      </View>
      <BottomBar />
    </View>
  );
}

function FilesScreen() {
  const visible = () => files().slice(fileOffset(), fileOffset() + 8);
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      <Header title={filePath() ? "< WORKSPACE FILES" : "WORKSPACE FILES"} />
      <View class="h-[1060] px-6 pt-5 flex-col">
        <View class="h-[48] px-4 justify-center bg-slate-100">
          <Text class="text-sm text-slate-600">{"/workspace" + (filePath() ? "/" + filePath() : "")}</Text>
        </View>
        <View class="pt-[10] flex-col gap-[12]">
          <For each={visible()}>{(entry) => (
            <View class="w-[584] h-[92] px-[18] flex-row items-center gap-5 bg-white">
              <View class={entry.kind === "dir" ? "w-[48] h-[48] items-center justify-center bg-blue-100" : "w-[48] h-[48] items-center justify-center bg-emerald-100"}>
                <Text class={entry.kind === "dir" ? "text-base text-blue-700 font-bold" : "text-base text-emerald-700 font-bold"}>{entry.kind === "dir" ? "D" : "F"}</Text>
              </View>
              <View class="w-[474] flex-col gap-2">
                <Text class="text-base text-slate-900 font-bold">{(entry.name + (entry.kind === "dir" ? "/" : "")).slice(0, 60)}</Text>
                <Text class="text-sm text-slate-500">{entry.kind === "dir" ? "FOLDER" : formatSize(entry.size)}</Text>
              </View>
            </View>
          )}</For>
        </View>
        <Show when={files().length === 0}>
          <Text class="pt-20 pl-10 text-base text-slate-500">{fileError() || "THIS DIRECTORY IS EMPTY"}</Text>
        </Show>
        <Show when={files().length > 8}><ScrollButtons
            top="absolute left-[628] top-[78] w-[68] h-[132] items-center justify-center bg-orange-100"
            bottom="absolute left-[628] top-[828] w-[68] h-[132] items-center justify-center bg-orange-100"
          /></Show>
      </View>
      <BottomBar />
    </View>
  );
}

function AppsScreen() {
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      <Header title="APPS" />
      <View class="h-[1060] px-6 pt-7 flex-col gap-4">
        <Text class="text-sm text-slate-500 font-bold">2 INSTALLED APPS</Text>
        <View class="w-[672] h-[150] px-6 flex-row items-center justify-between bg-white">
          <View class="flex-row items-center gap-5">
            <View class="w-[68] h-[68] items-center justify-center bg-emerald-100"><Text class="text-xl text-emerald-700 font-bold">R</Text></View>
            <View class="flex-col gap-2"><Text class="text-lg text-slate-900 font-bold">Robinhood</Text><Text class="text-base text-slate-600">Portfolio and positions</Text><Text class="text-sm text-slate-500">UPDATES EVERY 5 MINUTES</Text></View>
          </View>
          <Text class="text-2xl text-orange-600">›</Text>
        </View>
        <View class="w-[672] h-[150] px-6 flex-row items-center justify-between bg-white">
          <View class="flex-row items-center gap-5">
            <View class="w-[68] h-[68] items-center justify-center bg-indigo-100"><Text class="text-xl text-indigo-700 font-bold">E</Text></View>
            <View class="flex-col gap-2"><Text class="text-lg text-slate-900 font-bold">Exa Research</Text><Text class="text-base text-slate-600">Agent search history</Text><Text class="text-sm text-slate-500">STORED LOCALLY IN SQLITE</Text></View>
          </View>
          <Text class="text-2xl text-orange-600">›</Text>
        </View>
        <View class="mt-4 h-[112] px-6 justify-center bg-slate-100">
          <Text class="text-sm text-slate-600">{"APP DATA STAYS ISOLATED.\nPI AGENT CAN USE EACH APP'S TOOLS."}</Text>
        </View>
      </View>
      <BottomBar />
    </View>
  );
}

function SettingsScreen() {
  const settings = () => projection().settings ?? {};
  const wifi = () => settings().wifi ?? {};
  const networks = () => (wifi().networks ?? []).slice(wifiOffset(), wifiOffset() + 5);
  const detail = () => wifi().ipAddress
    ? "IP " + wifi().ipAddress + "  RSSI " + String(wifi().rssiDbm ?? "--") + " DBM"
    : wifi().status || "SCAN AND SELECT A NETWORK";
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      <Header title="SETTINGS" />
      <View class="h-[1060] px-6 pt-5 flex-col">
        <View class="relative h-[154] px-5 pt-5 flex-col bg-white">
          <Text class="text-base text-slate-900 font-bold">WI-FI</Text>
          <Text class="pt-3 text-base text-orange-600 font-bold">{wifi().connectedSsid || "NOT CONNECTED"}</Text>
          <Text class="pt-3 text-sm text-slate-500">{detail()}</Text>
          <View class="absolute left-[456] top-[14] w-[196] h-[72] items-center justify-center bg-orange-600">
            <Text class="text-base text-white font-bold">{wifi().scanning ? "SCANNING" : "SCAN"}</Text>
          </View>
        </View>
        <Text class="h-[40] pt-3 text-sm text-slate-500">AVAILABLE NETWORKS</Text>
        <View class="h-[470] flex-col gap-2">
          <Show when={networks().length > 0} fallback={
            <View class="h-[214] px-7 items-center justify-center bg-slate-100"><Text class="text-base text-slate-500 font-bold">{"NO NETWORK LIST YET\n\nTAP SCAN TO FIND WI-FI"}</Text></View>
          }>
            <For each={networks()}>{(network) => (
              <View class="w-[584] h-[84] px-5 flex-row items-center justify-between bg-white">
                <Text class="text-base text-slate-900 font-bold">{network.ssid}</Text>
                <Text class="text-sm text-slate-500">{String(network.rssiDbm) + " DBM  " + (network.secured ? "LOCK" : "OPEN")}</Text>
              </View>
            )}</For>
          </Show>
          <Show when={(wifi().networks ?? []).length > 5}><ScrollButtons
              top="absolute left-[604] top-[0] w-[68] h-[132] items-center justify-center bg-orange-100"
              bottom="absolute left-[604] top-[320] w-[68] h-[132] items-center justify-center bg-orange-100"
            /></Show>
        </View>
        <View class="h-[160] px-5 pt-5 flex-col bg-white">
          <Text class="text-sm text-slate-500">MODEL BACKEND</Text>
          <Text class="pt-3 text-base text-slate-900 font-bold">{projection().model ?? "UNKNOWN"}</Text>
          <Text class="pt-3 text-sm text-slate-600">{"FIRMWARE " + String(settings().firmwareVersion ?? "0.1.0") + "  ·  WORKSPACE FREE " + String(settings().workspaceFree ?? "--")}</Text>
        </View>
        <View class="h-[108] pt-7 flex-row gap-4">
          <View class="w-[316] h-[80] items-center justify-center bg-slate-100"><Text class="text-base text-slate-900 font-bold">FORGET WI-FI</Text></View>
          <View class="w-[340] h-[80] items-center justify-center bg-red-100"><Text class="text-base text-red-500 font-bold">RESTART DEVICE</Text></View>
        </View>
      </View>
      <BottomBar />
    </View>
  );
}

function KeyboardScreen() {
  const purpose = () => keyboardPurpose();
  const rows = () => keyboardMode() === "letters" ? letterRows : numberRows;
  const display = () => purpose().type === "wifi" ? "*".repeat(input().length) : input();
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      <Header title={purpose().type === "wifi" ? "WIFI PASSWORD" : "NEW MESSAGE"} />
      <View class="h-[1052] px-6 pt-5 flex-col">
        <View class="h-[270] px-[22] pt-6 bg-white">
          <Text class={input() ? "text-lg text-slate-900" : "text-lg text-slate-400"}>{display() || (purpose().type === "wifi" ? "ENTER NETWORK PASSWORD..." : "TYPE YOUR MESSAGE...")}</Text>
        </View>
        <View class="h-[86] px-1 flex-row items-center justify-between"><Text class="text-sm text-slate-500">{String(input().length) + " / " + (purpose().type === "wifi" ? "63" : "256") + " CHARACTERS"}</Text><View class="w-[132] h-[58] items-center justify-center bg-slate-100"><Text class="text-sm text-slate-900 font-bold">CLEAR</Text></View></View>
        <For each={rows()}>{(row, rowIndex) => (
          <View class="h-[140] flex-row gap-2">
            <For each={row.split("")}>{(key) => (
              <View class="grow h-[120] items-center justify-center bg-white"><Text class="text-xl text-slate-900 font-bold">{uppercase() ? key.toUpperCase() : key}</Text></View>
            )}</For>
            <Show when={rowIndex() === 2}><View class="w-[104] h-[120] items-center justify-center bg-slate-100"><Text class="text-sm text-slate-900 font-bold">DEL</Text></View></Show>
          </View>
        )}</For>
        <View class="h-[176] flex-row gap-2">
          <View class="w-[92] h-[156] items-center justify-center bg-slate-100"><Text class="text-sm text-slate-900 font-bold">{keyboardMode() === "letters" ? "123" : "ABC"}</Text></View>
          <View class="w-[300] h-[156] items-center justify-center bg-slate-100"><Text class="text-sm text-slate-900 font-bold">SPACE</Text></View>
          <View class="w-[144] h-[156] items-center justify-center bg-slate-100"><Text class="text-sm text-slate-900 font-bold">{keyboardMode() === "letters" ? "SHIFT" : ".  ?"}</Text></View>
          <View class="w-[112] h-[156] items-center justify-center bg-emerald-500"><Text class="text-sm text-slate-950 font-bold">{purpose().type === "wifi" ? "JOIN" : "SEND"}</Text></View>
        </View>
      </View>
      <View class="h-[108] px-6 py-3 flex-col bg-slate-50"><View class="h-[84] items-center justify-center bg-slate-100"><Text class="text-sm text-slate-900 font-bold">CLOSE KEYBOARD</Text></View></View>
    </View>
  );
}

function ViewerScreen() {
  const current = () => viewer();
  const lines = () => (current()?.text ?? "").split("\n");
  const offset = () => current()?.lineOffset ?? 0;
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      <Header title="< FILE VIEWER" />
      <View class="h-[1168] px-6 pt-5 flex-col">
        <View class="h-[82] px-4 justify-center bg-white"><Text class="text-base text-slate-900 font-bold">{current()?.path ?? "NO FILE OPEN"}</Text></View>
        <View class="w-[584] h-[900] px-5 pt-5 bg-slate-950">
          <Text class="text-sm text-slate-200">{lines().slice(offset(), offset() + 39).join("\n")}</Text>
        </View>
        <Text class="pt-3 text-sm text-slate-500">{"LINES " + String(offset() + 1) + "-" + String(Math.min(offset() + 39, lines().length)) + " / " + String(lines().length)}</Text>
        <ScrollButtons
          top="absolute left-[628] top-[58] w-[68] h-[132] items-center justify-center bg-orange-100"
          bottom="absolute left-[628] top-[828] w-[68] h-[132] items-center justify-center bg-orange-100"
        />
      </View>
    </View>
  );
}

function ReaderScreen() {
  const current = () => reader();
  const lines = () => wrapLines(current()?.text ?? "", FONT_CAPTION, READER_TEXT_WIDTH);
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      <Header title="< MESSAGE READER" />
      <View class="h-[1168] px-6 pt-5 flex-col">
        <View class="h-[82] px-4 justify-center bg-white"><Text class="text-base text-orange-600 font-bold">{current()?.author ?? "PI"}</Text></View>
        <View class="w-[584] h-[900] px-5 pt-5 bg-white"><Text class="text-sm text-slate-900">{lines().slice(readerOffset(), readerOffset() + READER_PAGE_LINES).join("\n")}</Text></View>
        <ScrollButtons
          top="absolute left-[628] top-[58] w-[68] h-[132] items-center justify-center bg-orange-100"
          bottom="absolute left-[628] top-[828] w-[68] h-[132] items-center justify-center bg-orange-100"
        />
      </View>
    </View>
  );
}

function Root() {
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      {screen() === "chat" ? <ChatScreen />
        : screen() === "files" ? <FilesScreen />
        : screen() === "apps" ? <AppsScreen />
        : screen() === "settings" ? <SettingsScreen />
        : screen() === "keyboard" ? <KeyboardScreen />
        : screen() === "viewer" ? <ViewerScreen />
        : <ReaderScreen />}
    </View>
  );
}

function openTab(tab: Tab) {
  batch(() => {
    setActiveTab(tab);
    setScreen(tab);
    if (tab === "files") refreshFiles();
  });
}

function keyboardCharacterAt(x: number, y: number): string | null {
  const rows = keyboardMode() === "letters" ? letterRows : numberRows;
  const ys = [488, 628, 768];
  for (let row = 0; row < 3; row++) {
    if (y < ys[row] || y >= ys[row] + 120) continue;
    const chars = rows[row];
    const available = row === 2 ? 560 : 672;
    const width = available / chars.length;
    const index = Math.floor((x - 24) / width);
    if (index >= 0 && index < chars.length) return chars[index];
  }
  return null;
}

function handleKeyboardTap(x: number, y: number): string {
  if (y >= 1164) {
    setInput("");
    setScreen(activeTab());
    return "";
  }
  if (x >= 548 && y >= 402 && y <= 482) {
    setInput("");
    return "";
  }
  const key = keyboardCharacterAt(x, y);
  if (key) {
    const next = keyboardMode() === "letters" && uppercase() ? key.toUpperCase() : key;
    const limit = keyboardPurpose().type === "wifi" ? 63 : 256;
    if (input().length < limit) setInput(input() + next);
    return "";
  }
  if (x >= 592 && y >= 768 && y <= 888) {
    setInput(input().slice(0, -1));
    return "";
  }
  if (y >= 908 && y <= 1064) {
    if (x <= 116) {
      setKeyboardMode(keyboardMode() === "letters" ? "numbers" : "letters");
      setUppercase(false);
    } else if (x <= 424) {
      if (input()) setInput(input() + " ");
    } else if (x <= 576) {
      if (keyboardMode() === "letters") setUppercase(!uppercase());
      else setInput(input() + (x <= 500 ? "." : "?"));
    } else if (input().trim()) {
      const value = input().trim();
      const purpose = keyboardPurpose();
      setInput("");
      setKeyboardMode("letters");
      setUppercase(false);
      setScreen(activeTab());
      if (purpose.type === "wifi") return JSON.stringify({ type: "settings", command: "connect", ssid: purpose.ssid, password: value });
      return JSON.stringify({ type: "submitPrompt", prompt: value });
    }
  }
  return "";
}

mount(() => <Root />);
queueMicrotask(() => refreshFiles("", true));

(globalThis as any).PocketPiApp = {
  tick() {
    return "";
  },
  update(line: string) {
    setProjection(JSON.parse(line));
    return "";
  },
  dataChanged() {
    fileCache.delete(filePath());
    if (activeTab() === "files") refreshFiles(filePath(), true);
    return "";
  },
  invokeTool() {
    return JSON.stringify({ text: "Pi Agent has no App tools", isError: true });
  },
  invokeTask() {
    return JSON.stringify({ text: "Pi Agent has no App tasks", isError: true });
  },
  tap(x: number, y: number) {
    if (screen() === "keyboard") return handleKeyboardTap(x, y);
    if (screen() === "viewer") {
      const current = viewer();
      if (x < 104 && y < 112) {
        setViewer(null);
        setScreen("files");
      } else if (current && x >= 620 && y >= 170 && y <= 340) {
        setViewer({ ...current, lineOffset: Math.max(0, current.lineOffset - 18) });
      } else if (current && x >= 620 && y >= 920 && y <= 1100) {
        setViewer({ ...current, lineOffset: Math.min(Math.max(0, current.text.split("\n").length - 39), current.lineOffset + 18) });
      }
      return "";
    }
    if (screen() === "reader") {
      if (x < 104 && y < 112) {
        setReader(null);
        setReaderOffset(0);
        setScreen("chat");
      } else if (x >= 620 && y >= 170 && y <= 340) {
        setReaderOffset(Math.max(0, readerOffset() - 18));
      } else if (x >= 620 && y >= 920 && y <= 1100) {
        const lineCount = wrapLines(reader()?.text ?? "", FONT_CAPTION, READER_TEXT_WIDTH).length;
        setReaderOffset(Math.min(Math.max(0, lineCount - READER_PAGE_LINES), readerOffset() + 18));
      }
      return "";
    }
    if (y >= 1172) {
      openTab(tabNames[Math.min(3, Math.floor(x / 180))]);
      return "";
    }
    if (screen() === "chat") {
      if (x >= 620 && y >= 140 && y <= 272) setChatScroll(chatScroll() + 2);
      else if (x >= 620 && y >= 624 && y <= 756) setChatScroll(Math.max(0, chatScroll() - 2));
      else if (x < 610 && y >= 140 && y < 738) {
        const row = Math.floor((y - 140) / 320);
        const turn = visibleTurns()[row];
        if (turn) {
          const isPi = y >= 140 + row * 320 + 150;
          setReader({ author: isPi ? "PI" : "YOU", text: isPi ? turn.assistant : turn.user });
          setReaderOffset(0);
          setScreen("reader");
        }
      } else if (y >= 1070 && y <= 1150) {
        setKeyboardPurpose({ type: "prompt" });
        setInput("");
        setScreen("keyboard");
      }
      return "";
    }
    if (screen() === "files") {
      if (x < 104 && y < 112 && filePath()) {
        const parts = filePath().split("/");
        parts.pop();
        const next = parts.join("/");
        setFilePath(next);
        setFileOffset(0);
        refreshFiles(next);
      } else if (x >= 620 && y >= 170 && y <= 340) {
        setFileOffset(Math.max(0, fileOffset() - 4));
      } else if (x >= 620 && y >= 920 && y <= 1100) {
        setFileOffset(Math.min(Math.max(0, files().length - 8), fileOffset() + 4));
      } else if (x < 610 && y >= 190) {
        const row = Math.floor((y - 190) / 104);
        const entry = files()[fileOffset() + row];
        if (entry) {
          const path = joinPath(filePath(), entry.name);
          if (entry.kind === "dir") {
            setFilePath(path);
            setFileOffset(0);
            refreshFiles(path);
          } else {
            try {
              setViewer({ path: "/workspace/" + path, text: readFileSync(path, "utf8"), lineOffset: 0 });
              setScreen("viewer");
            } catch (error) {
              setFileError(error instanceof Error ? error.message : String(error));
            }
          }
        }
      }
      return "";
    }
    if (screen() === "apps") {
      if (y >= 162 && y < 324) {
        return JSON.stringify({ type: "navigate", app: "robinhood" });
      }
      if (y >= 324 && y < 490) {
        return JSON.stringify({ type: "navigate", app: "exa" });
      }
      return "";
    }
    if (screen() === "settings") {
      const networks = projection().settings?.wifi?.networks ?? [];
      if (x >= 480 && y >= 126 && y <= 218) return JSON.stringify({ type: "settings", command: "scan" });
      if (x >= 620 && y >= 330 && y <= 462) setWifiOffset(Math.max(0, wifiOffset() - 4));
      else if (x >= 620 && y >= 650 && y <= 782) setWifiOffset(Math.min(Math.max(0, networks.length - 5), wifiOffset() + 4));
      else if (x < 610 && y >= 330 && y < 790) {
        const row = Math.floor((y - 330) / 92);
        const network = networks[wifiOffset() + row];
        if (network) {
          if (!network.secured) return JSON.stringify({ type: "settings", command: "connect", ssid: network.ssid, password: "" });
          setKeyboardPurpose({ type: "wifi", ssid: network.ssid });
          setInput("");
          setScreen("keyboard");
        }
      } else if (x <= 340 && y >= 1010 && y <= 1090) return JSON.stringify({ type: "settings", command: "forget" });
      else if (x >= 356 && y >= 1010 && y <= 1090) return JSON.stringify({ type: "settings", command: "restart" });
    }
    return "";
  },
};
