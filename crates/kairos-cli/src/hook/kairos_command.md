---
description: État de Kairos, ou bascule vers une tâche (ID Monday ou nom)
argument-hint: [ID Monday ou nom de la tâche]
---

Utilise les outils MCP de Kairos.

Argument de l'utilisateur : $ARGUMENTS

- Sans argument : appelle `status` et résume en une phrase ce qui tourne (lot, durée, total du jour).
- Avec un ID Monday : appelle `switch` avec cet ID (`parent`), en passant `label` si tu connais le nom de la tâche.
- Avec un nom ou une description sans ID : appelle `recent` pour retrouver le lot correspondant, puis `switch` avec son ID. Plusieurs candidats ou aucun : demande lequel.
- Ne devine jamais un ID Monday.
