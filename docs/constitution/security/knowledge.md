# Security — knowledge

<!-- ASD-STE100 descriptive register: short sentences, one topic per paragraph. -->

## Knowledge — posture and trust boundaries

No LLM/model SDK in core or adapter manifests (deterministic core; LLM calls
stay TS-side — PR-002, gate-enforced). Shipped code carries zero stub//mock markers (baseline 0, measured). No secrets in artifacts;
launcher install is name-tagged (only removes its own files). The L3 audit
chain (events.jsonl) is the tamper-evident record of privileged actions.
Embeddings: FastEmbed local or HTTP provider — never a hard network dep
(disabled-vector mode is fully offline at query time).
