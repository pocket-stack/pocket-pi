const value = View.state("CLICKED");

PocketPi.projection.one(
  "SELECT value FROM state LIMIT 1",
  {},
  (row) => value.set(String(row?.value || "clicked").toUpperCase()),
);

function render() {
  return View.Screen({ children: [
    View.Header({
      title: "DEMO",
      metaTop: "POCKET APP",
      metaBottom: "ITERATION E2E",
      onBack: () => PocketPi.navigate("pi-agent"),
    }),
    View.PageIntro({
      eyebrow: "AGENT ITERATION",
      title: "View + Action",
      description: "One button writes one SQLite value.",
      tone: "info",
    }),
    View.Column({
      style: { grow: 1, align: "center", justify: "center", gap: 18 },
      children: [
        View.Badge({ label: "BUILT BY PI", tone: "info" }),
        View.Text({ text: "SQL VALUE", style: { color: "muted", fontWeight: "bold" } }),
        View.Text({ text: value.get, style: { fontSize: "xl", fontWeight: "bold" } }),
      ],
    }),
    View.Box({
      style: { height: 104, paddingX: 20, paddingY: 12 },
      children: View.ActionButton({
        label: "SET CLICKED",
        onPress: () => PocketPi.action("setValue"),
        style: { width: "full", height: "full" },
      }),
    }),
  ] });
}

View.mount(render);
