import { createSignal, For, Show } from "solid-js";
import { Focusable, Text, View } from "@pocketjs/framework/components";
import { onButtonPress, onFrame } from "@pocketjs/framework/lifecycle";
import { BTN } from "@pocketjs/framework/input";
import { connect, type Command, type Snapshot, type Turn, type ViewName } from "./protocol.ts";

const FALLBACK: Snapshot = {
  revision: 0,
  active_view: "chat",
  chat: {
    busy: false,
    turns: [
      { id: 1, role: "user", text: "What is running here?", streaming: false },
      {
        id: 2,
        role: "assistant",
        text: "Pocket Pi embedded profile is running in the macOS simulator.",
        streaming: false,
      },
    ],
  },
  workspace: {
    path: "/workspace",
    entries: [
      { name: "memory.md", size: 128, modified_unix_seconds: 0 },
      { name: "notes.txt", size: 64, modified_unix_seconds: 0 },
    ],
    open_file: null,
  },
  system: { backend: "SIMULATED", network: "ONLINE", free_ram_kib: 24576, fps: 60 },
};

function Button(props: { label: string; active?: boolean; onPress: () => void }) {
  return (
    <Focusable
      class={
        props.active
          ? "flex-1 h-10 items-center justify-center rounded-xl bg-blue-600 focus:bg-blue-500"
          : "flex-1 h-10 items-center justify-center rounded-xl bg-slate-800 focus:bg-slate-700"
      }
      onPress={props.onPress}
    >
      <Text class="text-xs text-white font-bold">{props.label}</Text>
    </Focusable>
  );
}

function StatusBar(props: { state: Snapshot }) {
  return (
    <View class="h-7 flex-row items-center justify-between px-3 bg-slate-950">
      <Text class="text-xs text-slate-300">RAM {props.state.system.free_ram_kib}K</Text>
      <Text class="text-xs text-slate-300">{props.state.system.backend}</Text>
      <Text class="text-xs text-emerald-400">{props.state.system.network}</Text>
      <Text class="text-xs text-slate-300">{props.state.system.fps} FPS</Text>
    </View>
  );
}

function Message(props: { turn: Turn }) {
  const assistant = props.turn.role === "assistant";
  const lines = () => wrapText(props.turn.text, 42, 4);
  return (
    <View
      class={
        assistant
          ? "flex-col gap-1 p-3 rounded-xl bg-slate-800"
          : "flex-col gap-1 p-3 rounded-xl bg-blue-950"
      }
    >
      <Text class={assistant ? "text-xs text-emerald-400 font-bold" : "text-xs text-blue-300 font-bold"}>
        {assistant ? "PI" : "YOU"}{props.turn.streaming ? "  ..." : ""}
      </Text>
      <Text class="text-sm text-slate-100">{lines()[0] || " "}</Text>
      <Text class="text-sm text-slate-100">{lines()[1] || " "}</Text>
      <Text class="text-sm text-slate-100">{lines()[2] || " "}</Text>
      <Text class="text-sm text-slate-100">{lines()[3] || " "}</Text>
    </View>
  );
}

function wrapText(text: string, columns: number, maxLines: number): string[] {
  const words = text.trim().split(/\s+/).filter(Boolean);
  const lines: string[] = [];
  let current = "";
  for (const word of words) {
    if (current && current.length + word.length + 1 > columns) {
      lines.push(current);
      current = word;
    } else {
      current = current ? `${current} ${word}` : word;
    }
  }
  if (current) lines.push(current);
  return lines.slice(-maxLines);
}

function Chat(props: { state: Snapshot; send: (command: Command) => void }) {
  const empty: Turn = {
    id: 0,
    role: "user",
    text: "Tap RUN DEMO TURN to talk to embedded Pi.",
    streaming: false,
  };
  const previous = () => props.state.chat.turns.at(-2) || empty;
  const latest = () => props.state.chat.turns.at(-1) || empty;
  return (
    <View class="grow flex-col gap-3 p-3 bg-slate-900">
      <View class="flex-row items-center justify-between">
        <Text class="text-lg text-white font-bold">CHAT</Text>
        <Text class={props.state.chat.busy ? "text-xs text-amber-400" : "text-xs text-emerald-400"}>
          {props.state.chat.busy ? "THINKING" : "READY"}
        </Text>
      </View>
      <View class="grow flex-col justify-end gap-2">
        <Message turn={previous()} />
        <Message turn={latest()} />
      </View>
      <Focusable
        class="h-11 items-center justify-center rounded-xl bg-blue-600 focus:bg-blue-500"
        onPress={() => props.send({ type: "send_prompt", text: "Give me a short status update." })}
      >
        <Text class="text-sm text-white font-bold">RUN DEMO TURN</Text>
      </Focusable>
    </View>
  );
}

function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  return `${Math.round(size / 1024)} KB`;
}

function Workspace(props: { state: Snapshot; send: (command: Command) => void }) {
  const [line, setLine] = createSignal(0);
  const opened = () => props.state.workspace.open_file;
  const visibleContent = () => opened()?.content.split("\n").slice(line(), line() + 15).join("\n") || "";

  return (
    <View class="grow flex-col gap-3 p-3 bg-slate-900">
      <Show
        when={opened()}
        fallback={
          <>
            <View class="flex-row items-end justify-between">
              <Text class="text-lg text-white font-bold">WORKSPACE</Text>
              <Text class="text-xs text-slate-400">{props.state.workspace.entries.length} FILES</Text>
            </View>
            <Text class="text-xs text-slate-500">{props.state.workspace.path}</Text>
            <View class="grow flex-col gap-2">
              <For each={props.state.workspace.entries.slice(0, 8)}>
                {(entry) => (
                  <Focusable
                    class="h-12 flex-row items-center justify-between px-3 rounded-xl bg-slate-800 focus:bg-slate-700"
                    onPress={() => props.send({ type: "open_path", name: entry.name })}
                  >
                    <Text class="text-sm text-white font-bold">{entry.name}</Text>
                    <Text class="text-xs text-slate-400">{formatBytes(entry.size)}</Text>
                  </Focusable>
                )}
              </For>
            </View>
          </>
        }
      >
        <View class="flex-row items-center gap-2">
          <Focusable
            class="w-12 h-10 items-center justify-center rounded-xl bg-slate-800 focus:bg-slate-700"
            onPress={() => props.send({ type: "close_file" })}
          >
            <Text class="text-lg text-white">X</Text>
          </Focusable>
          <Text class="text-sm text-white font-bold">{opened()?.name}</Text>
        </View>
        <View class="grow p-3 rounded-xl bg-slate-950">
          <Text class="text-sm text-slate-200">{visibleContent()}</Text>
        </View>
        <View class="h-10 flex-row gap-2">
          <Button label="UP" onPress={() => setLine(Math.max(0, line() - 8))} />
          <Button label="DOWN" onPress={() => setLine(line() + 8)} />
        </View>
      </Show>
    </View>
  );
}

export default function App() {
  const service = connect();
  const [state, setState] = createSignal(FALLBACK);

  const send = (command: Command) => {
    service?.send(command);
    if (command.type === "switch_view") {
      setState((previous) => ({ ...previous, active_view: command.view }));
    }
  };
  const switchView = (view: ViewName) => send({ type: "switch_view", view });

  onFrame(() => {
    const next = service?.poll();
    if (next) setState(next);
  });
  onButtonPress(BTN.LTRIGGER, () => switchView("chat"));
  onButtonPress(BTN.RTRIGGER, () => switchView("workspace"));

  return (
    <View class="w-full h-full flex-col bg-slate-950">
      <StatusBar state={state()} />
      <Show
        when={state().active_view === "chat"}
        fallback={<Workspace state={state()} send={send} />}
      >
        <Chat state={state()} send={send} />
      </Show>
      <View class="h-14 flex-row gap-2 p-2 bg-slate-950">
        <Button label="CHAT" active={state().active_view === "chat"} onPress={() => switchView("chat")} />
        <Button
          label="WORKSPACE"
          active={state().active_view === "workspace"}
          onPress={() => switchView("workspace")}
        />
      </View>
    </View>
  );
}
