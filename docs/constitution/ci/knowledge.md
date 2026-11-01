# Ci — knowledge

<!-- ASD-STE100 descriptive register: short sentences, one topic per paragraph. -->

## Knowledge — what CI runs

15 Python gates under `tools/ci/` (registry:
`docs/contracts/constitution/v1/gates/registry.json`): constitution umbrella,
contracts, engine-neutrality, surface-parity, implementation-registry,
coherent-integration (orphans), production-completeness (stubs+LLM-SDK lint),
distribution-drift, canonical-evidence (L3), job-artifacts (SDD pipeline),
docs umbrella (imports the three checkers), code-docs, bound-code matrix,
plus contracts codegen. Blocking vs advisory per the registry; advisory
windows tracked in root DECAY-WINDOWS. Runs on every push (ci.yml).
