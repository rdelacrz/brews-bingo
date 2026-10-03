---
type: "path_query"
date: "2026-10-03T16:51:32.164421+00:00"
question: "What is the exact relationship between BR-021 and the tracking interpretation note?"
contributor: "graphify"
outcome: "useful"
source_nodes: ["BR-021 [Approved] — The host shall be able to review completed games’ draw records after starting a new game.", "Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival"]
---

# Q: What is the exact relationship between BR-021 and the tracking interpretation note?

## Answer

Expanded from graph vocabulary: [host, review, completed, draw, record, history, retention]. Graphify returns a direct AMBIGUOUS conceptual edge. BR-021 explicitly requires the host to review completed draw records after starting a new game (requirements.md L48); the design separately provides read-only History for started terminal games with a three-calendar-month retention window (hld.md L253-L272). The note at requirements.md L58 says past-game saving is not implied, which is in tension with BR-021 and should be reconciled; detailed History UI/schema remains separate design work.

## Outcome

- Signal: useful

## Source Nodes

- BR-021 [Approved] — The host shall be able to review completed games’ draw records after starting a new game.
- Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival