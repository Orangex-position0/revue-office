CREATE TABLE IF NOT EXISTS artifact_publications (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    content TEXT NOT NULL,
    staging_path TEXT,
    final_path TEXT,
    error TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_artifact_publications_session
    ON artifact_publications(session_id);
CREATE INDEX IF NOT EXISTS idx_artifact_publications_status
    ON artifact_publications(status);
