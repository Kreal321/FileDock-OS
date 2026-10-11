CREATE TABLE operation (
    id            TEXT PRIMARY KEY,
    kind          TEXT NOT NULL,
    state         TEXT NOT NULL,
    resource_kind TEXT,
    resource_id   TEXT,
    started_by    TEXT NOT NULL,
    params        TEXT NOT NULL CHECK (json_valid(params)),
    error         TEXT,
    started_at    TEXT NOT NULL,
    finished_at   TEXT,
    updated_at    TEXT NOT NULL
) STRICT;

CREATE INDEX operation_by_state ON operation (state, started_at);
CREATE INDEX operation_by_started ON operation (started_at DESC);

CREATE TABLE operation_step (
    operation_id TEXT    NOT NULL REFERENCES operation(id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    state        TEXT    NOT NULL,
    intent       TEXT             CHECK (intent IS NULL OR json_valid(intent)),
    result       TEXT             CHECK (result IS NULL OR json_valid(result)),
    attempts     INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    detail       TEXT,
    PRIMARY KEY (operation_id, name)
) STRICT;
