CREATE TABLE IF NOT EXISTS artifact_publications (
    id VARCHAR(36) PRIMARY KEY,
    session_id VARCHAR(36) NOT NULL,
    owner_id VARCHAR(36) NOT NULL,
    kind VARCHAR(100) NOT NULL,
    title VARCHAR(500) NOT NULL,
    status VARCHAR(50) NOT NULL,
    content LONGTEXT NOT NULL,
    staging_path VARCHAR(2000),
    final_path VARCHAR(2000),
    error VARCHAR(2000),
    version INT NOT NULL DEFAULT 1,
    created_at VARCHAR(50) NOT NULL,
    updated_at VARCHAR(50) NOT NULL,
    INDEX idx_artifact_publications_session (session_id),
    INDEX idx_artifact_publications_status (status)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
