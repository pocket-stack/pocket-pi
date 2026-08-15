import { createSignal, For, Show } from "solid-js";
import { Text, View } from "@pocketjs/framework/components";
import { mount } from "@pocketjs/framework";
import { EmptyState, PageIntro, PocketHeader, ScrollButtons, statusBadge, StatusBar } from "../_shared/ui";

const DB_SCHEMA_VERSION = 5;
const HISTORY_PAGE_SIZE = 10;
const HISTORY_MAX_ROWS = 50;
const HISTORY_VISIBLE_ROWS = 6;

type SearchRow = { id: number; query: string; searched_at: number; status: string; result_count: number; top_title: string | null; error: string | null };
const [history, setHistory] = createSignal<SearchRow[]>([]);
const [hasMore, setHasMore] = createSignal(false);
const [historyOffset, setHistoryOffset] = createSignal(0);
const [status, setStatus] = createSignal("SEARCH HISTORY IS LOCAL");
let schemaVersion = 0;
let historyLimit = HISTORY_PAGE_SIZE;

function searchTime(seconds: number): string {
  const value = new Date(seconds * 1000).toISOString();
  return value.slice(0, 10) + "  ·  " + value.slice(11, 16) + " UTC";
}

function loadMore() {
  if (!hasMore() || historyLimit >= HISTORY_MAX_ROWS) return;
  historyLimit = Math.min(HISTORY_MAX_ROWS, historyLimit + HISTORY_PAGE_SIZE);
  historyProjection.refresh();
}

function scrollHistory(direction: -1 | 1) {
  const step = 4;
  if (direction > 0 && historyOffset() + HISTORY_VISIBLE_ROWS + step > history().length && hasMore()) loadMore();
  setHistoryOffset((offset) => direction < 0
    ? Math.max(0, offset - step)
    : Math.min(Math.max(0, history().length - HISTORY_VISIBLE_ROWS), offset + step));
}

PocketPi.projection.one("PRAGMA user_version", {}, (schema: { user_version?: number } | null) => {
  schemaVersion = Number(schema?.user_version ?? 0);
});

const historyProjection = PocketPi.projection.many(
  `SELECT id,query,searched_at,status,result_count,top_title,error
   FROM searches ORDER BY id DESC LIMIT $limit`,
  () => ({ "$limit": historyLimit + 1 }),
  (rows: SearchRow[]) => {
    if (schemaVersion !== DB_SCHEMA_VERSION) {
      setHistory([]);
      setHasMore(false);
      setHistoryOffset(0);
      setStatus("SEARCH HISTORY IS LOCAL");
      return;
    }
    const next = rows.slice(0, historyLimit);
    setHistory(next);
    setHasMore(rows.length > historyLimit && historyLimit < HISTORY_MAX_ROWS);
    setHistoryOffset((offset) => Math.min(offset, Math.max(0, next.length - HISTORY_VISIBLE_ROWS)));
    setStatus(next[0]?.status === "error" ? String(next[0].error || "EXA SEARCH FAILED").slice(0, 80)
      : next.length ? "SEARCH HISTORY UPDATED FROM SQLITE" : "SEARCH HISTORY IS LOCAL");
  }
);

function Exa() {
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      <PocketHeader title="EXA RESEARCH" back metaTop="POCKET APP" metaBottom="SQLITE HISTORY" />
      <PageIntro
        eyebrow="AGENT RESEARCH MEMORY"
        title="Search history"
        description="Every research.search call is saved here automatically."
        tone="info"
      />
      <View class="relative grow px-6 pt-4 flex-col">
        <ScrollButtons
          top="absolute left-[628] top-[16] w-[68] h-[132] items-center justify-center rounded-xl bg-orange-100"
          bottom="absolute left-[628] top-[742] w-[68] h-[132] items-center justify-center rounded-xl bg-orange-100"
        />
        <View class="w-[584] h-[890] flex-col gap-[12]">
          <For each={history().slice(historyOffset(), historyOffset() + HISTORY_VISIBLE_ROWS)}>{(item) => (
            <View class="h-[126] px-5 flex-row items-center justify-between rounded-xl shadow bg-white border-slate-100">
              <View class="w-[390] flex-col gap-2">
                <Text class="text-lg text-slate-900 font-bold">{item.query.slice(0, 48)}</Text>
                <Text class="text-base text-slate-500">{(item.top_title || item.error || "No result title").slice(0, 58)}</Text>
                <Text class="text-base text-indigo-600 font-bold">{searchTime(item.searched_at)}</Text>
              </View>
              <View class={item.status === "ok" ? statusBadge.success.surface : statusBadge.danger.surface}>
                <Text class={item.status === "ok" ? statusBadge.success.text : statusBadge.danger.text}>{item.status === "ok" ? item.result_count + " RESULTS" : "FAILED"}</Text>
              </View>
            </View>
          )}</For>
        </View>
        <Show when={history().length === 0}>
          <View class="absolute left-[24] top-[16] w-[672] h-[890] bg-slate-50">
            <EmptyState
              icon="E"
              title="No searches yet"
              detail={"Ask Pi Agent to research a topic.\nThe search and its results will appear here."}
              tone="info"
            />
          </View>
        </Show>
      </View>
      <View class="h-[96]"><StatusBar text={status()} tone={status().includes("FAILED") ? "danger" : "neutral"} dark /></View>
    </View>
  );
}

mount(() => <Exa />);

PocketPi.defineView({
  tick() { return ""; },
  tap(x: number, y: number) {
    if (y < 112 && x < 100) return PocketPi.navigate("pi-agent");
    if (x >= 620 && y >= 294 && y <= 426) scrollHistory(-1);
    else if (x >= 620 && y >= 1020 && y <= 1152) scrollHistory(1);
    return "";
  },
});
