-- raw.db as of design 5.4.2. Not migrated: a change bumps
-- `store::raw::RAW_SCHEMA` and older databases are emptied and rebuilt.

CREATE TABLE raw_frames (
  instance_id      TEXT NOT NULL,              -- the adapter instance
  seq              INTEGER NOT NULL,           -- within the instance
  ts               INTEGER NOT NULL,
  dir              TEXT NOT NULL,              -- send | recv | recv_nonjson | stderr | note
  agent_session_id TEXT,                       -- from params.sessionId
  frame            TEXT NOT NULL,              -- after structured redaction
  PRIMARY KEY (instance_id, seq)
) WITHOUT ROWID;
CREATE INDEX raw_by_ts ON raw_frames(ts);
CREATE INDEX raw_by_session ON raw_frames(agent_session_id, ts);
