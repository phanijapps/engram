import { describe, it, expect } from "vitest";
import { execFileSync } from "node:child_process";
// @ts-ignore JavaScript fixture builder owns artifact publication.
import { RECIPES, publishedRecipeArtifact } from "./helpers/build-native-recipes.mjs";

// One addon per process: upstream embedded teardown and N-API registries are
// intentionally isolated from other recipe builds and module loads.
const nativeSelection = String.raw`
const {createRequire} = require('node:module');
const {mkdtempSync,existsSync,rmSync} = require('node:fs');
const {tmpdir} = require('node:os'); const {join} = require('node:path');
const {randomUUID,createHash} = require('node:crypto'); const assert = require('node:assert/strict');
const artifactHash=()=>createHash('sha256').update(require('node:fs').readFileSync(process.argv[1])).digest('hex');const beforeHash=artifactHash();
const addon = require(process.argv[1]); const recipe=process.argv[2];
const root=mkdtempSync(join(tmpdir(),'node-provider-selection-')); const scope={tenant:'native-'+randomUUID(),subject:'subject',workspace:'workspace',session:'session',environment:'test'};
const config={storage_path:join(root,'store'),trusted_root:root,scope_policy:'Strict',embedding_provider:{provider_type:'none',model:'none',dimensions:384,prompt_profile:'query'},migration_mode:'Apply',capability_policy:'FailClosed',enable_vector:recipe==='helixdb-service',scope};
if(recipe==='pgvector') { assert(process.env.ENGRAM_TEST_PG_URL,'BLOCKED: actual PG fixture required'); config.storage_provider='pgvector';config.pgvector_connection_string=process.env.ENGRAM_TEST_PG_URL; }
if(recipe.startsWith('helixdb-')) { const mode=recipe.slice(8);config.storage_provider='helixdb'; config.helixdb={mode,logical_store:'node-'+randomUUID(),path:mode==='embedded'?join(root,'native'):null,endpoint:mode==='service'?process.env.HELIX_TEST_ENDPOINT:null,credential_env:mode==='service'?process.env.HELIX_TEST_CREDENTIAL_ENV:null}; if(mode==='service')assert(config.helixdb.endpoint,'BLOCKED: actual service fixture required'); }
let success=false;
try {
 assert.equal(typeof addon.NativeProvider,'function');
 const owner=new addon.NativeProvider(JSON.stringify(config)); const caps=JSON.parse(owner.capabilitiesJson());assert(caps.memory);
 const memory=owner.requireMemoryApi(); owner.close();owner.close();assert.throws(()=>owner.capabilitiesJson(),/closed/);
 const actor={id:'native-selection',kind:'agent'};
 const request={kind:'observation',content:{text:'native-selection-sentinel'},scope,requester:{actor,roles:[],permissions:['memory.write','memory.retrieve']},provenance:{source:'native-selection',actor,observedAt:new Date().toISOString(),confidence:1},policy:{visibility:'private',retention:'durable',sensitivity:'low',allowedUses:['retrieval']},idempotencyKey:randomUUID()};
 if(recipe.startsWith('helixdb-'))assert.throws(()=>memory.writeJson(JSON.stringify({...request,scope:{...scope,subject:'foreign-subject'}})),/scope|PolicyDenied|denied/);
 const record=JSON.parse(memory.writeJson(JSON.stringify(request))).record;assert(record.id);
 const readback=api=>recipe==='pgvector'?JSON.parse(api.searchJson(JSON.stringify({query:'native-selection-sentinel',scope,requester:request.requester}))).items.map(item=>item.content):JSON.parse(api.listMemoriesPagedJson(JSON.stringify({scope}))).items.map(item=>item.content.text);
 if(recipe==='pgvector')assert.throws(()=>memory.listMemoriesPagedJson(JSON.stringify({scope})),/does not implement paged reads/);
 assert.deepEqual(readback(memory),['native-selection-sentinel']);
 memory.close();memory.close();assert.throws(()=>memory.listMemoriesPagedJson(JSON.stringify({scope})),/closed/);
 const reopened=new addon.NativeProvider(JSON.stringify(config));const retained=reopened.requireMemoryApi();assert.deepEqual(readback(retained),['native-selection-sentinel']);retained.close();reopened.close();
 const bootstrap=addon.bootstrapIntegrationProvider(JSON.stringify(config)); assert(bootstrap.schemaVersion);assert(JSON.parse(bootstrap.capabilitiesJson).memory);
 if(recipe.startsWith('helixdb-')) {assert(!existsSync(join(root,'store','memory.db')));assert(!existsSync(join(root,'store','engram_data.db')));assert.equal(addon.NativeKnowledgeEngine,undefined);assert.equal(addon.presentationErrorFixture,undefined);assert.equal(addon.NativeProvider.openInstrumented,undefined);}
 // Selected but unpackaged provider must fail before creating a substitute root.
 const absent=join(root,'not-created');const bad={...config,storage_path:absent,storage_provider:recipe==='sqlite'?'helixdb':'sqlite',helixdb:recipe==='sqlite'?{mode:'embedded',logical_store:'missing'}:undefined,pgvector_connection_string:undefined};
 assert.throws(()=>new addon.NativeProvider(JSON.stringify(bad)),/BuildFeatureMissing|requires|feature|unavailable/);assert(!existsSync(absent));
 assert.equal(artifactHash(),beforeHash);success=true;console.log(JSON.stringify({recipe,pid:process.pid,artifact:process.argv[1],shaBefore:beforeHash,shaAfter:artifactHash(),constructed:true,bootstrap:true,retainedHandle:true,reopened:true,closed:true,scopeDimensions:5}));
} finally { if(success)rmSync(root,{recursive:true,force:true});else console.error('RETAIN_FAILED_NATIVE_ROOT '+root); }
`;

describe("actual native provider recipe selection", () => {
  for (const recipe of RECIPES) it(`constructs/bootstrap/closes ${recipe} without substitution`, () => {
    const artifact = publishedRecipeArtifact(recipe);
    if (!artifact) throw new Error(`BLOCKED: missing current ${recipe} native addon`);
    const output = execFileSync(process.execPath, ["-e", nativeSelection, artifact.path, recipe], {encoding:"utf8",stdio:["ignore","pipe","pipe"]});
    console.log(output.trim());
    expect(JSON.parse(output.trim())).toMatchObject({recipe,constructed:true,bootstrap:true,retainedHandle:true,reopened:true,closed:true});
  }, 600_000);
});
