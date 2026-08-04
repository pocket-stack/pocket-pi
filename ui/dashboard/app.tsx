import { createSignal, Show } from "solid-js";
import { Text, View } from "@pocketjs/framework/components";
import { onButtonPress } from "@pocketjs/framework/lifecycle";
import { BTN } from "@pocketjs/framework/input";

type Link = "online" | "connecting" | "offline";

interface Position {
  symbol: string;
  quantity: string;
  value: string;
  change: string;
  positive: boolean;
}

interface DashboardState {
  agent: "idle" | "thinking" | "acting" | "stopped";
  authMode: "Coding Plan" | "API key" | "Unconfigured";
  wifi: Link;
  codex: Link;
  robinhood: Link;
  equity: string;
  buyingPower: string;
  dayChange: string;
  positions: Position[];
  readOnly: boolean;
  updated: string;
}

// Replaced by the firmware effect driver. Keeping a deterministic initial
// projection makes the screen renderable in PocketJS host tests without any
// account credentials.
const INITIAL: DashboardState = {
  agent: "idle",
  authMode: "Coding Plan",
  wifi: "online",
  codex: "online",
  robinhood: "online",
  equity: "$12,480.32",
  buyingPower: "$2,104.18",
  dayChange: "+$184.22  +1.50%",
  positions: [
    { symbol: "NVDA", quantity: "8.00", value: "$1,438.40", change: "+2.18%", positive: true },
    { symbol: "AAPL", quantity: "5.00", value: "$1,086.25", change: "+0.42%", positive: true },
    { symbol: "QQQ", quantity: "3.00", value: "$1,602.78", change: "-0.31%", positive: false },
  ],
  readOnly: true,
  updated: "UPDATED 14:32:08",
};

// Font coverage for values supplied dynamically by the device driver.
const DYNAMIC_GLYPHS = "0123456789$,+-.:%ABCDEFGHIJKLMNOPQRSTUVWXYZ";
void DYNAMIC_GLYPHS;

function statusColor(state: Link): string {
  if (state === "online") return "w-2 h-2 rounded-full bg-emerald-500";
  if (state === "connecting") return "w-2 h-2 rounded-full bg-amber-500";
  return "w-2 h-2 rounded-full bg-red-500";
}

function Status(props: { name: string; state: Link }) {
  return (
    <View class="flex-row items-center gap-1">
      <View class={statusColor(props.state)} />
      <Text class="text-xs text-slate-500">{props.name}</Text>
    </View>
  );
}

function Overview(props: { state: DashboardState }) {
  return (
    <View class="grow flex-col gap-2">
      <View class="flex-row gap-2">
        <View class="flex-1 flex-col gap-1 p-2 rounded-xl shadow bg-white border-slate-200">
          <Text class="text-xs text-slate-500 tracking-wide">TOTAL EQUITY</Text>
          <Text class="text-2xl text-slate-950 font-bold">{props.state.equity}</Text>
          <Text class="text-xs text-emerald-600">{props.state.dayChange}</Text>
        </View>
        <View class="w-[138] flex-col gap-1 p-2 rounded-xl shadow bg-slate-900 border-slate-800">
          <Text class="text-xs text-slate-400 tracking-wide">BUYING POWER</Text>
          <Text class="text-xl text-white font-bold">{props.state.buyingPower}</Text>
          <Text class="text-xs text-slate-400">{props.state.authMode}</Text>
        </View>
      </View>

      <View class="grow flex-col gap-1 p-2 rounded-xl shadow bg-white border-slate-200">
        <View class="flex-row justify-between">
          <Text class="text-xs text-slate-500 tracking-wide">POSITIONS</Text>
          <Text class="text-xs text-slate-400">{props.state.updated}</Text>
        </View>
        {props.state.positions.map((position) => (
          <View class="flex-row items-center justify-between py-[2] border-slate-100">
            <View class="w-[88] flex-row items-end gap-1">
              <Text class="text-sm text-slate-900 font-bold">{position.symbol}</Text>
              <Text class="text-xs text-slate-400">{position.quantity}</Text>
            </View>
            <Text class="text-sm text-slate-700">{position.value}</Text>
            <Text class={position.positive ? "text-xs text-emerald-600" : "text-xs text-red-500"}>
              {position.change}
            </Text>
          </View>
        ))}
      </View>
    </View>
  );
}

function Systems(props: { state: DashboardState }) {
  const rows: Array<{ name: string; state: Link; detail: string }> = [
    { name: "WIFI COPROCESSOR", state: props.state.wifi, detail: props.state.wifi.toUpperCase() },
    { name: "CODEX BACKEND", state: props.state.codex, detail: props.state.authMode },
    { name: "ROBINHOOD MCP", state: props.state.robinhood, detail: props.state.robinhood.toUpperCase() },
  ];
  return (
    <View class="grow flex-col gap-2">
      {rows.map((row) => (
        <View class="flex-row items-center justify-between px-3 py-2 rounded-xl shadow bg-white border-slate-200">
          <View class="flex-row items-center gap-2">
            <View class={statusColor(row.state)} />
            <Text class="text-sm text-slate-800 font-bold">{row.name}</Text>
          </View>
          <Text class="text-xs text-slate-500">{row.detail}</Text>
        </View>
      ))}
      <View class="grow flex-col justify-center items-center rounded-xl bg-slate-900 border-slate-800">
        <Text class="text-xs text-slate-400 tracking-wide">AGENT STATE</Text>
        <Text class="text-2xl text-white font-bold">{props.state.agent.toUpperCase()}</Text>
        <Text class={props.state.readOnly ? "text-xs text-amber-500" : "text-xs text-emerald-500"}>
          {props.state.readOnly ? "READ ONLY - ORDERS BLOCKED" : "TRADING POLICY ACTIVE"}
        </Text>
      </View>
    </View>
  );
}

export default function Dashboard() {
  const [page, setPage] = createSignal(0);
  const [state] = createSignal(INITIAL);

  onButtonPress(BTN.LEFT, () => setPage(0));
  onButtonPress(BTN.RIGHT, () => setPage(1));

  return (
    <View class="flex-col w-full h-full p-3 gap-2 bg-gradient-to-b from-slate-50 to-slate-100">
      <View class="flex-row items-center justify-between">
        <View class="flex-row items-center gap-2">
          <View class="w-3 h-3 rounded-full bg-emerald-500" />
          <View class="flex-col">
            <Text class="text-lg text-slate-950 font-bold">Pocket Pi</Text>
            <Text class="text-xs text-slate-500 tracking-wide">ESP32-P4 AGENT</Text>
          </View>
        </View>
        <View class="flex-row gap-2">
          <Status name="WIFI" state={state().wifi} />
          <Status name="CODEX" state={state().codex} />
          <Status name="RH" state={state().robinhood} />
        </View>
      </View>

      <Show when={page() === 0}>
        <Overview state={state()} />
      </Show>
      <Show when={page() === 1}>
        <Systems state={state()} />
      </Show>

      <View class="flex-row justify-between">
        <Text class="text-xs text-slate-400">LEFT OVERVIEW  RIGHT SYSTEMS</Text>
        <Text class={state().readOnly ? "text-xs text-amber-600 font-bold" : "text-xs text-emerald-600 font-bold"}>
          {state().readOnly ? "READ ONLY" : "LIVE"}
        </Text>
      </View>
    </View>
  );
}
