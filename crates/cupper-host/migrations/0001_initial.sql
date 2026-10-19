-- host.db as of design 5.4.2. Until T160 this script is the only one and
-- changes in place (plan P-4). Times are UTC milliseconds; JSON is UTF-8 text.

-- Keys and their writers: `store::meta::MetaKey`.
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;

CREATE TABLE projects (
  project_id     TEXT PRIMARY KEY,
  path           TEXT NOT NULL UNIQUE,     -- realpath
  name           TEXT NOT NULL,
  git_common_dir TEXT,                     -- NULL outside git
  repo_key       TEXT,                     -- first 16 hex of sha256(git_common_dir)
  position       INTEGER NOT NULL,
  added_at       INTEGER NOT NULL,
  removed_at     INTEGER,
  trust_state    TEXT NOT NULL DEFAULT 'untrusted',  -- untrusted | trusted | stale
  trust_digest   TEXT,
  trusted_at     INTEGER,
  trusted_by     TEXT                      -- pr_desktop | pr_test | auto_empty
);

CREATE TABLE sessions (
  session_id        TEXT PRIMARY KEY,
  project_id        TEXT NOT NULL REFERENCES projects(project_id),
  agent             TEXT NOT NULL,           -- claude | codex
  agent_session_id  TEXT,
  title             TEXT,
  title_source      TEXT NOT NULL DEFAULT 'none',  -- none | prompt | agent | user
  workspace_kind    TEXT NOT NULL,           -- local | worktree
  workspace_path    TEXT NOT NULL,           -- realpath
  worktree_branch   TEXT,
  base_ref          TEXT,
  base_oid          TEXT,
  config_json       TEXT NOT NULL DEFAULT '{}',
  initial_prompt_json TEXT,
  mode_set_by       TEXT,
  full_access_confirmed_at INTEGER,
  state             TEXT NOT NULL,           -- idle | running | error | interrupted
  read_only         INTEGER NOT NULL DEFAULT 0,
  queue_version     INTEGER NOT NULL DEFAULT 0,
  queue_paused      TEXT,                    -- why; NULL when not paused
  last_read_seq     INTEGER NOT NULL DEFAULT 0,
  last_turn_end_seq INTEGER NOT NULL DEFAULT 0,
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL,
  archived_at       INTEGER
);
CREATE INDEX sessions_by_project ON sessions(project_id, archived_at, updated_at DESC);

CREATE TABLE turns (
  session_id      TEXT NOT NULL,
  turn_id         TEXT NOT NULL,
  turn_no         INTEGER NOT NULL,          -- from 1 within the session
  origin          TEXT NOT NULL,             -- user | queued | steer_new_turn | autonomous
  status          TEXT NOT NULL,             -- running | completed | cancelled | failed | interrupted
  first_seq       INTEGER NOT NULL,
  last_seq        INTEGER,
  started_at      INTEGER NOT NULL,
  ended_at        INTEGER,
  stop_reason     TEXT,
  snapshot_start  TEXT,                      -- git tree id, never sent
  snapshot_end    TEXT,
  snapshot_status TEXT,                      -- ok | partial | failed | skipped
  usage_json      TEXT,
  PRIMARY KEY (session_id, turn_id)
) WITHOUT ROWID;
CREATE INDEX turns_by_time ON turns(session_id, started_at);
CREATE UNIQUE INDEX turns_by_no ON turns(session_id, turn_no);

CREATE TABLE turn_snapshots (
  session_id TEXT NOT NULL,
  turn_id    TEXT NOT NULL,
  turn_no    INTEGER NOT NULL,
  v          INTEGER NOT NULL,               -- event schema version when written
  ops        TEXT NOT NULL,                  -- JSON array of event bodies
  bytes      INTEGER NOT NULL,
  PRIMARY KEY (session_id, turn_id)
) WITHOUT ROWID;
CREATE UNIQUE INDEX turn_snapshots_by_no ON turn_snapshots(session_id, turn_no);

CREATE TABLE events (
  stream TEXT NOT NULL,                      -- index | session/<session_id>
  seq    INTEGER NOT NULL,
  type   TEXT NOT NULL,
  v      INTEGER NOT NULL,
  ts     INTEGER NOT NULL,
  json   TEXT NOT NULL,                      -- the whole envelope
  PRIMARY KEY (stream, seq)
) WITHOUT ROWID;

CREATE TABLE checkpoints (
  stream     TEXT PRIMARY KEY,
  at_seq     INTEGER NOT NULL,
  view_v     INTEGER NOT NULL,               -- `cupper_protocol::view::VIEW_VERSION`
  view       TEXT NOT NULL,
  created_at INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE interactions (
  request_id       TEXT PRIMARY KEY,
  session_id       TEXT NOT NULL,
  turn_id          TEXT,
  item_id          TEXT,
  kind             TEXT NOT NULL,            -- permission | question | plan_confirm | auth_url
  digest           TEXT NOT NULL,
  status           TEXT NOT NULL,            -- pending | resolved | cancelled | expired
  adapter_instance TEXT NOT NULL,
  request_json     TEXT NOT NULL,
  outcome_json     TEXT,
  resolved_by      TEXT,
  reason           TEXT,
  created_at       INTEGER NOT NULL,
  resolved_at      INTEGER
);
CREATE INDEX interactions_pending ON interactions(session_id) WHERE status = 'pending';

CREATE TABLE queue_entries (
  queue_id     TEXT PRIMARY KEY,
  session_id   TEXT NOT NULL,
  position     INTEGER NOT NULL,
  content_json TEXT NOT NULL,
  principal    TEXT NOT NULL,
  command_id   TEXT NOT NULL,
  created_at   INTEGER NOT NULL
);
CREATE INDEX queue_by_session ON queue_entries(session_id, position);

CREATE TABLE commands (
  principal   TEXT NOT NULL,
  command_id  TEXT NOT NULL,
  method      TEXT NOT NULL,
  params_hash TEXT NOT NULL,
  state       TEXT NOT NULL,                 -- done | pending_external
  result      TEXT,
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (principal, command_id)
) WITHOUT ROWID;
CREATE INDEX commands_by_time ON commands(created_at);

CREATE TABLE adapters (
  instance_id     TEXT PRIMARY KEY,
  agent           TEXT NOT NULL,
  pid             INTEGER NOT NULL,
  pgid            INTEGER NOT NULL,
  start_us        INTEGER NOT NULL,          -- tells a reused pid apart
  adapter_version TEXT NOT NULL,
  node_version    TEXT NOT NULL,
  cli_path        TEXT,
  cli_version     TEXT,
  started_at      INTEGER NOT NULL,
  ended_at        INTEGER,
  exit_status     TEXT
);
CREATE INDEX adapters_live ON adapters(agent) WHERE ended_at IS NULL;

CREATE TABLE agent_capabilities (
  agent           TEXT PRIMARY KEY,
  adapter_version TEXT NOT NULL,
  cli_version     TEXT,
  captured_at     INTEGER NOT NULL,
  json            TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE blobs (
  hash         TEXT PRIMARY KEY,             -- sha-256 hex
  size         INTEGER NOT NULL,
  media_type   TEXT,
  created_at   INTEGER NOT NULL,
  last_used_at INTEGER NOT NULL
) WITHOUT ROWID;
CREATE TABLE blob_refs (
  hash  TEXT NOT NULL,
  owner TEXT NOT NULL,                       -- session_id
  PRIMARY KEY (hash, owner)
) WITHOUT ROWID;

CREATE TABLE settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,                  -- JSON
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE audit (
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  event       TEXT NOT NULL,
  conn_id     TEXT,
  principal   TEXT,
  peer_json   TEXT,
  detail_json TEXT
);
CREATE INDEX audit_by_ts ON audit(ts);
