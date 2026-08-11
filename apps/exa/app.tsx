import { createSignal, Show } from "solid-js";
import { Text, View } from "@pocketjs/framework/components";
import { mount } from "@pocketjs/framework";
import { Database } from "@pocketjs/framework/db";
import { VirtualList } from "@pocketjs/framework/virtual-list";
import { EmptyState, PageIntro, PocketHeader, statusBadge, StatusBar } from "../_shared/ui";

const DB_SCHEMA_VERSION = 5;
const RETENTION_DAYS = 7;
const HISTORY_PAGE_SIZE = 10;
const HISTORY_ROW_HEIGHT = 138;
const HISTORY_VIEWPORT_HEIGHT = 890;
const db = new Database("exa");

type SearchRow = { id: number; query: string; searched_at: number; status: string; result_count: number; top_title: string | null; error: string | null };
const [history, setHistory] = createSignal<SearchRow[]>([]);
const [hasMore, setHasMore] = createSignal(false);
const [status, setStatus] = createSignal("SEARCH HISTORY IS LOCAL");
let loadedRevision = -1;
let loadingMore = false;

function parse(value: string): any { try { return JSON.parse(value); } catch { return null; } }

function searchTime(seconds: number): string {
  const value = new Date(seconds * 1000).toISOString();
  return value.slice(0, 10) + "  ·  " + value.slice(11, 16) + " UTC";
}

function historyPage(offset: number): SearchRow[] {
  return db.query(`
      SELECT id,query,searched_at,status,result_count,top_title,error
      FROM searches ORDER BY id DESC LIMIT ? OFFSET ?
    `).all(HISTORY_PAGE_SIZE + 1, offset) as unknown as SearchRow[];
}

function loadMore() {
  if (loadingMore || !hasMore()) return;
  loadingMore = true;
  try {
    const next = historyPage(history().length);
    setHistory((current) => [...current, ...next.slice(0, HISTORY_PAGE_SIZE)]);
    setHasMore(next.length > HISTORY_PAGE_SIZE);
  } finally {
    loadingMore = false;
  }
}

function loadView(revision: number) {
  if (loadedRevision === revision) return;
  try {
    const schema = db.query("PRAGMA user_version").get() as unknown as { user_version?: number } | null;
    if (Number(schema?.user_version ?? 0) !== DB_SCHEMA_VERSION) {
      setHistory([]);
      setHasMore(false);
      setStatus("SEARCH HISTORY IS LOCAL");
      return;
    }
    const page = historyPage(0);
    const next = page.slice(0, HISTORY_PAGE_SIZE);
    setHistory(next);
    setHasMore(page.length > HISTORY_PAGE_SIZE);
    setStatus(next[0]?.status === "error" ? String(next[0].error || "EXA SEARCH FAILED").slice(0, 80)
      : next.length ? "SEARCH HISTORY UPDATED FROM SQLITE" : "SEARCH HISTORY IS LOCAL");
    loadedRevision = revision;
  } catch {
    setHistory([]);
    setStatus("SEARCH HISTORY IS LOCAL");
  }
}

function storageStatus(): { text: string; details: any } {
  const pageSize = Number((db.query("PRAGMA page_size").get() as unknown as { page_size?: number } | null)?.page_size ?? 0);
  const pageCount = Number((db.query("PRAGMA page_count").get() as unknown as { page_count?: number } | null)?.page_count ?? 0);
  const freelistCount = Number((db.query("PRAGMA freelist_count").get() as unknown as { freelist_count?: number } | null)?.freelist_count ?? 0);
  const schemaVersion = Number((db.query("PRAGMA user_version").get() as unknown as { user_version?: number } | null)?.user_version ?? 0);
  const searches = schemaVersion === DB_SCHEMA_VERSION
    ? Number((db.query("SELECT COUNT(*) AS count FROM searches").get() as unknown as { count?: number } | null)?.count ?? 0)
    : 0;
  const latestSearch = schemaVersion === DB_SCHEMA_VERSION ? db.query(`
    SELECT id,searched_at,status,result_count,error
    FROM searches ORDER BY id DESC LIMIT 1
  `).get() : null;
  const details = {
    database: "exa.sqlite",
    schemaVersion,
    expectedSchemaVersion: DB_SCHEMA_VERSION,
    retentionDays: RETENTION_DAYS,
    searches,
    allocatedBytes: pageSize * pageCount,
    reusableBytes: pageSize * freelistCount,
    latestSearch,
  };
  const latest = latestSearch as null | { id?: number; status?: string; result_count?: number };
  const latestText = latest
    ? ` Latest #${latest.id ?? "?"}: ${latest.status ?? "unknown"}, ${latest.result_count ?? 0} results.`
    : " No searches yet.";
  return {
    text: `Exa storage (schema ${schemaVersion}/${DB_SCHEMA_VERSION}): ${searches} searches; ${RETENTION_DAYS}-day retention; ${details.allocatedBytes} bytes allocated (${details.reusableBytes} reusable).${latestText}`,
    details,
  };
}

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
      <View class="grow px-6 pt-4 flex-col">
        <Show when={history().length === 0}>
          <EmptyState
            icon="E"
            title="No searches yet"
            detail={"Ask Pi Agent to research a topic.\nThe search and its results will appear here."}
            tone="info"
          />
        </Show>
        <Show when={history().length > 0}>
          <VirtualList
            count={history().length}
            rowHeight={HISTORY_ROW_HEIGHT}
            height={HISTORY_VIEWPORT_HEIGHT}
            focusRows={false}
            onNearEnd={loadMore}
            renderRow={(index) => {
              const item = () => history()[index];
              return (
                <View class="h-[126] px-5 flex-row items-center justify-between rounded-xl shadow bg-white border-slate-100">
                  <View class="w-[474] flex-col gap-2">
                    <Text class="text-lg text-slate-900 font-bold">{item().query.slice(0, 60)}</Text>
                    <Text class="text-base text-slate-500">{(item().top_title || item().error || "No result title").slice(0, 76)}</Text>
                    <Text class="text-base text-indigo-600 font-bold">{searchTime(item().searched_at)}</Text>
                  </View>
                  <View class={item().status === "ok" ? statusBadge.success.surface : statusBadge.danger.surface}>
                    <Text class={item().status === "ok" ? statusBadge.success.text : statusBadge.danger.text}>{item().status === "ok" ? item().result_count + " RESULTS" : "FAILED"}</Text>
                  </View>
                </View>
              );
            }}
          />
        </Show>
      </View>
      <View class="h-[96]"><StatusBar text={status()} tone={status().includes("FAILED") ? "danger" : "neutral"} dark /></View>
    </View>
  );
}

loadView(0);
mount(() => <Exa />);

(globalThis as any).PocketPiApp = {
  tick() { return ""; },
  dataChanged(eventsLine: string) {
    const events = parse(eventsLine);
    const revision = Array.isArray(events)
      ? events.reduce((latest: number, event: any) => Math.max(latest, Number(event?.revision ?? 0)), loadedRevision)
      : loadedRevision;
    loadView(revision);
    return "";
  },
  invokeTool(name: string) {
    try {
      const value = name === "research.storage_status" ? storageStatus()
        : (() => { throw new Error("Data-writing tools run in the background App Data Action"); })();
      return JSON.stringify({ text: value.text, details: value.details, isError: false });
    } catch (error) {
      return JSON.stringify({ text: error instanceof Error ? error.message : String(error), isError: true });
    }
  },
  tap(x: number, y: number) {
    if (y < 112 && x < 100) return JSON.stringify({ type: "navigate", app: "pi-agent" });
    return "";
  },
};
