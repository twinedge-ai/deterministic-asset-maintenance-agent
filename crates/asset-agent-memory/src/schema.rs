pub const SQLITE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS cases(
  case_id TEXT PRIMARY KEY,
  timestamp TEXT,
  asset_id TEXT,
  observations_json TEXT,
  derived_features_json TEXT,
  belief_json TEXT,
  prediction_json TEXT,
  recommendation_json TEXT,
  explanation_json TEXT,
  feedback_json TEXT,
  post_action_json TEXT,
  created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS checklist_stats(
  item_id TEXT PRIMARY KEY,
  item_text TEXT NOT NULL,
  fault_type TEXT NOT NULL,
  successes INTEGER NOT NULL DEFAULT 0,
  attempts INTEGER NOT NULL DEFAULT 0,
  last_updated TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS pump_thresholds(
  asset_id TEXT NOT NULL,
  threshold_name TEXT NOT NULL,
  default_value REAL NOT NULL,
  current_value REAL NOT NULL,
  proposed_value REAL,
  evidence_count INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL,
  last_updated TEXT DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY(asset_id, threshold_name)
);

CREATE TABLE IF NOT EXISTS calibration_events(
  event_id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  before_json TEXT NOT NULL,
  after_json TEXT NOT NULL,
  reason TEXT NOT NULL,
  requires_review INTEGER NOT NULL,
  created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS similar_case_index(
  case_id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL,
  feature_vector_json TEXT NOT NULL,
  label TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS llm_runs(
  llm_run_id TEXT PRIMARY KEY,
  case_id TEXT,
  provider TEXT,
  model_path TEXT,
  binary_path TEXT,
  prompt_json TEXT,
  prompt_text TEXT,
  raw_output TEXT,
  guarded_output TEXT,
  guard_passed INTEGER,
  fallback_used INTEGER,
  latency_ms REAL,
  created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS explanation_feedback(
  explanation_feedback_id TEXT PRIMARY KEY,
  case_id TEXT,
  explanation_source TEXT,
  helpful INTEGER,
  confusing INTEGER,
  notes TEXT,
  created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_cases_asset_created_at ON cases(asset_id, created_at);
CREATE INDEX IF NOT EXISTS idx_similar_case_asset ON similar_case_index(asset_id);
"#;
