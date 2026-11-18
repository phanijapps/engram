import {it,expect} from "vitest";
import {createRequire} from "node:module";
// @ts-ignore Owned JavaScript native artifact builder.
import {ensureInstrumentedRecipeArtifact} from "./helpers/build-native-recipes.mjs";
const requireNode=createRequire(import.meta.url);
let fileArtifact: any;
function currentInstrumentedArtifact() {
  return fileArtifact ??= ensureInstrumentedRecipeArtifact();
}
const id="11111111-2222-4333-8444-555555555555";
it("presentation_ direct and batch preserve exact UUID/classification without private payload", () => {
 const artifact=currentInstrumentedArtifact();
 const addon=createRequire(import.meta.url)(artifact.path);
 expect(()=>addon.presentationErrorFixture()).toThrow(id);
 let text="";try{addon.presentationErrorFixture();}catch(error){text=String(error);}
 expect(text).toContain("OutcomeUnknown");expect(text).not.toContain("secret-record");expect(text).not.toContain("secret-token");
 const batch=JSON.parse(addon.presentationBatchFixture());
 expect(batch.status).toBe("Partial");expect(batch.steps.map((step:any)=>step.status)).toEqual(["Succeeded","Failed","Skipped"]);
 expect(batch.steps[1].error).toContain(id);expect(batch.steps[1].error).toContain("OutcomeUnknown");expect(JSON.stringify(batch)).not.toMatch(/secret-record|secret-token/);
 // Actual instrumented initialization before fresh Store aliases, without
 // injecting a loss here (real outcome_unknown proof belongs to Task006).
 const {mkdtempSync,rmSync}=requireNode("node:fs");
 const {tmpdir}=requireNode("node:os");const {join,resolve}=requireNode("node:path");
 const {randomUUID,createHash}=requireNode("node:crypto");
 const {readFileSync}=requireNode("node:fs");const {execFileSync}=requireNode("node:child_process");
 const root=mkdtempSync(join(tmpdir(),"native-instrumented-init-"));let closed=false;
 try {
  const scope={tenant:"fixture-"+randomUUID(),workspace:"owned"};
  const config={storage_provider:"helixdb",helixdb:{mode:"embedded",logical_store:"fixture-"+randomUUID(),path:join(root,"native")},storage_path:join(root,"store"),trusted_root:root,scope_policy:"Strict",embedding_provider:{provider_type:"none",model:"none",dimensions:384,prompt_profile:"query"},migration_mode:"Apply",capability_policy:"FailClosed",enable_vector:false,scope};
  const sourceRoot=resolve("../..");
  const fixture={invocation:randomUUID(),sourceRoot,sourceRevision:execFileSync("git",["rev-parse","HEAD"],{encoding:"utf8"}).trim(),artifactSha256:createHash("sha256").update(readFileSync(artifact.path)).digest("hex"),ownedRoot:root};
  const owner=addon.NativeProvider.openInstrumented(JSON.stringify(config),JSON.stringify(fixture));
  expect(JSON.parse(owner.fixtureControlJson(JSON.stringify({operation:"snapshot"}))).binding).toMatchObject({backend:"helixdb",mode:"embedded",scope,store:config.helixdb.logical_store});
  owner.close();owner.close();closed=true;
  const after=createHash("sha256").update(readFileSync(artifact.path)).digest("hex");expect(after).toBe(artifact.digest);console.log(JSON.stringify({artifact:artifact.path,pid:process.pid,shaBefore:artifact.digest,shaAfter:after,instrumentedInitialization:true}));
 } finally {if(closed)rmSync(root,{recursive:true,force:true});else console.error("RETAIN_FAILED_NATIVE_ROOT "+root);}
},600_000);


// Canonical host controls use the ordinary selected embedded artifact; no fault
// profile, forced GC, alternate lexical directory or synthetic provider is used.
async function selectedEmbeddedFixture() {
 const fs=await import("node:fs"),path=await import("node:path"),os=await import("node:os"),crypto=await import("node:crypto");
 // One physical addon per file. Ordinary constructors on this explicit
 // instrumented artifact install no control; normal four-profile coverage is
 // isolated in provider-selection.test.ts child processes.
 const artifact=currentInstrumentedArtifact();
 const addon=createRequire(import.meta.url)(artifact.path);
 const root=fs.mkdtempSync(path.join(os.tmpdir(),"native-host-overlap-"));
 const scope={tenant:"host-"+crypto.randomUUID(),workspace:"overlap"};
 const config={storage_provider:"helixdb",helixdb:{mode:"embedded",logical_store:"host-"+crypto.randomUUID(),path:path.join(root,"native")},storage_path:path.join(root,"store"),trusted_root:root,scope_policy:"Strict",embedding_provider:{provider_type:"none",model:"none",dimensions:384,prompt_profile:"query"},migration_mode:"Apply",capability_policy:"FailClosed",enable_vector:false,scope};
 const actor={id:"host-fixture",kind:"agent"};
 const request=(text:string)=>({kind:"observation",content:{text},scope,requester:{actor,roles:[],permissions:["memory.write","memory.retrieve"]},provenance:{source:"host-fixture",actor,observedAt:new Date().toISOString(),confidence:1},policy:{visibility:"private",retention:"durable",sensitivity:"low",allowedUses:["retrieval"]},links:[],idempotencyKey:crypto.randomUUID()});
 return{addon,artifact,root,scope,config,request,remove:()=>fs.rmSync(root,{recursive:true,force:true})};
}

it("presentation_readonly_selected_provider_overlaps_live_writer",async()=>{
 const f=await selectedEmbeddedFixture();let writer:any,memory:any,reader:any,readMemory:any,passed=false;
 try{
  writer=new f.addon.NativeProvider(JSON.stringify(f.config));memory=writer.requireMemoryApi();
  memory.writeJson(JSON.stringify(f.request("eligible native overlap sentinel")));
  const seed=f.request("seed"),createdAt=new Date().toISOString(),sourceId="overlap-source",documentId="overlap-document",chunkId="overlap-chunk";
  const common={policy:seed.policy,provenance:seed.provenance,createdAt};
  const batch=writer.requireBatchApi();const feed=writer.requireLexicalFeedApi();
  try {
   const outcome=JSON.parse(batch.ingestJson(JSON.stringify({idempotency_key:seed.idempotencyKey,scope:f.scope,source:{...common,id:sourceId,name:"overlap source",kind:"generated",scope:f.scope},documents:[{...common,id:documentId,sourceId,kind:"text",contentHash:"overlap-document-hash"}],chunks:[{...common,id:chunkId,sourceId,documentId,kind:"paragraph",text:"lexicaloverlapneedle authoritative paragraph",contentHash:"overlap-chunk-hash"}],facts:[],entities:[],relationships:[],evidence:[],embeddings:[],source_id:null})));
   expect(outcome.status).toBe("Complete");feed.upsertJson(JSON.stringify({targetId:chunkId,text:"lexicaloverlapneedle authoritative paragraph"}));
  }finally{batch.close();feed.close();}
  expect(()=>{reader=new f.addon.NativeProvider(JSON.stringify({...f.config,helixdb:{...f.config.helixdb,read_only:true}}));}).not.toThrow();
  readMemory=reader.requireMemoryApi();expect(JSON.parse(readMemory.listMemoriesPagedJson(JSON.stringify({scope:f.scope}))).items[0].content.text).toBe("eligible native overlap sentinel");
  const recall=reader.requireRecallApi();try{
   const payload=JSON.parse(recall.recallJson(JSON.stringify({query:"lexicaloverlapneedle",scope:f.scope,requester:seed.requester,modes:["keyword"],limit:10})));
   expect(payload.items.some((item:any)=>item.targetId===chunkId&&item.content.includes("lexicaloverlapneedle"))).toBe(true);
  }finally{recall.close();}
  const binary=process.env.ENGRAM_TASK005_MCP_BIN;if(!binary)throw new Error("BLOCKED: bound current MCP binary required for overlap");
  const {execFileSync}=await import("node:child_process");const {join}=await import("node:path");
  const output=execFileSync(binary,["recall","--query","lexicaloverlapneedle","--json","--backend","helixdb","--helix-mode","embedded","--helix-store",f.config.helixdb.logical_store,"--helix-path",f.config.helixdb.path,"--storage",f.config.storage_path,"--trusted-root",f.root,"--org",f.scope.tenant,"--project",f.scope.workspace,"--no-vector"],{encoding:"utf8"});
  const oneShot=JSON.parse(output.trim());expect(oneShot.ok).toBe(true);expect(JSON.stringify(oneShot.results)).toContain("lexicaloverlapneedle");
  readMemory.close();readMemory=undefined;reader.close();reader=undefined;
  memory.writeJson(JSON.stringify(f.request("writer remains usable after reader")));passed=true;
 }finally{readMemory?.close();reader?.close();memory?.close();writer?.close();if(passed)f.remove();else console.error("RETAIN_FAILED_NATIVE_ROOT "+f.root);}
},600_000);

it("presentation_owned_facade_memory_releases_before_immediate_reopen",async()=>{
 const f=await selectedEmbeddedFixture();const observed:any[]=[];let facade:any,reopened:any,passed=false;
 try{
  const native=new f.addon.NativeProvider(JSON.stringify(f.config));
  // Observe internally-created proxies without allowing nondeterministic GC to
  // conceal ownership. The caller never receives these as retained APIs.
  const observedProvider=new Proxy(native,{get(target,key){if(key==="requireMemoryApi")return()=>{const api=target.requireMemoryApi();observed.push(api);return api;};const value=Reflect.get(target,key);return typeof value==="function"?value.bind(target):value;}});
  const {createNativeProviderTransport}=await import("../src/provider.js");
  facade=createNativeProviderTransport({provider:observedProvider});
  await facade.write(f.request("facade-owned memory sentinel") as any);
  await expect(facade.write({...f.request("denied"),scope:{...f.scope,tenant:"foreign"}} as any)).rejects.toThrow();
  for(const api of observed)expect(()=>api.listMemoriesPagedJson(JSON.stringify({scope:f.scope}))).toThrow(/closed/);
  facade.close();
  expect(()=>{reopened=new f.addon.NativeProvider(JSON.stringify(f.config));}).not.toThrow();
  reopened.close();reopened=undefined;passed=true;
 }finally{for(const api of observed)api.close();facade?.close();reopened?.close();if(passed)f.remove();else console.error("RETAIN_FAILED_NATIVE_ROOT "+f.root);}
},600_000);


it("presentation_selected_vocabulary_mutations_delegate_with_full_parent_scope",async()=>{
 const f=await selectedEmbeddedFixture();Object.assign(f.scope,{subject:"vocabulary",session:"vocabulary-session",environment:"test"});let facade:any,passed=false;
 try{
  const {createNativeProviderTransport}=await import("../src/provider.js");
  facade=createNativeProviderTransport({provider:new f.addon.NativeProvider(JSON.stringify(f.config))});
  const seed=f.request("seed"),common={provenance:seed.provenance,createdAt:new Date().toISOString()};
  const ontology={...common,id:"ontology",uri:"urn:fixture:ontology",name:"selected ontology",scope:f.scope,language:"custom",version:"1",status:"active",policy:seed.policy};
  expect(await facade.putOntology(ontology)).toMatchObject({id:"ontology",scope:f.scope});
  expect(await facade.putClass({...common,id:"class",ontologyId:"ontology",uri:"urn:fixture:class",label:"class",status:"active"})).toMatchObject({id:"class"});
  expect(await facade.putProperty({...common,id:"property",ontologyId:"ontology",uri:"urn:fixture:property",label:"property",kind:"object",domainClassId:"class",rangeClassId:"class",status:"active"})).toMatchObject({id:"property"});
  expect(await facade.putAxiom({...common,id:"axiom",ontologyId:"ontology",kind:"custom",subjectClassId:"class",propertyId:"property",objectClassId:"class"})).toMatchObject({id:"axiom"});
  expect(await facade.getOntology({id:"ontology",scope:f.scope})).toMatchObject({name:"selected ontology",scope:f.scope});
  const scheme={...common,id:"scheme",uri:"urn:fixture:scheme",name:"selected scheme",scope:f.scope,version:"1",policy:seed.policy};
  expect(await facade.putConceptScheme(scheme)).toMatchObject({id:"scheme",scope:f.scope});
  for(const id of ["first","second"])expect(await facade.putConcept({...common,id,uri:"urn:fixture:"+id,schemeId:"scheme",prefLabel:{value:id},status:"active"})).toMatchObject({id});
  expect(await facade.putConceptRelation({...common,id:"relation",schemeId:"scheme",subjectId:"first",predicate:"broader",objectId:"second"})).toMatchObject({id:"relation"});
  expect(await facade.getConceptScheme({id:"scheme",scope:f.scope})).toMatchObject({scope:f.scope});
  expect(await facade.listConcepts({schemeId:"scheme",scope:f.scope})).toHaveLength(2);
  await expect(facade.putOntology({...ontology,id:"foreign-ontology",scope:{...f.scope,subject:"foreign"}})).rejects.toThrow();
  await expect(facade.putConceptScheme({...scheme,id:"foreign-scheme",scope:{...f.scope,environment:"foreign"}})).rejects.toThrow();
  await expect(facade.putClass({...common,id:"foreign-class",ontologyId:"missing-parent",uri:"urn:fixture:foreign",label:"foreign",status:"active"})).rejects.toThrow();
  await expect(facade.putConcept({...common,id:"foreign-concept",uri:"urn:fixture:foreign",schemeId:"missing-parent",prefLabel:{value:"foreign"},status:"active"})).rejects.toThrow();
  facade.close();passed=true;
 }finally{facade?.close();if(passed)f.remove();else console.error("RETAIN_FAILED_NATIVE_ROOT "+f.root);}
},600_000);
