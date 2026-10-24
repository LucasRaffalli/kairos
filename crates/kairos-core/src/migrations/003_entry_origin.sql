-- Migration 3 : l'origine de chaque entrée (voir le spec de l'étape 2a).
-- `source` : qui a lancé l'entrée (vide pour les entrées d'avant) ; `workdir` : le dossier de travail du lancement.
ALTER TABLE entries ADD COLUMN source TEXT;
ALTER TABLE entries ADD COLUMN workdir TEXT;
