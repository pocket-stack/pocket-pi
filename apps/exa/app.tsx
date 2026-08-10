import { createSignal, For, Show } from "solid-js";
import { Text, View } from "@pocketjs/framework/components";
import { mount } from "@pocketjs/framework";
import { Database } from "@pocketjs/framework/db";

const DB_SCHEMA_VERSION = 4;
const RETENTION_DAYS = 7;
const db = new Database("exa");

type SearchRow = { id: number; query: string; searched_at: number; status: string; result_count: number; top_title: string | null; top_url: string | null; error: string | null };
const [history, setHistory] = createSignal<SearchRow[]>([]);
const [status, setStatus] = createSignal("SEARCH HISTORY IS LOCAL");
let loadedRevision = -1;

function parse(value: string): any { try { return JSON.parse(value); } catch { return null; } }

function searchTime(seconds: number): string {
  const value = new Date(seconds * 1000).toISOString();
  return value.slice(0, 10) + "  ·  " + value.slice(11, 16) + " UTC";
}

function loadView(revision: number) {
  if (loadedRevision === revision) return;
  try {
    const schema = db.query("PRAGMA user_version").get() as unknown as { user_version?: number } | null;
    if (Number(schema?.user_version ?? 0) !== DB_SCHEMA_VERSION) {
      setHistory([]);
      setStatus("SEARCH HISTORY IS LOCAL");
      return;
    }
    const next = db.query(`
      SELECT search.id,search.query,search.searched_at,search.status,search.result_count,
             result.title AS top_title,result.url AS top_url,search.error
      FROM searches search
      LEFT JOIN search_results result ON result.search_id=search.id AND result.rank=0
      ORDER BY search.id DESC LIMIT 8
    `).all() as unknown as SearchRow[];
    setHistory(next);
    setStatus(next[0]?.status === "error" ? String(next[0].error || "EXA SEARCH FAILED").slice(0, 80)
      : next.length ? "SEARCH HISTORY UPDATED FROM SQLITE" : "SEARCH HISTORY IS LOCAL");
    loadedRevision = revision;
  } catch {
    setHistory([]);
    setStatus("SEARCH HISTORY IS LOCAL");
  }
}

function storageStatus(): { text: string; details: any } {
  const tableRows = db.query("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").all() as unknown as Array<{ name: string }>;
  const names = new Set(tableRows.map((table) => table.name));
  const count = (table: string): number => names.has(table)
    ? Number((db.query(`SELECT COUNT(*) AS count FROM ${table}`).get() as unknown as { count?: number } | null)?.count ?? 0)
    : 0;
  const pageSize = Number((db.query("PRAGMA page_size").get() as unknown as { page_size?: number } | null)?.page_size ?? 0);
  const pageCount = Number((db.query("PRAGMA page_count").get() as unknown as { page_count?: number } | null)?.page_count ?? 0);
  const freelistCount = Number((db.query("PRAGMA freelist_count").get() as unknown as { freelist_count?: number } | null)?.freelist_count ?? 0);
  const schemaVersion = Number((db.query("PRAGMA user_version").get() as unknown as { user_version?: number } | null)?.user_version ?? 0);
  const latestSearch = names.has("searches") ? db.query(`
    SELECT id,searched_at,status,result_count,error
    FROM searches ORDER BY id DESC LIMIT 1
  `).get() : null;
  const tableCounts = {
    searches: count("searches"),
    searchResults: count("search_results"),
    documents: count("documents"),
  };
  const details = {
    database: "exa.sqlite",
    schemaVersion,
    expectedSchemaVersion: DB_SCHEMA_VERSION,
    retentionDays: RETENTION_DAYS,
    tableCounts,
    allocatedBytes: pageSize * pageCount,
    reusableBytes: pageSize * freelistCount,
    latestSearch,
  };
  const latest = latestSearch as null | { id?: number; status?: string; result_count?: number };
  const latestText = latest
    ? ` Latest #${latest.id ?? "?"}: ${latest.status ?? "unknown"}, ${latest.result_count ?? 0} results.`
    : " No searches yet.";
  return {
    text: `Exa storage (schema ${schemaVersion}/${DB_SCHEMA_VERSION}): ${tableCounts.searches} searches, ${tableCounts.searchResults} results, ${tableCounts.documents} documents; ${RETENTION_DAYS}-day retention; ${details.allocatedBytes} bytes allocated (${details.reusableBytes} reusable).${latestText}`,
    details,
  };
}

function Exa() {
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      <View class="h-[112] px-6 flex-row items-center justify-between bg-slate-950">
        <View class="flex-row items-center gap-4">
          <Text class="text-2xl text-white">‹</Text>
          <View class="flex-col"><Text class="text-sm text-indigo-300 tracking-wide">POCKET APP</Text><Text class="text-2xl text-white font-bold">Exa Research</Text></View>
        </View>
        <View class="px-3 py-2 bg-indigo-900"><Text class="text-sm text-indigo-200">SQLITE HISTORY</Text></View>
      </View>
      <View class="p-6 flex-col gap-3">
        <Text class="text-sm text-indigo-700 font-bold">AGENT RESEARCH MEMORY</Text>
        <Text class="text-2xl text-slate-950 font-bold">Search history</Text>
        <Text class="text-base text-slate-600">Every research.search call is saved here automatically.</Text>
      </View>
      <View class="grow px-6 pt-4 flex-col gap-3">
        <Show when={history().length === 0}>
          <View class="h-[430] px-12 flex-col items-center justify-center bg-white">
            <View class="w-[88] h-[88] items-center justify-center bg-indigo-100"><Text class="text-2xl text-indigo-700 font-bold">E</Text></View>
            <Text class="pt-7 text-xl text-slate-900 font-bold">No searches yet</Text>
            <Text class="pt-4 text-base text-slate-500">{"Ask Pi Agent to research a topic.\nThe search and its results will appear here."}</Text>
          </View>
        </Show>
        <For each={history()}>{(item) => (
          <View class="h-[118] px-5 flex-row items-center justify-between bg-white">
            <View class="w-[500] flex-col gap-2">
              <Text class="text-base text-slate-900 font-bold">{item.query.slice(0, 72)}</Text>
              <Text class="text-sm text-slate-500">{(item.top_title || item.error || "No result title").slice(0, 92)}</Text>
              <Text class="text-sm text-indigo-600 font-bold">{searchTime(item.searched_at)}</Text>
            </View>
            <View class={item.status === "ok" ? "px-3 py-2 bg-emerald-100" : "px-3 py-2 bg-red-100"}>
              <Text class={item.status === "ok" ? "text-sm text-emerald-700" : "text-sm text-red-500"}>{item.status === "ok" ? item.result_count + " RESULTS" : "FAILED"}</Text>
            </View>
          </View>
        )}</For>
      </View>
      <View class="h-[96] px-6 flex-row items-center bg-slate-950"><Text class="text-sm text-slate-300">{status()}</Text></View>
    </View>
  );
}

loadView(0);
mount(() => <Exa />);

(globalThis as any).PocketPiApp = {
  tick() { return ""; },
  update() { return ""; },
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
  invokeTask(name: string) { return JSON.stringify({ text: "Unknown Exa task: " + name, isError: true }); },
  tap(x: number, y: number) {
    if (y < 112 && x < 100) return JSON.stringify({ type: "navigate", app: "pi-agent" });
    return "";
  },
};
