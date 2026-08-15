import { batch, createMemo, createSignal, For, Show } from "solid-js";
import { Text, View } from "@pocketjs/framework/components";
import { mount } from "@pocketjs/framework";
import { ActionButton, EmptyState, MetricCard, PocketHeader, ScrollButtons, SectionHeading, statusBadge, StatusBar } from "../_shared/ui";

const DB_SCHEMA_VERSION = 5;

type Screen = "dashboard" | "accounts" | "activity" | "positions";
type Span = "day" | "week";
type Account = { number: string; label: string; suffix: string; status: string };
type Position = { symbol: string; quantity: string; averagePrice: string; marketValue: string };
type Activity = { title: string; timestamp: string; detail: string; state: string; amount: string; side: string };
type Dashboard = {
  account: Account;
  totalValue: string | null;
  cash: string | null;
  buyingPower: string | null;
  pnlDay: string | null;
  pnlWeek: string | null;
  positions: Position[];
  activity: Activity[];
  positionsAvailable: boolean;
  activityAvailable: boolean;
  observedAt: number | null;
};
type ChartPoint = { x: number; y: number };
type ChartSegment = { x: number; y: number; width: number; angle: number };
type ChartProjection = { points: ChartPoint[]; segments: ChartSegment[]; labels: string[]; trend: { change: string; percent: string; positive: boolean } };
type AccountRow = { account_number: string; label: string; suffix: string; status: string };
type PortfolioRow = { account_number: string; cash: string | null; buying_power: string | null; day_pnl: string | null; week_pnl: string | null; observed_at: number };
type PositionRow = { account_number: string; symbol: string; quantity: string | null; average_price: string | null; market_value: string | null };
type ActivityRow = { account_number: string; activity_id: string; occurred_at: string | null; symbol: string | null; side: string | null; quantity: string | null; price: string | null; amount: string | null; state: string | null; activity_type: string | null };

const EMPTY_ACCOUNT: Account = { number: "", label: "ACCOUNT", suffix: "", status: "WAITING FOR ROBINHOOD" };
const EMPTY_ACTIVITY: Activity = { title: "NO RECENT ACTIVITY", timestamp: "", detail: "", state: "", amount: "", side: "" };
const UNAVAILABLE_ACTIVITY: Activity = { ...EMPTY_ACTIVITY, title: "ACTIVITY UNAVAILABLE" };
const BLANK_ACTIVITY: Activity = { ...EMPTY_ACTIVITY, title: "" };
const EMPTY_POSITION: Position = { symbol: "NO OPEN POSITIONS", quantity: "", averagePrice: "", marketValue: "" };
const UNAVAILABLE_POSITION: Position = { ...EMPTY_POSITION, symbol: "POSITIONS UNAVAILABLE" };
const BLANK_POSITION: Position = { ...EMPTY_POSITION, symbol: "" };
const EMPTY: Dashboard = {
  account: EMPTY_ACCOUNT, totalValue: null, cash: null, buyingPower: null,
  pnlDay: null, pnlWeek: null, positions: [], activity: [],
  positionsAvailable: true, activityAvailable: true, observedAt: null,
};

const [screen, setScreen] = createSignal<Screen>("dashboard");
const [span, setSpan] = createSignal<Span>("day");
const [accounts, setAccounts] = createSignal<Account[]>([]);
const [selectedAccount, setSelectedAccount] = createSignal("");
const [dashboard, setDashboard] = createSignal<Dashboard>(EMPTY);
const [chartPoints, setChartPoints] = createSignal<ChartPoint[]>([]);
const [chartSegments, setChartSegments] = createSignal<ChartSegment[]>([]);
const [chartLabels, setChartLabels] = createSignal<string[]>([]);
const [chartTrend, setChartTrend] = createSignal({ change: "$—", percent: "—", positive: true });
const [accountScroll, setAccountScroll] = createSignal(0);
const [activityScroll, setActivityScroll] = createSignal(0);
const [positionScroll, setPositionScroll] = createSignal(0);
const [status, setStatus] = createSignal("WAITING FOR ROBINHOOD");
const [refreshing, setRefreshing] = createSignal(false);
let schemaVersion = 0;
let accountRows: AccountRow[] = [];
let portfolioRows: PortfolioRow[] = [];
let totalRows: Array<{ account_number: string; value: string; observed_at: number }> = [];
let positionRows: PositionRow[] = [];
let activityRows: ActivityRow[] = [];
let latestRun: { status?: string; error?: string | null; completed_at?: number | null } | null = null;
const dashboardCache = new Map<string, Dashboard>();
function now(): number { return Math.floor(Date.now() / 1000); }

function rebuildDashboardCache(): void {
  const accountValues = accountRows.map((row) => ({ number: row.account_number, label: row.label, suffix: row.suffix, status: row.status }));
  const portfolioByAccount = new Map(portfolioRows.map((row) => [row.account_number, row]));
  const totalByAccount = new Map(totalRows.map((row) => [row.account_number, row]));
  const positionsByAccount = new Map<string, PositionRow[]>();
  const activitiesByAccount = new Map<string, ActivityRow[]>();
  for (const row of positionRows) {
    const rows = positionsByAccount.get(row.account_number) || [];
    if (rows.length < 64) rows.push(row);
    positionsByAccount.set(row.account_number, rows);
  }
  for (const row of activityRows) {
    const rows = activitiesByAccount.get(row.account_number) || [];
    if (rows.length < 64) rows.push(row);
    activitiesByAccount.set(row.account_number, rows);
  }
  dashboardCache.clear();
  for (const account of accountValues) {
    const portfolio = portfolioByAccount.get(account.number);
    const total = totalByAccount.get(account.number);
    const accountPositions = positionsByAccount.get(account.number) || [];
    const accountActivities = activitiesByAccount.get(account.number) || [];
    dashboardCache.set(account.number, {
      account,
      totalValue: total?.value ?? null,
      cash: portfolio?.cash ?? null,
      buyingPower: portfolio?.buying_power ?? null,
      pnlDay: portfolio?.day_pnl ?? null,
      pnlWeek: portfolio?.week_pnl ?? null,
      positions: accountPositions.map((row) => ({ symbol: row.symbol, quantity: row.quantity || "—", averagePrice: row.average_price || "—", marketValue: row.market_value || "" })),
      activity: accountActivities.map((row) => ({
        title: (row.side ? row.side + " " : "") + (row.symbol || "ORDER"),
        timestamp: formatTime(row.occurred_at),
        detail: row.quantity ? row.quantity + " SH · " + (row.activity_type || "ORDER") : row.activity_type || "ORDER",
        state: row.state || "RECENT",
        amount: row.amount || row.price || row.quantity || "",
        side: row.side || "",
      })),
      positionsAvailable: portfolio !== undefined,
      activityAvailable: portfolio !== undefined,
      observedAt: Math.max(portfolio?.observed_at ?? 0, total?.observed_at ?? 0) || null,
    });
  }
  setAccounts(accountValues);
  let accountNumber = selectedAccount();
  if (!accountValues.some((item) => item.number === accountNumber)) {
    accountNumber = accountValues[0]?.number || "";
    setSelectedAccount(accountNumber);
  }
  loadAccountProjection(accountNumber);
}

function number(value: string | null): number | null {
  if (value === null) return null;
  const parsed = Number(value.replace(/[$,%]/g, ""));
  return Number.isFinite(parsed) ? parsed : null;
}

function formatTime(value: string | null): string {
  if (!value) return "RECENT";
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return value.slice(0, 16).toUpperCase();
  return String(date.getMonth() + 1).padStart(2, "0") + "/" + String(date.getDate()).padStart(2, "0") + " " + String(date.getHours()).padStart(2, "0") + ":" + String(date.getMinutes()).padStart(2, "0");
}


function relativeTime(seconds: number): string {
  const age = Math.max(0, now() - seconds);
  if (age < 60) return "JUST NOW";
  if (age < 3600) return Math.floor(age / 60) + " MIN AGO";
  if (age < 86400) return Math.floor(age / 3600) + " HR AGO";
  return Math.floor(age / 86400) + " DAY AGO";
}

function money(value: string | null | undefined): string {
  if (!value) return "$—";
  const parsed = number(value);
  if (parsed === null) return value;
  const absolute = Math.abs(parsed);
  const parts = absolute.toFixed(2).split(".");
  const formatted = "$" + parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ",") + "." + parts[1];
  return parsed < 0 ? "-" + formatted : formatted;
}

function applyChart(rows: Array<{ bucket_index: number; bucket_time: number; total_value: string | null }>) {
  const buckets = rows.map((row) => ({ time: row.bucket_time, value: number(row.total_value) }));
  const bucketTimes = buckets.map((bucket) => bucket.time);
  const values = buckets.map((bucket) => bucket.value).filter((value): value is number => value !== null);
  const labels = [bucketTimes[0], bucketTimes[9], bucketTimes[19]].map((time) => {
    const date = new Date(time * 1000);
    return span() === "day"
      ? String(date.getHours()).padStart(2, "0") + ":" + String(date.getMinutes()).padStart(2, "0")
      : String(date.getMonth() + 1) + "/" + String(date.getDate());
  });
  if (values.length === 0) {
    const pnl = span() === "day" ? dashboard().pnlDay : dashboard().pnlWeek;
    const value = number(pnl);
    const projection = { points: [], segments: [], labels, trend: { change: money(pnl), percent: "—", positive: value === null || value >= 0 } };
    setChartPoints(projection.points);
    setChartSegments(projection.segments);
    setChartLabels(projection.labels);
    setChartTrend(projection.trend);
    return;
  }
  const low = Math.min(...values);
  const high = Math.max(...values);
  const range = Math.max(0.01, high - low);
  const points = buckets.flatMap((bucket, index) => bucket.value === null ? [] : [{
    x: index * 622 / 19,
    y: values.length === 1 ? 80 : 10 + (high - bucket.value) * 140 / range,
  }]);
  const segments = points.slice(1).map((point, index) => {
    const previous = points[index];
    const dx = point.x - previous.x;
    const dy = point.y - previous.y;
    return { x: previous.x, y: previous.y, width: Math.sqrt(dx * dx + dy * dy), angle: Math.atan2(dy, dx) * 180 / Math.PI };
  });
  const delta = values[values.length - 1] - values[0];
  const trend = { change: money(String(delta)), percent: values[0] === 0 ? "—" : (delta * 100 / values[0]).toFixed(2) + "%", positive: delta >= 0 };
  setChartPoints(points);
  setChartSegments(segments);
  setChartLabels(labels);
  setChartTrend(trend);
}

function loadRefreshProjection(): void {
  setRefreshing(false);
  if (latestRun?.status === "failed") setStatus("REFRESH FAILED · " + String(latestRun.error || "UNKNOWN ERROR").slice(0, 52));
  else if (latestRun?.status === "partial") setStatus("LIVE WITH PARTIAL DATA");
  else if (dashboard().observedAt) setStatus("LIVE · " + relativeTime(dashboard().observedAt as number));
  else setStatus("WAITING FOR ROBINHOOD");
}

function loadAccountProjection(accountNumber: string): void {
  if (!accountNumber) {
    setDashboard(EMPTY);
    setChartPoints([]);
    setChartSegments([]);
    if (!refreshing()) setStatus("WAITING FOR ROBINHOOD");
    return;
  }
  const cached = dashboardCache.get(accountNumber);
  if (cached) {
    setDashboard(cached);
    if (cached.observedAt && !refreshing()) setStatus("LIVE · " + relativeTime(cached.observedAt));
    return;
  }
  setDashboard(EMPTY);
  setChartPoints([]);
  setChartSegments([]);
}

PocketPi.projection.one("PRAGMA user_version", {}, (schema: { user_version?: number } | null) => {
  schemaVersion = Number(schema?.user_version ?? 0);
});
PocketPi.projection.many(
  "SELECT account_number,label,suffix,status FROM accounts ORDER BY label,account_number LIMIT 16",
  {},
  (rows: AccountRow[]) => { accountRows = rows; },
);
PocketPi.projection.many(
  "SELECT account_number,cash,buying_power,day_pnl,week_pnl,observed_at FROM portfolio_current LIMIT 16",
  {},
  (rows: PortfolioRow[]) => { portfolioRows = rows; },
);
PocketPi.projection.many(
  `SELECT value.account_number,value.value,value.observed_at
   FROM total_value value
   JOIN (SELECT account_number,MAX(observed_at) AS observed_at FROM total_value GROUP BY account_number) latest
     ON latest.account_number=value.account_number AND latest.observed_at=value.observed_at
   LIMIT 16`,
  {},
  (rows: Array<{ account_number: string; value: string; observed_at: number }>) => { totalRows = rows; },
);
PocketPi.projection.many(
  "SELECT account_number,symbol,quantity,average_price,market_value FROM positions ORDER BY account_number,CAST(market_value AS REAL) DESC,symbol LIMIT 1024",
  {},
  (rows: PositionRow[]) => { positionRows = rows; },
);
PocketPi.projection.many(
  "SELECT account_number,activity_id,occurred_at,symbol,side,quantity,price,amount,state,activity_type FROM activities ORDER BY account_number,occurred_at DESC,observed_at DESC LIMIT 1024",
  {},
  (rows: ActivityRow[]) => {
    activityRows = rows;
    if (schemaVersion === DB_SCHEMA_VERSION) rebuildDashboardCache();
    else {
      setAccounts([]);
      setSelectedAccount("");
      loadAccountProjection("");
    }
  },
);
PocketPi.projection.one(
  "SELECT status,error,completed_at FROM refresh_runs ORDER BY id DESC LIMIT 1",
  {},
  (row: typeof latestRun) => {
    latestRun = row;
    setRefreshing(false);
    loadRefreshProjection();
  },
);
const chartProjection = PocketPi.projection.many(
  `WITH RECURSIVE buckets(bucket_index, bucket_time) AS (
     SELECT 0, $cutoff
     UNION ALL
     SELECT bucket_index + 1, $cutoff + CAST(($window * (bucket_index + 1)) / 19 AS INTEGER)
     FROM buckets WHERE bucket_index < 19
   )
   SELECT bucket_index,bucket_time,
     (SELECT value FROM total_value
      WHERE account_number=$account AND observed_at >= $cutoff AND observed_at <= bucket_time
      ORDER BY observed_at DESC LIMIT 1) AS total_value
   FROM buckets ORDER BY bucket_index`,
  () => {
    const window = span() === "day" ? 86400 : 7 * 86400;
    return { "$cutoff": now() - window, "$window": window, "$account": selectedAccount() };
  },
  applyChart,
);

function tick(): string {
  return "";
}

function trend(): { change: string; percent: string; positive: boolean } {
  return chartTrend();
}

function Header(props: { title: string; metaBottom?: string }) {
  return (
    <PocketHeader title={props.title} back metaTop="AGENTIC" metaBottom={props.metaBottom ?? "AUTO · 5 MIN"} />
  );
}

function Metric(props: { label: string; value: string | null }) {
  return <View class="w-[210] h-[104]"><MetricCard label={props.label} value={money(props.value)} /></View>;
}

function SectionTitle(props: { title: string; detail?: string }) {
  return <SectionHeading title={props.title} detail={props.detail} action />;
}

function activityPreview(index: number): Activity {
  if (index === 0 && !dashboard().activityAvailable) return UNAVAILABLE_ACTIVITY;
  return dashboard().activity[index] ?? (index === 0 ? EMPTY_ACTIVITY : BLANK_ACTIVITY);
}

function CompactActivity(props: { index: number }) {
  const item = () => activityPreview(props.index);
  return <View class="h-[72] px-5 flex-row items-center justify-between"><View class="w-[450] flex-col gap-2"><Text class="text-lg text-slate-900 font-bold">{item().title}</Text><Text class="text-base text-slate-500">{item().timestamp ? item().timestamp + "  ·  " + item().detail : ""}</Text></View><Text class={item().side === "SELL" ? "text-base text-emerald-600 font-bold" : "text-base text-slate-900 font-bold"}>{item().amount ? money(item().amount) : ""}</Text></View>;
}

function positionPreview(index: number): Position {
  if (index === 0 && !dashboard().positionsAvailable) return UNAVAILABLE_POSITION;
  return dashboard().positions[index] ?? (index === 0 ? EMPTY_POSITION : BLANK_POSITION);
}

function CompactPosition(props: { index: number }) {
  const item = () => positionPreview(props.index);
  return <View class="h-[64] px-5 flex-row items-center"><Text class="w-[240] text-lg text-slate-900 font-bold">{item().symbol}</Text><Text class="w-[156] text-base text-slate-500">{item().quantity ? item().quantity + " SH" : ""}</Text><Text class="text-base text-slate-500">{item().averagePrice ? "AVG " + money(item().averagePrice) : ""}</Text></View>;
}

function Chart() {
  return (
    <View class="relative w-[632] h-[196] flex-col">
      <View class="relative w-[632] h-[160] overflow-hidden">
        <View class="absolute w-[632] h-[2] bg-slate-200" style={{ posType: 1, insetL: 0, insetT: 158 }} />
        <Text class="absolute text-base text-slate-500 font-bold" style={{ posType: 1, insetL: 152, insetT: 72 }}>{chartPoints().length < 2 ? "COLLECTING 5M VALUE HISTORY" : ""}</Text>
        <For each={chartSegments()}>{(item) => <View class={trend().positive ? "absolute rounded-lg bg-emerald-500" : "absolute rounded-lg bg-red-500"} style={{ posType: 1, insetL: item.x, insetT: item.y - 1, width: item.width, height: 2, rotate: item.angle, originX: -0.5, originY: 0 }} />}</For>
        <For each={chartPoints()}>{(item) => <View class={trend().positive ? "absolute w-[6] h-[6] rounded-lg bg-emerald-500" : "absolute w-[6] h-[6] rounded-lg bg-red-500"} style={{ posType: 1, insetL: item.x - 3, insetT: item.y - 3 }} />}</For>
      </View>
      <View class="h-[36] px-1 flex-row items-center justify-between"><Text class="text-base text-slate-500">{chartLabels()[0] || ""}</Text><Text class="text-base text-slate-500">{chartLabels()[1] || ""}</Text><Text class="text-base text-slate-500">{chartLabels()[2] || ""}</Text></View>
    </View>
  );
}

function DashboardScreen() {
  const currentTrend = () => trend();
  return (
    <View class="flex-col w-full h-full bg-slate-50">
      <Header title="ROBINHOOD" />
      <View class="h-[64] px-6 pt-2"><View class="w-full h-[56] px-5 flex-row items-center justify-between rounded-xl shadow bg-white border-slate-100">
          <Text class="text-base text-slate-500 font-bold">ACCOUNT</Text>
          <Text class="text-base text-slate-900 font-bold">{dashboard().account.label + (dashboard().account.suffix ? "  ····" + dashboard().account.suffix + "  " + (accounts().findIndex((item) => item.number === selectedAccount()) + 1) + "/" + accounts().length : "") + "   ›"}</Text>
      </View></View>
      <View class="h-[304] px-6 pt-3"><View class="h-[291] px-5 pt-4 flex-col rounded-xl shadow bg-white border-slate-100">
          <View class="h-[64] flex-row items-end justify-between">
            <Text class="text-2xl text-slate-950 font-bold">{money(dashboard().totalValue)}</Text>
            <Text class={currentTrend().positive ? "text-lg text-emerald-600 font-bold" : "text-lg text-red-500 font-bold"}>{currentTrend().change + "  (" + currentTrend().percent + ")"}</Text>
          </View>
          <Chart />
      </View></View>
      <View class="h-[60] px-6 pt-2 flex-row gap-3"><View class={span() === "day" ? "w-[100] h-[44] items-center justify-center rounded-lg bg-orange-600" : "w-[100] h-[44] items-center justify-center rounded-lg bg-white"}><Text class={span() === "day" ? "text-base text-white font-bold" : "text-base text-slate-500 font-bold"}>1D</Text></View><View class={span() === "week" ? "w-[100] h-[44] items-center justify-center rounded-lg bg-orange-600" : "w-[100] h-[44] items-center justify-center rounded-lg bg-white"}><Text class={span() === "week" ? "text-base text-white font-bold" : "text-base text-slate-500 font-bold"}>1W</Text></View><Text class="pt-3 text-base text-slate-500">{span() === "day" ? "TODAY" : "PAST WEEK"}</Text></View>
      <View class="h-[126] px-6 pt-3 flex-row items-start gap-[21]"><Metric label="VALUE" value={dashboard().totalValue} /><Metric label="CASH" value={dashboard().cash} /><Metric label="BUY POWER" value={dashboard().buyingPower} /></View>
      <View class="h-[200] px-6 flex-col"><SectionTitle title="ACTIVITY" detail="LAST 7 DAYS" /><View class="h-[150] flex-col rounded-xl shadow bg-white border-slate-100"><CompactActivity index={0} /><CompactActivity index={1} /></View></View>
      <View class="h-[184] px-6 flex-col"><SectionTitle title="POSITIONS" /><View class="h-[136] flex-col rounded-xl shadow bg-white border-slate-100"><CompactPosition index={0} /><CompactPosition index={1} /></View></View>
      <View class="h-[126] px-6 pt-2"><View class="w-full h-[110] px-5 flex-row items-center justify-between rounded-xl shadow bg-white border-slate-100"><View class="flex-col gap-3"><Text class="text-xl text-slate-900 font-bold">{"REALIZED P&L"}</Text><Text class="text-base text-slate-500">{"EQUITIES / " + (span() === "day" ? "TODAY" : "WEEK")}</Text></View><Text class={(number(span() === "day" ? dashboard().pnlDay : dashboard().pnlWeek) ?? 0) >= 0 ? "text-2xl text-emerald-600 font-bold" : "text-2xl text-red-500 font-bold"}>{money(span() === "day" ? dashboard().pnlDay : dashboard().pnlWeek)}</Text></View></View>
      <View class="h-[104] px-6 flex-row items-center justify-between"><View class="w-[460] h-[64]"><StatusBar text={status()} tone={status().startsWith("REFRESH FAILED") ? "danger" : "neutral"} /></View><View class="w-[176] h-[64]"><ActionButton label={refreshing() ? "REFRESHING" : "REFRESH NOW"} disabled={refreshing()} /></View></View>
    </View>
  );
}

function SideButtons() {
  return <ScrollButtons
    top="absolute left-[628] top-[156] w-[68] h-[132] items-center justify-center rounded-xl bg-orange-100"
    bottom="absolute left-[628] top-[972] w-[68] h-[132] items-center justify-center rounded-xl bg-orange-100"
  />;
}

function AccountsScreen() {
  const visible = createMemo(() => accounts().slice(accountScroll(), accountScroll() + 8));
  const selectedAtOpen = selectedAccount();
  return (
    <View class="relative flex-col w-full h-full bg-slate-50">
      <Header title="ACCOUNTS" />
      <View class="px-6 pt-[14] flex-col gap-[12]"><For each={visible()}>{(account) => (
        <View class={account.number === selectedAtOpen ? "w-[584] h-[100] px-5 flex-col justify-center gap-3 rounded-xl shadow bg-emerald-100 border-emerald-500" : "w-[584] h-[100] px-5 flex-col justify-center gap-3 rounded-xl shadow bg-white border-slate-100"}>
            <View class="flex-row items-center justify-between"><Text class="text-xl text-slate-900 font-bold">{account.label}</Text><Text class="text-lg text-slate-500 font-bold">{"····" + account.suffix}</Text></View>
            <View class="flex-row items-center justify-between"><View class={statusBadge.success.surface}><Text class={statusBadge.success.text}>{account.status}</Text></View><Show when={account.number === selectedAtOpen}><Text class="text-base text-emerald-600 font-bold">SELECTED</Text></Show></View>
        </View>
      )}</For></View>
      <SideButtons />
    </View>
  );
}

function ActivityScreen() {
  const visible = createMemo(() => dashboard().activity.slice(activityScroll(), activityScroll() + 8));
  return (
    <View class="relative flex-col w-full h-full bg-slate-50">
      <Header title="ACTIVITY" metaBottom="LAST 7 DAYS" />
      <View class="px-6 pt-[14] flex-col gap-[14]"><Show when={dashboard().activityAvailable && visible().length > 0} fallback={
        <EmptyState title={dashboard().activityAvailable ? "NO ACTIVITY YET" : "ACTIVITY UNAVAILABLE"} compact />
      }><For each={visible()}>{(item) => (
        <View class="w-[584] h-[112] px-5 flex-col justify-center gap-2 rounded-xl shadow bg-white border-slate-100">
          <View class="flex-row items-center justify-between"><Text class="text-lg text-slate-900 font-bold">{item.title}</Text><Text class={item.side === "SELL" ? "text-lg text-emerald-600 font-bold" : "text-lg text-slate-900 font-bold"}>{money(item.amount)}</Text></View>
          <Text class="text-base text-slate-500">{item.timestamp + "  ·  " + item.detail}</Text>
          <Text class="text-base text-emerald-600 font-bold">{item.state}</Text>
        </View>
      )}</For></Show></View>
      <SideButtons />
    </View>
  );
}

function PositionsScreen() {
  const visible = createMemo(() => dashboard().positions.slice(positionScroll(), positionScroll() + 9));
  return (
    <View class="relative flex-col w-full h-full bg-slate-50">
      <Header title="POSITIONS" />
      <View class="px-6 pt-[14] flex-col gap-[12]"><Show when={dashboard().positionsAvailable && visible().length > 0} fallback={
        <EmptyState title={dashboard().positionsAvailable ? "NO OPEN POSITIONS" : "POSITIONS UNAVAILABLE"} compact />
      }><For each={visible()}>{(item) => (
        <View class="w-[584] h-[98] px-5 flex-col justify-center gap-3 rounded-xl shadow bg-white border-slate-100">
          <View class="flex-row items-center"><Text class="w-[170] text-xl text-slate-900 font-bold">{item.symbol}</Text><Text class="text-lg text-slate-900 font-bold">{item.quantity + " SH"}</Text></View>
          <Text class="text-base text-slate-500">{"AVERAGE COST  " + money(item.averagePrice) + (item.marketValue ? "  ·  VALUE " + money(item.marketValue) : "")}</Text>
        </View>
      )}</For></Show></View>
      <SideButtons />
    </View>
  );
}

function SubScreen() {
  if (screen() === "accounts") return <AccountsScreen />;
  if (screen() === "activity") return <ActivityScreen />;
  return <PositionsScreen />;
}

function Robinhood() {
  return <Show when={screen() === "dashboard"} fallback={<SubScreen />}><DashboardScreen /></Show>;
}

mount(() => <Robinhood />);

PocketPi.defineView({
  tick,
  tap(x: number, y: number) {
    if (screen() !== "dashboard") {
      if (y < 112 && x < 220) { setScreen("dashboard"); return ""; }
      if (x >= 620 && y >= 140 && y < 310) {
        if (screen() === "accounts") setAccountScroll((value) => Math.max(0, value - 1));
        if (screen() === "activity") setActivityScroll((value) => Math.max(0, value - 1));
        if (screen() === "positions") setPositionScroll((value) => Math.max(0, value - 1));
        return "";
      }
      if (x >= 620 && y >= 940 && y < 1130) {
        if (screen() === "accounts") setAccountScroll((value) => Math.min(Math.max(0, accounts().length - 8), value + 1));
        if (screen() === "activity") setActivityScroll((value) => Math.min(Math.max(0, dashboard().activity.length - 8), value + 1));
        if (screen() === "positions") setPositionScroll((value) => Math.min(Math.max(0, dashboard().positions.length - 9), value + 1));
        return "";
      }
      if (screen() === "accounts" && x < 610 && y >= 126) {
        const index = accountScroll() + Math.floor((y - 126) / 112);
        const account = accounts()[index];
        if (account) {
          batch(() => {
            setSelectedAccount(account.number);
            setScreen("dashboard");
          });
          loadAccountProjection(account.number);
          chartProjection.refresh();
        }
      }
      return "";
    }
    if (y < 112 && x < 100) return PocketPi.navigate("pi-agent");
    if (y >= 112 && y < 176) { setScreen("accounts"); return ""; }
    if (y >= 480 && y < 540) { batch(() => { setSpan(x < 136 ? "day" : "week"); chartProjection.refresh(); }); return ""; }
    if (y >= 666 && y < 866) { setScreen("activity"); return ""; }
    if (y >= 866 && y < 1050) { setScreen("positions"); return ""; }
    if (y >= 1176 && x >= 500) {
      if (refreshing()) return "";
      setRefreshing(true);
      setStatus("REFRESHING ROBINHOOD…");
      return PocketPi.action("refreshPortfolio");
    }
    return "";
  },
});
