const EMPTY_ACCOUNT = { number: "", label: "ACCOUNT", suffix: "", status: "WAITING FOR ROBINHOOD" };
const EMPTY_ACTIVITY = { title: "NO RECENT ACTIVITY", timestamp: "", detail: "", state: "", amount: "", side: "" };
const UNAVAILABLE_ACTIVITY = { ...EMPTY_ACTIVITY, title: "ACTIVITY UNAVAILABLE" };
const BLANK_ACTIVITY = { ...EMPTY_ACTIVITY, title: "" };
const EMPTY_POSITION = { symbol: "NO OPEN POSITIONS", quantity: "", averagePrice: "", marketValue: "" };
const UNAVAILABLE_POSITION = { ...EMPTY_POSITION, symbol: "POSITIONS UNAVAILABLE" };
const BLANK_POSITION = { ...EMPTY_POSITION, symbol: "" };
const EMPTY_DASHBOARD = {
  account: EMPTY_ACCOUNT,
  totalValue: null,
  cash: null,
  buyingPower: null,
  pnlDay: null,
  pnlWeek: null,
  positions: [],
  activity: [],
  positionsAvailable: true,
  activityAvailable: true,
  observedAt: null,
};

const model = View.state({
  screen: "dashboard",
  span: "day",
  accounts: [],
  selectedAccount: "",
  dashboard: EMPTY_DASHBOARD,
  chartPoints: [],
  chartSegments: [],
  chartLabels: [],
  chartTrend: { change: "$—", percent: "—", positive: true },
  accountScroll: 0,
  activityScroll: 0,
  positionScroll: 0,
  status: "WAITING FOR ROBINHOOD",
  refreshing: false,
});

let accountRows = [];
let portfolioRows = [];
let totalRows = [];
let positionRows = [];
let activityRows = [];
let latestRun = null;
const dashboardCache = new Map();

function now() {
  return Math.floor(Date.now() / 1000);
}

function number(value) {
  if (value === null || value === undefined) return null;
  const parsed = Number(String(value).replace(/[$,%]/g, ""));
  return Number.isFinite(parsed) ? parsed : null;
}

function money(value) {
  if (!value) return "$—";
  const parsed = number(value);
  if (parsed === null) return String(value);
  const parts = Math.abs(parsed).toFixed(2).split(".");
  const formatted = "$" + parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ",") + "." + parts[1];
  return parsed < 0 ? "-" + formatted : formatted;
}

function formatTime(value) {
  if (!value) return "RECENT";
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return value.slice(0, 16).toUpperCase();
  return String(date.getMonth() + 1).padStart(2, "0") + "/"
    + String(date.getDate()).padStart(2, "0") + " "
    + String(date.getHours()).padStart(2, "0") + ":"
    + String(date.getMinutes()).padStart(2, "0");
}

function relativeTime(seconds) {
  const age = Math.max(0, now() - seconds);
  if (age < 60) return "JUST NOW";
  if (age < 3600) return Math.floor(age / 60) + " MIN AGO";
  if (age < 86400) return Math.floor(age / 3600) + " HR AGO";
  return Math.floor(age / 86400) + " DAY AGO";
}

function loadAccount(accountNumber) {
  if (!accountNumber) {
    model.update({ dashboard: EMPTY_DASHBOARD, chartPoints: [], chartSegments: [], status: "WAITING FOR ROBINHOOD" });
    return;
  }
  const dashboard = dashboardCache.get(accountNumber) ?? EMPTY_DASHBOARD;
  const status = dashboard.observedAt && !model.get().refreshing
    ? "LIVE · " + relativeTime(dashboard.observedAt)
    : model.get().status;
  model.update({ dashboard, status });
}

function rebuildDashboardCache() {
  const accounts = accountRows.map((row) => ({
    number: row.account_number,
    label: row.label,
    suffix: row.suffix,
    status: row.status,
  }));
  const portfolioByAccount = new Map(portfolioRows.map((row) => [row.account_number, row]));
  const totalByAccount = new Map(totalRows.map((row) => [row.account_number, row]));
  const positionsByAccount = new Map();
  const activitiesByAccount = new Map();
  for (const row of positionRows) {
    const rows = positionsByAccount.get(row.account_number) ?? [];
    if (rows.length < 64) rows.push(row);
    positionsByAccount.set(row.account_number, rows);
  }
  for (const row of activityRows) {
    const rows = activitiesByAccount.get(row.account_number) ?? [];
    if (rows.length < 64) rows.push(row);
    activitiesByAccount.set(row.account_number, rows);
  }
  dashboardCache.clear();
  for (const account of accounts) {
    const portfolio = portfolioByAccount.get(account.number);
    const total = totalByAccount.get(account.number);
    dashboardCache.set(account.number, {
      account,
      totalValue: total?.value ?? null,
      cash: portfolio?.cash ?? null,
      buyingPower: portfolio?.buying_power ?? null,
      pnlDay: portfolio?.day_pnl ?? null,
      pnlWeek: portfolio?.week_pnl ?? null,
      positions: (positionsByAccount.get(account.number) ?? []).map((row) => ({
        symbol: row.symbol,
        quantity: row.quantity || "—",
        averagePrice: row.average_price || "—",
        marketValue: row.market_value || "",
      })),
      activity: (activitiesByAccount.get(account.number) ?? []).map((row) => ({
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
  let selectedAccount = model.get().selectedAccount;
  if (!accounts.some((account) => account.number === selectedAccount)) {
    selectedAccount = accounts[0]?.number || "";
  }
  model.update({ accounts, selectedAccount });
  loadAccount(selectedAccount);
}

function applyChart(rows) {
  const state = model.get();
  const buckets = rows.map((row) => ({ time: row.bucket_time, value: number(row.total_value) }));
  const values = buckets.map((bucket) => bucket.value).filter((value) => value !== null);
  const labels = [buckets[0]?.time, buckets[9]?.time, buckets[19]?.time].map((time) => {
    if (!time) return "";
    const date = new Date(time * 1000);
    return state.span === "day"
      ? String(date.getHours()).padStart(2, "0") + ":" + String(date.getMinutes()).padStart(2, "0")
      : String(date.getMonth() + 1) + "/" + String(date.getDate());
  });
  if (values.length === 0) {
    const pnl = state.span === "day" ? state.dashboard.pnlDay : state.dashboard.pnlWeek;
    const value = number(pnl);
    model.update({
      chartPoints: [],
      chartSegments: [],
      chartLabels: labels,
      chartTrend: { change: money(pnl), percent: "—", positive: value === null || value >= 0 },
    });
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
  model.update({
    chartPoints: points,
    chartSegments: segments,
    chartLabels: labels,
    chartTrend: {
      change: money(String(delta)),
      percent: values[0] === 0 ? "—" : (delta * 100 / values[0]).toFixed(2) + "%",
      positive: delta >= 0,
    },
  });
}

function updateRefreshStatus() {
  const state = model.get();
  let status = "WAITING FOR ROBINHOOD";
  if (latestRun?.status === "failed") status = "REFRESH FAILED · " + String(latestRun.error || "UNKNOWN ERROR").slice(0, 52);
  else if (latestRun?.status === "partial") status = "LIVE WITH PARTIAL DATA";
  else if (state.dashboard.observedAt) status = "LIVE · " + relativeTime(state.dashboard.observedAt);
  model.update({ refreshing: false, status });
}

PocketPi.projection.many(
  "SELECT account_number,label,suffix,status FROM accounts ORDER BY label,account_number LIMIT 16",
  {},
  (rows) => { accountRows = rows; },
);
PocketPi.projection.many(
  "SELECT account_number,cash,buying_power,day_pnl,week_pnl,observed_at FROM portfolio_current LIMIT 16",
  {},
  (rows) => { portfolioRows = rows; },
);
PocketPi.projection.many(
  `SELECT value.account_number,value.value,value.observed_at
   FROM total_value value
   JOIN (SELECT account_number,MAX(observed_at) AS observed_at FROM total_value GROUP BY account_number) latest
     ON latest.account_number=value.account_number AND latest.observed_at=value.observed_at
   LIMIT 16`,
  {},
  (rows) => { totalRows = rows; },
);
PocketPi.projection.many(
  "SELECT account_number,symbol,quantity,average_price,market_value FROM positions ORDER BY account_number,CAST(market_value AS REAL) DESC,symbol LIMIT 1024",
  {},
  (rows) => { positionRows = rows; },
);
PocketPi.projection.many(
  "SELECT account_number,activity_id,occurred_at,symbol,side,quantity,price,amount,state,activity_type FROM activities ORDER BY account_number,occurred_at DESC,observed_at DESC LIMIT 1024",
  {},
  (rows) => { activityRows = rows; rebuildDashboardCache(); },
);
PocketPi.projection.one(
  "SELECT status,error,completed_at FROM refresh_runs ORDER BY id DESC LIMIT 1",
  {},
  (row) => { latestRun = row; updateRefreshStatus(); },
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
    const state = model.get();
    const window = state.span === "day" ? 86400 : 7 * 86400;
    return { "$cutoff": now() - window, "$window": window, "$account": state.selectedAccount };
  },
  applyChart,
);

function setSpan(span) {
  model.update({ span });
  chartProjection.refresh();
}

function selectAccount(account) {
  model.update({ selectedAccount: account.number, screen: "dashboard" });
  loadAccount(account.number);
  chartProjection.refresh();
}

function refreshPortfolio() {
  if (model.get().refreshing) return "";
  model.update({ refreshing: true, status: "REFRESHING ROBINHOOD…" });
  return PocketPi.action("refreshPortfolio");
}

function scroll(screen, direction) {
  const state = model.get();
  const config = {
    accounts: ["accountScroll", state.accounts.length, 8],
    activity: ["activityScroll", state.dashboard.activity.length, 8],
    positions: ["positionScroll", state.dashboard.positions.length, 9],
  }[screen];
  const [key, length, visible] = config;
  model.update({ [key]: Math.max(0, Math.min(Math.max(0, length - visible), state[key] + direction)) });
}

function header(title, metaBottom) {
  return View.Header({
    title,
    metaTop: "AGENTIC",
    metaBottom: metaBottom ?? "AUTO · 5 MIN",
    onBack: () => model.get().screen === "dashboard"
      ? PocketPi.navigate("pi-agent")
      : model.update({ screen: "dashboard" }),
  });
}

function metric(label, value) {
  return View.Box({ style: { grow: 1, basis: 0, height: "full" }, children: View.MetricCard({ label, value: money(value) }) });
}

function activityPreview(state, index) {
  if (index === 0 && !state.dashboard.activityAvailable) return UNAVAILABLE_ACTIVITY;
  return state.dashboard.activity[index] ?? (index === 0 ? EMPTY_ACTIVITY : BLANK_ACTIVITY);
}

function compactActivity(state, index) {
  const item = activityPreview(state, index);
  return View.Row({ style: { height: 72, paddingX: 20, align: "center", justify: "between" }, children: [
    View.Column({ style: { grow: 1, gap: 8 }, children: [
      View.Text({ text: item.title, style: { fontSize: "lg", color: "heading", fontWeight: "bold" } }),
      View.Text({ text: item.timestamp ? item.timestamp + "  ·  " + item.detail : "", style: { color: "muted" } }),
    ] }),
    View.Text({ text: item.amount ? money(item.amount) : "", style: { color: item.side === "SELL" ? "success" : "heading", fontWeight: "bold" } }),
  ] });
}

function positionPreview(state, index) {
  if (index === 0 && !state.dashboard.positionsAvailable) return UNAVAILABLE_POSITION;
  return state.dashboard.positions[index] ?? (index === 0 ? EMPTY_POSITION : BLANK_POSITION);
}

function compactPosition(state, index) {
  const item = positionPreview(state, index);
  return View.Row({ style: { height: 64, paddingX: 20, align: "center", gap: 12 }, children: [
    View.Text({ text: item.symbol, style: { grow: 1, basis: 0, fontSize: "lg", color: "heading", fontWeight: "bold" } }),
    View.Text({ text: item.quantity ? item.quantity + " SH" : "", style: { grow: 1, basis: 0, color: "muted" } }),
    View.Text({ text: item.averagePrice ? "AVG " + money(item.averagePrice) : "", style: { grow: 1, basis: 0, color: "muted" } }),
  ] });
}

function chart(state) {
  const tone = state.chartTrend.positive ? "success" : "danger";
  return View.Column({ style: { width: 632, height: 196 }, children: [
    View.Box({ style: { position: "relative", width: 632, height: 160, overflow: "hidden" }, children: [
      View.Box({ style: { position: "absolute", left: 0, top: 158, width: 632, height: 2, background: "disabled" } }),
      state.chartPoints.length < 2 ? View.Text({ text: "COLLECTING 5M VALUE HISTORY", style: { position: "absolute", left: 152, top: 72, color: "muted", fontWeight: "bold" } }) : null,
      state.chartSegments.map((item) => View.Box({ style: {
        position: "absolute", left: item.x, top: item.y - 1, width: item.width, height: 2,
        radius: 8, background: tone, rotate: item.angle, originX: -0.5, originY: 0,
      } })),
      state.chartPoints.map((item) => View.Box({ style: {
        position: "absolute", left: item.x - 3, top: item.y - 3, width: 6, height: 6, radius: 8, background: tone,
      } })),
    ] }),
    View.Row({ style: { height: 36, paddingX: 4, align: "center", justify: "between" }, children:
      state.chartLabels.map((label) => View.Text({ text: label || "", style: { color: "muted" } })) }),
  ] });
}

function section(title, detail, height, children, onPress) {
  return View.Pressable({
    onPress,
    style: { height, direction: "column" },
    children: [
      View.SectionHeading({ title, detail, action: true }),
      View.Card({ style: { grow: 1 }, children }),
    ],
  });
}

function dashboardScreen(state) {
  const dashboard = state.dashboard;
  const selectedIndex = state.accounts.findIndex((account) => account.number === state.selectedAccount);
  const accountLabel = dashboard.account.label + (dashboard.account.suffix
    ? "  ····" + dashboard.account.suffix + "  " + (selectedIndex + 1) + "/" + state.accounts.length
    : "") + "   ›";
  const pnl = state.span === "day" ? dashboard.pnlDay : dashboard.pnlWeek;
  return View.Screen({ children: [
    header("ROBINHOOD"),
    View.Column({ style: { grow: 1, paddingX: 24, paddingY: 12, gap: 12 }, children: [
      View.Pressable({
        onPress: () => model.update({ screen: "accounts" }),
        style: { width: "full", height: 56, paddingX: 20, direction: "row", align: "center", justify: "between", radius: 12, background: "surface", borderColor: "border", borderWidth: 1, shadow: 1 },
        children: [
          View.Text({ text: "ACCOUNT", style: { color: "muted", fontWeight: "bold" } }),
          View.Text({ text: accountLabel, style: { color: "heading", fontWeight: "bold" } }),
        ],
      }),
      View.Card({
        style: { grow: 1, minHeight: 291, paddingX: 20, paddingTop: 16 },
        children: [
          View.Row({ style: { height: 64, align: "end", justify: "between" }, children: [
            View.Text({ text: money(dashboard.totalValue), style: { fontSize: "title", color: "heading", fontWeight: "bold" } }),
            View.Text({ text: state.chartTrend.change + "  (" + state.chartTrend.percent + ")", style: { fontSize: "lg", color: state.chartTrend.positive ? "success" : "danger", fontWeight: "bold" } }),
          ] }),
          chart(state),
        ],
      }),
      View.Row({ style: { height: 44, gap: 12 }, children: [
        View.Pressable({ onPress: () => setSpan("day"), style: { width: 100, height: 44, align: "center", justify: "center", radius: 8, background: state.span === "day" ? "accent" : "surface" }, children:
          View.Text({ text: "1D", style: { color: state.span === "day" ? "white" : "muted", fontWeight: "bold" } }) }),
        View.Pressable({ onPress: () => setSpan("week"), style: { width: 100, height: 44, align: "center", justify: "center", radius: 8, background: state.span === "week" ? "accent" : "surface" }, children:
          View.Text({ text: "1W", style: { color: state.span === "week" ? "white" : "muted", fontWeight: "bold" } }) }),
        View.Text({ text: state.span === "day" ? "TODAY" : "PAST WEEK", style: { marginTop: 12, color: "muted" } }),
      ] }),
      View.Row({ style: { height: 104, gap: 16 }, children: [
        metric("VALUE", dashboard.totalValue), metric("CASH", dashboard.cash), metric("BUY POWER", dashboard.buyingPower),
      ] }),
      section("ACTIVITY", "LAST 7 DAYS", 194, [compactActivity(state, 0), compactActivity(state, 1)], () => model.update({ screen: "activity" })),
      section("POSITIONS", "", 180, [compactPosition(state, 0), compactPosition(state, 1)], () => model.update({ screen: "positions" })),
      View.Card({
        style: { width: "full", height: 110, paddingX: 20, direction: "row", align: "center", justify: "between" },
        children: [
          View.Column({ style: { gap: 12 }, children: [
            View.Text({ text: "REALIZED P&L", style: { fontSize: "xl", color: "heading", fontWeight: "bold" } }),
            View.Text({ text: "EQUITIES / " + (state.span === "day" ? "TODAY" : "WEEK"), style: { color: "muted" } }),
          ] }),
          View.Text({ text: money(pnl), style: { fontSize: "title", color: (number(pnl) ?? 0) >= 0 ? "success" : "danger", fontWeight: "bold" } }),
        ],
      }),
      View.Row({ style: { height: 64, gap: 16 }, children: [
        View.Box({ style: { grow: 1, height: "full" }, children: View.StatusBar({ text: state.status, tone: state.status.startsWith("REFRESH FAILED") ? "danger" : "neutral" }) }),
        View.Box({ style: { width: 176, height: 64 }, children: View.ActionButton({ label: state.refreshing ? "REFRESHING" : "REFRESH NOW", disabled: state.refreshing, onPress: refreshPortfolio }) }),
      ] }),
    ] }),
  ] });
}

function scrollRail(screen) {
  return View.ScrollRail({ onUp: () => scroll(screen, -1), onDown: () => scroll(screen, 1) });
}

function accountsScreen(state) {
  const visible = state.accounts.slice(state.accountScroll, state.accountScroll + 8);
  return View.Screen({ children: [
    header("ACCOUNTS"),
    View.Column({ style: { grow: 1, padding: 24 }, children: visible.length
      ? View.Row({ style: { gap: 20 }, children: [
        View.Column({ style: { grow: 1, gap: 12 }, children: visible.map((account) => View.Pressable({
          onPress: () => selectAccount(account),
          style: { width: "full", height: 100, paddingX: 20, direction: "column", justify: "center", gap: 12, radius: 12, shadow: 1, background: account.number === state.selectedAccount ? "successSoft" : "surface", borderColor: account.number === state.selectedAccount ? "success" : "border", borderWidth: 1 },
          children: [
            View.Row({ style: { align: "center", justify: "between" }, children: [
              View.Text({ text: account.label, style: { fontSize: "xl", color: "heading", fontWeight: "bold" } }),
              View.Text({ text: "····" + account.suffix, style: { fontSize: "lg", color: "muted", fontWeight: "bold" } }),
            ] }),
            View.Row({ style: { align: "center", justify: "between" }, children: [
              View.Badge({ label: account.status, tone: "success" }),
              account.number === state.selectedAccount ? View.Text({ text: "SELECTED", style: { color: "success", fontWeight: "bold" } }) : null,
            ] }),
          ],
        })) }),
        state.accounts.length > 8 ? scrollRail("accounts") : null,
      ] })
      : View.EmptyState({ compact: true, style: { height: "full" }, title: "NO ACCOUNTS AVAILABLE" }) }),
  ] });
}

function activityCard(item) {
  return View.Card({ style: { width: "full", height: 112, paddingX: 20, justify: "center", gap: 8 }, children: [
    View.Row({ style: { align: "center", justify: "between" }, children: [
      View.Text({ text: item.title, style: { fontSize: "lg", color: "heading", fontWeight: "bold" } }),
      View.Text({ text: money(item.amount), style: { fontSize: "lg", color: item.side === "SELL" ? "success" : "heading", fontWeight: "bold" } }),
    ] }),
    View.Text({ text: item.timestamp + "  ·  " + item.detail, style: { color: "muted" } }),
    View.Text({ text: item.state, style: { color: "success", fontWeight: "bold" } }),
  ] });
}

function activityScreen(state) {
  const visible = state.dashboard.activity.slice(state.activityScroll, state.activityScroll + 8);
  const empty = state.dashboard.activityAvailable ? "NO ACTIVITY YET" : "ACTIVITY UNAVAILABLE";
  return View.Screen({ children: [
    header("ACTIVITY", "LAST 7 DAYS"),
    View.Column({ style: { grow: 1, padding: 24 }, children: visible.length
      ? View.Row({ style: { gap: 20 }, children: [
        View.Column({ style: { grow: 1, gap: 12 }, children: visible.map(activityCard) }),
        state.dashboard.activity.length > 8 ? scrollRail("activity") : null,
      ] })
      : View.EmptyState({ title: empty, compact: true, style: { height: "full" } }) }),
  ] });
}

function positionCard(item) {
  return View.Card({ style: { width: "full", height: 98, paddingX: 20, justify: "center", gap: 12 }, children: [
    View.Row({ style: { align: "center" }, children: [
      View.Text({ text: item.symbol, style: { grow: 1, fontSize: "xl", color: "heading", fontWeight: "bold" } }),
      View.Text({ text: item.quantity + " SH", style: { fontSize: "lg", color: "heading", fontWeight: "bold" } }),
    ] }),
    View.Text({ text: "AVERAGE COST  " + money(item.averagePrice) + (item.marketValue ? "  ·  VALUE " + money(item.marketValue) : ""), style: { color: "muted" } }),
  ] });
}

function positionsScreen(state) {
  const visible = state.dashboard.positions.slice(state.positionScroll, state.positionScroll + 9);
  const empty = state.dashboard.positionsAvailable ? "NO OPEN POSITIONS" : "POSITIONS UNAVAILABLE";
  return View.Screen({ children: [
    header("POSITIONS"),
    View.Column({ style: { grow: 1, padding: 24 }, children: visible.length
      ? View.Row({ style: { gap: 20 }, children: [
        View.Column({ style: { grow: 1, gap: 12 }, children: visible.map(positionCard) }),
        state.dashboard.positions.length > 9 ? scrollRail("positions") : null,
      ] })
      : View.EmptyState({ title: empty, compact: true, style: { height: "full" } }) }),
  ] });
}

function render() {
  const state = model.get();
  if (state.screen === "accounts") return accountsScreen(state);
  if (state.screen === "activity") return activityScreen(state);
  if (state.screen === "positions") return positionsScreen(state);
  return dashboardScreen(state);
}

View.mount(render);
