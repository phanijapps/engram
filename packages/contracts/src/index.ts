import {
  engramV1Definitions,
  engramV1Schema,
  type EngramV1DefinitionName
} from "./generated/schema.generated.js";

export {
  engramV1Definitions,
  engramV1Schema,
  type EngramV1DefinitionName
} from "./generated/schema.generated.js";

/** Re-exports TypeScript types generated from the accepted Engram v1 JSON Schema definitions. */
export type * from "./generated/types.generated.js";

/** Names of accepted Engram v1 contract definitions available from the schema package. */
export type ContractDefinitionName = EngramV1DefinitionName;

/** Returns one accepted Engram v1 JSON Schema definition for validation or tooling. */
export function getContractDefinition(name: ContractDefinitionName): unknown {
  return engramV1Definitions[name];
}
