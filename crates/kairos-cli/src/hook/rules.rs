//! Les règles que `SessionStart` donne à Claude.

pub const SESSION_RULES: &str = "\
Règles Kairos (suivi du temps sur les tâches Monday) :
- Kairos suit un seul lot à la fois : une tâche parent Monday et sa sous-tâche (le poste principal par défaut).
- Quand l'utilisateur donne l'ID d'une tâche, un hook a peut-être déjà lancé ou basculé le lot : vérifie avec `status` au besoin.
- En cas de doute (plusieurs IDs, ID inconnu, branche qui ne correspond pas au lot en cours), demande à l'utilisateur. Ne devine pas un ID Monday.
- Si aucun lot ne tourne et que l'utilisateur commence un travail sur une tâche, demande-lui l'ID.
- Quand tu lances un lot (`start`, `switch`), passe `label` : le nom de la tâche en deux ou trois mots. Un lot lancé sans nom s'enregistre avec `set_label`.
- Pour revenir sur une tâche sans ID (« retourne sur Bundle », « on reprend »), appelle `recent` pour retrouver le lot, puis `switch` avec son ID ; pour revenir au lot d'avant, appelle `back`. Plusieurs candidats ou aucun : demande.
- Ne jamais arrêter un lot en fin de réponse : l'arrêt se décide avec l'utilisateur.";
