# Release — knowledge

<!-- ASD-STE100 descriptive register: short sentences, one topic per paragraph. -->

## Knowledge — how shipping works

Surfaces: crates.io (workspace crates), npm (@engramlabs/*: mcp launcher,
client, contracts, node), agent bundle (skills+agents installed by launcher).
Every surface reconciles to a recorded source revision (DD-001). Merge
requires: 3 PASS reviews, owner approval at the human gate, L3 job.released
event with artifact hashes, ff-only merge. Compatibility classification per
surface change (compatible/breaking) is mandatory in specs. Rollback path
stated per plan step. Tarballs never committed (*.tgz ignored).
