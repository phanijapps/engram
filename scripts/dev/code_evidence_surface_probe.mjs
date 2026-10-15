#!/usr/bin/env node
// Actual native + HTTP calls over one provider. No labels or answer anchors.
import {createRequire} from "node:module";
import {createNativeProviderTransport} from "../../packages/node/dist/index.js";
import {startMcpHttpServer} from "../../packages/runtime/dist/index.js";
const require=createRequire(new URL("../../packages/runtime/package.json",import.meta.url));
const {Client,StreamableHTTPClientTransport}=await import(require.resolve("@modelcontextprotocol/client"));
const [storage,project]=process.argv.slice(2);
if (!storage || !project || process.argv.length!==4) throw new Error("usage: code_evidence_surface_probe.mjs STORE PROJECT");
const transport=createNativeProviderTransport({configJson:JSON.stringify({
  storage_path:storage,trusted_root:storage,scope_policy:"Strict",sqlite_storage_layout:{kind:"single_file",file_name:"engram_data.db"},
  embedding_provider:{provider_type:"fastembed",model:"BAAI/bge-small-en-v1.5",dimensions:384,prompt_profile:"query"},
  migration_mode:"DryRun",capability_policy:"FailClosed",enable_vector:true,
})});
const context={scope:{tenant:"default",workspace:project},requester:{actor:{id:"engram-mcp",kind:"agent",displayName:"engram-mcp"}},readerVersion:1};
const query={query:"register a URL rule and endpoint on the Flask application",resultLimit:10};
const capabilities=await transport.capabilities();
const configuration=await transport.codeConfiguration();
let start=performance.now();
const native=await transport.codeEvidence(query,context);
const nativeSeconds=(performance.now()-start)/1000;
const server=await startMcpHttpServer({transport,sourceContext:context,port:0});
const client=new Client({name:"code-evidence-parity-probe",version:"1"});
let http;
try {
  await client.connect(new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${server.address().port}/mcp`)));
  start=performance.now();
  const result=await client.callTool({name:"code",arguments:{op:"evidence",...query}});
  if(result.isError) throw new Error(JSON.stringify(result));
  http={packet:JSON.parse(result.content.find(c=>c.type==="text").text),seconds:(performance.now()-start)/1000};
} finally {await client.close();await new Promise(resolve=>server.close(resolve));}
console.log(JSON.stringify({capabilities,configuration,native:{packet:native,seconds:nativeSeconds},http}));
