import {it, expect} from "vitest";
import {createNativeProviderTransport} from "../src/provider.js";
import type {NativeProviderBinding} from "../src/binding.js";
it("transport close releases its injected owner exactly once", () => {
 let closes=0;
 const provider={close(){closes++;}} as NativeProviderBinding;
 const transport=createNativeProviderTransport({provider});
 transport.close();transport.close();expect(closes).toBe(1);
});
it("bootstrap and constructor expose selected-provider rejection instead of conformance seeds", async () => {
 // @ts-ignore Runtime helper publishes immutable current artifacts.
 const {publishedRecipeArtifact}=await import("./helpers/build-native-recipes.mjs");
 const {createRequire}=await import("node:module");
 const addon=createRequire(import.meta.url)(publishedRecipeArtifact("sqlite").path);
 expect(()=>addon.bootstrapIntegrationProvider(JSON.stringify({storage_provider:"helixdb",helixdb:{mode:"embedded",logical_store:"unpackaged"},storage_path:"/not-created",trusted_root:"/",scope_policy:"Strict",embedding_provider:{provider_type:"none",model:"none",dimensions:384,prompt_profile:"query"},migration_mode:"Apply",capability_policy:"FailClosed",enable_vector:false,scope:{tenant:"bootstrap"}}))).toThrow(/BuildFeatureMissing/);
});
