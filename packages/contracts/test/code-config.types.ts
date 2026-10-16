import type {CodeIntelligenceConfig, CodeIntelligenceConfigOverlay} from "../src/index.js";

// The resolved snapshot requires complete fusion; an operator overlay is partial.
// @ts-expect-error Effective fusion cannot omit RRF fields or rerank parameters.
const incomplete: CodeIntelligenceConfig["fusion"] = {rerank: {}};
const partial: CodeIntelligenceConfigOverlay = {fusion: {rrf_k: 42}};
void incomplete;
void partial;
