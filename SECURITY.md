# Security Policy

## Supported Versions

Engram is pre-1.0. Security fixes target the main branch until the project
publishes versioned releases.

## Reporting A Vulnerability

Do not open public issues for vulnerabilities involving data leakage, policy
bypass, credential exposure, or unsafe deletion behavior.

Until a dedicated security contact is published, report privately to the project
maintainers through the repository host's private vulnerability reporting
feature if available.

## Security-Sensitive Areas

- Tenant and scope isolation.
- Policy enforcement before retrieval composition.
- Redaction and forgetting behavior.
- Provenance and evidence integrity.
- Generated bindings preserving contract validation.
- Adapter error handling that could hide partial failures.
