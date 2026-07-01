# Spec: tf-deploy-skill (Terraform deployment skill)

- **Status:** Draft
- **Shape:** mixed (skill + infrastructure templates)
- **Constrained by:** AGENTS.md (infrastructure behind adapters)
- **Contract:** none

## Objective

A Claude Code skill that generates a complete Terraform deployment for engram to
any major cloud (AWS, GCP, Azure). The deployment includes: block storage for
git repos, compute (container/VM) for the async indexing backend, LLM model
config (Bedrock / Vertex / Foundry), embedding model config, and optional UI.
The backend indexes code asynchronously; the UI is optional and can be deployed
separately or omitted.

The skill asks: cloud provider, region, LLM (Bedrock Claude / Vertex Gemini /
Foundry GPT), embedding model, repo storage (S3 / GCS / Azure Blob), compute
size. Produces a `terraform/` directory with modules: `main.tf`, `variables.tf`,
`outputs.tf`, plus per-provider modules.

## Acceptance Criteria

- [ ] `.claude/skills/tf-deploy/SKILL.md` with the procedure.
- [ ] Bundled Terraform templates for AWS, GCP, Azure.
- [ ] Covers: block storage, async indexing compute, LLM + embedding config, optional UI.
- [ ] Produces a deployable `terraform/` directory per invocation.
