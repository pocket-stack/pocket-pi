const todos = View.state([]);

PocketPi.projection.many(
  "SELECT id, title, completed FROM todos ORDER BY completed, id DESC LIMIT 6",
  {},
  (rows) => todos.set(rows),
);

function todoRow(todo) {
  return View.Checkbox({
    label: todo.title,
    checked: Boolean(todo.completed),
    onChange: (completed) => PocketPi.action("updateTodo", { id: todo.id, completed }),
    style: { background: "surface", borderWidth: 1, borderColor: "border", radius: 12 },
  });
}

View.mount(() => View.Screen({ children: [
  View.Header({
    title: "TODO LIST",
    metaTop: "LOCAL",
    metaBottom: "LOCAL DATA",
    onBack: () => PocketPi.navigate("pi-agent"),
  }),
  View.PageIntro({
    eyebrow: "CREATED BY PI AGENT",
    title: "Keep moving",
    description: "Ask Pi Agent to create or edit a task.",
    tone: "info",
  }),
  View.Column({
    style: { grow: 1, paddingX: 20, paddingY: 12, gap: 10 },
    children: todos.get().length
      ? todos.get().map(todoRow)
      : View.EmptyState({
        style: { height: "full" },
        icon: "T",
        title: "NO TASKS YET",
        detail: "ASK PI AGENT TO CREATE ONE",
        tone: "info",
      }),
  }),
  View.Box({ style: { height: 72, paddingX: 20, paddingY: 12 }, children:
    View.Text({
      text: () => `${todos.get().filter((todo) => Boolean(todo.completed)).length}/${todos.get().length} DONE`,
      style: { fontSize: "2xl", fontWeight: "bold", color: "muted", textAlign: "right" },
    }) }),
] }));
