-- Migration 1 : le schéma du 1a (voir docs/superpowers/specs/2026-10-23-etape-1a-coeur-design.md).
-- Les colonnes d'instants s'appellent start_at / end_at : `end` est un mot réservé de SQL.

CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE parents (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    monday_id TEXT,
    label TEXT,
    pinned INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    last_seen INTEGER
);
CREATE UNIQUE INDEX parents_monday ON parents(project_id, monday_id) WHERE monday_id IS NOT NULL;

CREATE TABLE subtasks (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    parent_id INTEGER REFERENCES parents(id),
    name TEXT NOT NULL,
    monday_item_id TEXT,
    is_base INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX subtasks_unique ON subtasks(project_id, COALESCE(parent_id, 0), name);

CREATE TABLE lots (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    parent_id INTEGER REFERENCES parents(id),
    subtask_id INTEGER REFERENCES subtasks(id),
    name TEXT,
    source TEXT NOT NULL DEFAULT 'manual'
);
CREATE UNIQUE INDEX lots_unique ON lots(project_id, COALESCE(parent_id, 0), COALESCE(subtask_id, 0), COALESCE(name, ''));

CREATE TABLE entries (
    id INTEGER PRIMARY KEY,
    lot_id INTEGER NOT NULL REFERENCES lots(id),
    start_at INTEGER NOT NULL,
    end_at INTEGER,
    removed INTEGER NOT NULL DEFAULT 0,
    note TEXT,
    CHECK (end_at IS NULL OR end_at > start_at)
);
-- Au plus une entrée ouverte. NULL étant distinct pour SQLite, l'index porte sur l'expression (end_at IS NULL).
CREATE UNIQUE INDEX entries_one_open ON entries ((end_at IS NULL)) WHERE end_at IS NULL;
CREATE INDEX entries_by_start ON entries(start_at);

CREATE TABLE days (
    date TEXT PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN ('open', 'closed', 'declared', 'to_redeclare')),
    closed_at INTEGER,
    declared_at INTEGER,
    closed_totals TEXT,
    declared_totals TEXT
);

CREATE TABLE state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    paused_lot_id INTEGER REFERENCES lots(id)
);
INSERT INTO state (id, paused_lot_id) VALUES (1, NULL);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT INTO settings (key, value) VALUES ('main_subtask', 'Front'), ('rounding_minutes', '0');

INSERT INTO projects (id, name, is_default) VALUES (1, 'Mon projet', 1);
INSERT INTO subtasks (project_id, parent_id, name, is_base) VALUES
    (1, NULL, 'Back', 1), (1, NULL, 'Front', 1), (1, NULL, 'Recettage', 1), (1, NULL, 'Maquette', 1);
