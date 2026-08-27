function titleFrom(value) {
  const title = String(value ?? "").trim();
  if (!title) throw new Error("title is required");
  if (title.length > 80) throw new Error("title must be at most 80 characters");
  return title;
}

function createTodo(args) {
  const title = titleFrom(args.title);
  return PocketPi.data.transaction(() => {
    PocketPi.data.query(
      "INSERT INTO todos(title, completed) VALUES(?, 0)",
      [title],
    );
    return PocketPi.data.query(
      "SELECT id, title, completed FROM todos WHERE id = last_insert_rowid()",
    )[0];
  });
}

function updateTodo(args) {
  const id = Number(args.id);
  if (!Number.isInteger(id) || id < 1) throw new Error("id must be a positive integer");
  const changes = [];
  const values = [];
  if (args.title !== undefined) {
    changes.push("title = ?");
    values.push(titleFrom(args.title));
  }
  if (args.completed !== undefined) {
    if (typeof args.completed !== "boolean") throw new Error("completed must be a boolean");
    changes.push("completed = ?");
    values.push(args.completed ? 1 : 0);
  }
  if (!changes.length) throw new Error("title or completed is required");

  return PocketPi.data.transaction(() => {
    const existing = PocketPi.data.query("SELECT id FROM todos WHERE id = ?", [id])[0];
    if (!existing) throw new Error(`todo ${id} not found`);
    PocketPi.data.query(`UPDATE todos SET ${changes.join(", ")} WHERE id = ?`, [...values, id]);
    return PocketPi.data.query(
      "SELECT id, title, completed FROM todos WHERE id = ?",
      [id],
    )[0];
  });
}

PocketPi.defineActions({ createTodo, updateTodo });
