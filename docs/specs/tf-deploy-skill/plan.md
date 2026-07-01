# Plan: tf-deploy-skill

### T1 — Create the skill
- **Approach:** `.claude/skills/tf-deploy/SKILL.md`. Bundled Terraform assets: `templates/aws/`, `templates/gcp/`, `templates/azure/` — each with `main.tf`, `variables.tf`, `outputs.tf`. Modules: block storage, container/VM compute for the backend, LLM config (Bedrock/Vertex/Foundry env vars), embedding model, optional UI service.

### T2 — First invocation: AWS + Bedrock deployment
- **Approach:** Run the skill → produces `terraform/aws/` with ECR + ECS Fargate + S3 + Bedrock env vars. Validate `terraform plan` succeeds.
