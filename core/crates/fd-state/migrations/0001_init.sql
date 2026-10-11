-- Desired-state resources (data-model.md, "Core Service entities: desired state").
CREATE TABLE resource (
    kind        TEXT    NOT NULL,
    id          TEXT    NOT NULL,
    generation  INTEGER NOT NULL CHECK (generation > 0),
    spec        TEXT    NOT NULL CHECK (json_valid(spec)),
    status      TEXT             CHECK (status IS NULL OR json_valid(status)),
    deleted     INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    change_seq  INTEGER NOT NULL,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL,
    PRIMARY KEY (kind, id)
) STRICT;

CREATE INDEX resource_by_change ON resource (change_seq);

-- Core-internal settings such as the appliance identity. Desired-state settings are the
-- Settings resource.
CREATE TABLE setting (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL CHECK (json_valid(value)),
    updated_at  TEXT NOT NULL
) STRICT;

-- Sequence of authoritative changes, advanced once per committed ApplyChange.
CREATE TABLE state_meta (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    change_seq  INTEGER NOT NULL
) STRICT;

INSERT INTO state_meta (id, change_seq) VALUES (1, 0);
