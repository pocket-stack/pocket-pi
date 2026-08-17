CREATE TABLE accounts (
  account_number TEXT PRIMARY KEY,
  label TEXT NOT NULL,
  suffix TEXT NOT NULL,
  account_type TEXT,
  status TEXT NOT NULL,
  agentic_allowed INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL
);

CREATE TABLE portfolio_current (
  account_number TEXT PRIMARY KEY,
  cash TEXT,
  buying_power TEXT,
  day_pnl TEXT,
  week_pnl TEXT,
  observed_at INTEGER NOT NULL
);

CREATE TABLE total_value (
  account_number TEXT NOT NULL,
  observed_at INTEGER NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY(account_number, observed_at)
);

CREATE TABLE positions (
  account_number TEXT NOT NULL,
  symbol TEXT NOT NULL,
  quantity TEXT,
  average_price TEXT,
  market_value TEXT,
  observed_at INTEGER NOT NULL,
  PRIMARY KEY(account_number, symbol)
);

CREATE TABLE activities (
  account_number TEXT NOT NULL,
  activity_id TEXT NOT NULL,
  occurred_at TEXT,
  observed_at INTEGER NOT NULL,
  symbol TEXT,
  side TEXT,
  quantity TEXT,
  price TEXT,
  amount TEXT,
  state TEXT,
  activity_type TEXT,
  PRIMARY KEY(account_number, activity_id)
);

CREATE INDEX activities_account_recent
  ON activities(account_number, occurred_at DESC, observed_at DESC);

CREATE TABLE refresh_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  started_at INTEGER NOT NULL,
  completed_at INTEGER NOT NULL,
  status TEXT NOT NULL,
  operation_count INTEGER NOT NULL,
  success_count INTEGER NOT NULL,
  error TEXT
);

CREATE INDEX refresh_runs_recent ON refresh_runs(id DESC);
