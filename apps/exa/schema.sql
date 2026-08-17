CREATE TABLE searches (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  query TEXT NOT NULL,
  searched_at INTEGER NOT NULL,
  status TEXT NOT NULL,
  result_count INTEGER NOT NULL DEFAULT 0,
  top_title TEXT,
  error TEXT
);

CREATE INDEX searches_retention ON searches(searched_at);
