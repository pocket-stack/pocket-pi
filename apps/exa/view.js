const HISTORY_PAGE_SIZE = 10;
const HISTORY_MAX_ROWS = 50;
const LANDSCAPE = View.viewport.orientation === "landscape";
const HISTORY_VISIBLE_ROWS = LANDSCAPE ? 1 : 6;

const model = View.state({
  history: [],
  hasMore: false,
  offset: 0,
  status: "SEARCH HISTORY IS LOCAL",
});
let historyLimit = HISTORY_PAGE_SIZE;

function searchTime(seconds) {
  const value = new Date(seconds * 1000).toISOString();
  return `${value.slice(0, 10)}  ·  ${value.slice(11, 16)} UTC`;
}

function loadMore() {
  if (!model.get().hasMore || historyLimit >= HISTORY_MAX_ROWS) return;
  historyLimit = Math.min(HISTORY_MAX_ROWS, historyLimit + HISTORY_PAGE_SIZE);
  historyProjection.refresh();
}

function scrollHistory(direction) {
  let state = model.get();
  const step = 4;
  if (direction > 0 && state.offset + HISTORY_VISIBLE_ROWS + step > state.history.length && state.hasMore) {
    loadMore();
    state = model.get();
  }
  model.update({
    offset: direction < 0
      ? Math.max(0, state.offset - step)
      : Math.min(Math.max(0, state.history.length - HISTORY_VISIBLE_ROWS), state.offset + step),
  });
}

const historyProjection = PocketPi.projection.many(
  `SELECT id,query,searched_at,status,result_count,top_title,error
   FROM searches ORDER BY id DESC LIMIT $limit`,
  () => ({ "$limit": historyLimit + 1 }),
  (rows) => {
    const history = rows.slice(0, historyLimit);
    model.update({
      history,
      hasMore: rows.length > historyLimit && historyLimit < HISTORY_MAX_ROWS,
      offset: Math.min(model.get().offset, Math.max(0, history.length - HISTORY_VISIBLE_ROWS)),
      status: history[0]?.status === "error"
        ? String(history[0].error || "EXA SEARCH FAILED").slice(0, 80)
        : history.length ? "SEARCH HISTORY UPDATED FROM SQLITE" : "SEARCH HISTORY IS LOCAL",
    });
  },
);

function historyCard(item) {
  return View.Card({
    style: { height: 126, paddingX: 20, direction: "row", align: "center", justify: "between" },
    children: [
      View.Column({ style: { grow: 1, gap: 8 }, children: [
        View.Text({ text: item.query.slice(0, 48), style: { fontSize: "lg", fontWeight: "bold", color: "heading" } }),
        View.Text({ text: (item.top_title || item.error || "No result title").slice(0, 58), style: { color: "muted" } }),
        View.Text({ text: searchTime(item.searched_at), style: { color: "info", fontWeight: "bold" } }),
      ] }),
      View.Badge({
        label: item.status === "ok" ? `${item.result_count} RESULTS` : "FAILED",
        tone: item.status === "ok" ? "success" : "danger",
      }),
    ],
  });
}

function render() {
  const state = model.get();
  const visible = state.history.slice(state.offset, state.offset + HISTORY_VISIBLE_ROWS);
  const scrollable = state.history.length > HISTORY_VISIBLE_ROWS || state.hasMore;
  return View.Screen({ children: [
    View.Header({
      title: "EXA RESEARCH",
      metaTop: "POCKET APP",
      metaBottom: "SQLITE HISTORY",
      onBack: () => PocketPi.navigate("pi-agent"),
    }),
    View.PageIntro({
      eyebrow: "AGENT RESEARCH MEMORY",
      title: "Search history",
      description: "Every research.search call is saved here automatically.",
      tone: "info",
    }),
    View.Column({ style: { grow: 1, paddingX: LANDSCAPE ? 16 : 24, paddingY: LANDSCAPE ? 8 : 16 }, children: state.history.length === 0
      ? View.EmptyState({
        style: { height: "full" },
        icon: "E",
        title: "No searches yet",
        detail: "Ask Pi Agent to research a topic.\nThe search and its results will appear here.",
        tone: "info",
      })
      : View.Row({ style: { gap: 20 }, children: [
        View.Column({ style: { grow: 1, gap: 12 }, children: visible.map(historyCard) }),
        scrollable ? View.ScrollRail({ onUp: () => scrollHistory(-1), onDown: () => scrollHistory(1) }) : null,
      ] }) }),
    View.Box({ style: { height: LANDSCAPE ? 52 : 96 }, children: View.StatusBar({
      text: state.status,
      tone: state.status.includes("FAILED") ? "danger" : "neutral",
      dark: true,
    }) }),
  ] });
}

View.mount(render);
