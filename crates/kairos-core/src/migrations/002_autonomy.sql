-- Migration 2 : l'autonomie de Claude (voir le spec de l'étape 1b).
-- `ask` : Claude demande avant les actions difficiles à défaire ; `auto` : il agit seul.
INSERT INTO settings (key, value) VALUES ('claude_autonomy', 'ask');
