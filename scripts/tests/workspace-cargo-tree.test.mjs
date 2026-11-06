import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import { leased, acquire, UncertainProcessTreeError } from "../workspace/lease.mjs";
import { cargo, cargoProcess } from "../workspace/build.mjs";
import { cargoTreeMode, ownedCargo, verifiedCargoCompletion } from "../workspace/cargo-tree.mjs";
const tick = () => new Promise(resolve => setImmediate(resolve));
async function fixture(t) {
 const root = await mkdtemp(join(tmpdir(),"rbit-cargo-ownership-"));
 t.after(()=>rm(root,{recursive:true,force:true}));
 const ctx={root,checkout:root,state:join(root,"state"),cache:join(root,"cache")};
 await mkdir(ctx.cache);
 return ctx;
}
function fake() {
 const child=new EventEmitter(); child.pid=1234;
 child.launched=new Promise(resolve=>{child.markLaunched=resolve;});
 child.stdout=new PassThrough(); child.stderr=new PassThrough();
 return child;
}
function run(ctx,lease,child,native={},options={}) {
 return ownedCargo(ctx,["test","--message-format=json"],{},()=>{}, {
  lease,native:{resolve:async()=>"cargo.exe",launch:()=>{child.markLaunched();return child;},verify:async()=>true,...native},
  output:{write:()=>{}},diagnostics:{write:()=>{}},...options,
 },(ctx,args,env,artifact,stream)=>cargoProcess(ctx,args,env,artifact,stream));
}
async function close(child,code=0,signal=null) { await child.launched;child.stdout.end();child.stderr.end();await tick();child.emit("close",code,signal); }

test("strict ownership is explicit and unsupported platforms never fall back",()=>{
 assert.equal(cargoTreeMode({}),"off");assert.equal(cargoTreeMode({RBIT_WORKSPACE_CARGO_TREE:"off"},"linux"),"off");
 assert.equal(cargoTreeMode({RBIT_WORKSPACE_CARGO_TREE:"windows-job"},"win32"),"windows-job");
 for(const value of ["1","true","auto",""]) assert.throws(()=>cargoTreeMode({RBIT_WORKSPACE_CARGO_TREE:value}),/must be/);
 assert.throws(()=>cargoTreeMode({RBIT_WORKSPACE_CARGO_TREE:"windows-job"},"linux"),/unsupported/);
 assert.equal(verifiedCargoCompletion({method:"windows-job",treeStopped:true},{token:"fake"}),false);
});
test("real lease is mandatory before any owned launch",async t=>{
 const ctx=await fixture(t);let launched=false;
 await assert.rejects(run(ctx,{path:join(ctx.cache,".lease"),token:"forged"},fake(),{launch:()=>{launched=true;}}),/management lease/);
 assert.equal(launched,false);
});
test("close waits for verified tree drain and final JSON before receipt or lease release",async t=>{
 const ctx=await fixture(t),child=fake();let verifyEntered,releaseProof,settled=false,lines="";
 const entered=new Promise(resolve=>{verifyEntered=resolve;});const proof=new Promise(resolve=>{releaseProof=resolve;});
 const running=leased(join(ctx.cache,".lease"),async lease=>{
  const completed=await run(ctx,lease,child,{verify:async()=>{verifyEntered();await proof;return true;}},{output:{write:value=>{lines+=value;}}});
  assert.equal(verifiedCargoCompletion(completed,lease),true);
  assert.equal(verifiedCargoCompletion(completed,{token:"another"}),false);
  assert.equal(verifiedCargoCompletion(JSON.parse(JSON.stringify(completed)),lease),false);
  return completed;
 }).then(value=>{settled=true;return value;});
 await tick();await tick();
 child.stdout.write('ordinary test output\n');await close(child);
 await entered;assert.equal(settled,false);
 await assert.rejects(acquire(join(ctx.cache,".lease"),{timeout:0}),/Timed out/);
 releaseProof();assert.equal((await running).treeStopped,true);assert.equal(lines,"ordinary test output\n");
 const release=await acquire(join(ctx.cache,".lease"),{timeout:0});await release();
});
test("missing proof overrides normal exit, callback errors and keeps actual lease blocked",async t=>{
 const ctx=await fixture(t),child=fake();
 const running=leased(join(ctx.cache,".lease"),lease=>run(ctx,lease,child,{verify:async()=>false},{onMessage:()=>{throw new Error("callback failure");}}));
 const rejected=assert.rejects(running,error=>error instanceof UncertainProcessTreeError && !!error.control);
 await tick();await tick();child.stdout.write('{"reason":"build-finished"}\n');await close(child);await rejected;
 assert.equal(JSON.parse(await readFile(join(ctx.cache,".lease","blocked.json"),"utf8")).reason.includes("unverified"),true);
 await assert.rejects(acquire(join(ctx.cache,".lease"),{timeout:0}),/Blocked lease/);
 assert.equal((await readdir(join(ctx.state,"cargo"))).length,1);
});
test("proof read failure retains uncertainty ahead of ordinary compiler failure",async t=>{
 const ctx=await fixture(t),child=fake();
 const running=leased(join(ctx.cache,".lease"),lease=>run(ctx,lease,child,{verify:async()=>{throw new Error("read failed");}}));
 const rejected=assert.rejects(running,UncertainProcessTreeError);
 await tick();await tick();await close(child,101);await rejected;
 await assert.rejects(acquire(join(ctx.cache,".lease"),{timeout:0}),/Blocked lease/);
});
test("proved compiler failures and parser/output failures release only after drain",async t=>{
 for(const kind of ["exit","output","callback","stderr"]){
  const ctx=await fixture(t),child=fake();let verified=false;
  const options=kind==="output"?{output:{write:()=>{throw new Error("output failure");}}}
   :kind==="callback"?{onMessage:()=>{throw new Error("callback failure");}}
   :kind==="stderr"?{captureStderr:true,diagnostics:{write:()=>{throw new Error("stderr failure");}}}:{};
  const running=leased(join(ctx.cache,".lease"),lease=>run(ctx,lease,child,{verify:async()=>{verified=true;return true;}},options));
  const rejected=assert.rejects(running,kind==="exit"?/exited 101/:new RegExp(`${kind} failure`));
  await tick();await tick();
  if(kind==="callback")child.stdout.write('{"reason":"build-finished"}\n');
  if(kind==="output")child.stdout.write('plain output\n');
  if(kind==="stderr")child.stderr.write("diagnostic");
  await close(child,kind==="exit"?101:0);await rejected;assert.equal(verified,true);
  const release=await acquire(join(ctx.cache,".lease"),{timeout:0});await release();
 }
});
test("launch outcome failure fails closed and control/lease evidence remain",async t=>{
 const ctx=await fixture(t);
 await assert.rejects(leased(join(ctx.cache,".lease"),lease=>run(ctx,lease,fake(),{launch:()=>{throw new Error("unexpected launch outcome");}})),UncertainProcessTreeError);
 await assert.rejects(acquire(join(ctx.cache,".lease"),{timeout:0}),/Blocked lease/);
});
test("verified abnormal wrapper exit is an ordinary command error with no weak PID heuristic",async t=>{
 const ctx=await fixture(t),child=fake();
 const running=leased(join(ctx.cache,".lease"),lease=>run(ctx,lease,child));
 const rejected=assert.rejects(running,error=>!(error instanceof UncertainProcessTreeError)&&/exited 125/.test(error.message));
 await tick();await tick();await close(child,125);await rejected;
 const release=await acquire(join(ctx.cache,".lease"),{timeout:0});await release();
});

test("unterminated final JSON is decoded before proof settlement and receipt publication",async t=>{
 const ctx=await fixture(t),child=fake();let observed=false;
 const running=leased(join(ctx.cache,".lease"),lease=>run(ctx,lease,child,{verify:async()=>{assert.equal(observed,true);return true;}},{onMessage:message=>{assert.equal(message.reason,"build-finished");observed=true;}}));
 child.stdout.write('{"reason":"build-finished"}');await close(child);await running;assert.equal(observed,true);
});
test("native launch preserves actual command arguments and pipe/environment options",async t=>{
 const ctx=await fixture(t),child=fake();let launched;
 const running=leased(join(ctx.cache,".lease"),lease=>run(ctx,lease,child,{launch:(executable,args,options)=>{
  launched={executable,args,options};child.markLaunched();return child;
 }}));
 await child.launched;
 assert.equal(launched.executable,"cargo.exe");
 assert.deepEqual(launched.args,["test","--message-format=json"]);
 assert.deepEqual(launched.options.stdio,["inherit","pipe","inherit"]);
 assert.equal(launched.options.cwd,ctx.checkout);
 assert.deepEqual(launched.options.env,{});
 assert.equal(launched.options.stopFile.startsWith(join(ctx.state,"cargo")),true);
 await close(child);await running;
});

test("failure to persist blocked diagnostics cannot replace original uncertainty or release ownership",async t=>{
 const ctx=await fixture(t),child=fake();
 const running=leased(join(ctx.cache,".lease"),lease=>run(ctx,lease,child,{verify:async()=>{
  await mkdir(join(ctx.cache,".lease","blocked.json"));return false;
 }}));
 const rejected=assert.rejects(running,error=>error instanceof UncertainProcessTreeError && !!error.blockedRecordError);
 await close(child);await rejected;
 assert.equal(JSON.parse(await readFile(join(ctx.cache,".lease","owner.json"),"utf8")).pid,process.pid);
});

test("pipe read errors are retained until owned completion rather than escaping the parser",async t=>{
 const ctx=await fixture(t),child=fake();let proved=false;
 const running=leased(join(ctx.cache,".lease"),lease=>run(ctx,lease,child,{verify:async()=>{proved=true;return true;}}));
 const rejected=assert.rejects(running,/pipe read failed/);
 await child.launched;child.stdout.emit('error',new Error('pipe read failed'));
 assert.equal(proved,false);await close(child);await rejected;assert.equal(proved,true);
 const release=await acquire(join(ctx.cache,".lease"),{timeout:0});await release();
});

test("actual helper cannot publish an exact executable until owned completion is proved",async t=>{
 const ctx=await fixture(t);let enteredProof,releaseProof;
 const entered=new Promise(resolve=>{enteredProof=resolve;});const gate=new Promise(resolve=>{releaseProof=resolve;});
 const pending=cargo(ctx,'build',[],{key:'a'.repeat(20),env:{RBIT_BUILD_CACHE_MAX_BYTES:'0'},execute:async(ctx,args,env,artifact,control)=>{
  await mkdir(env.CARGO_TARGET_DIR,{recursive:true});const executable=join(env.CARGO_TARGET_DIR,'rbit.exe');await writeFile(executable,'exact fresh binary');
  const child=fake();
  const completed=ownedCargo(ctx,args,env,artifact,{...control,native:{resolve:async()=> 'cargo.exe',launch:()=>{child.markLaunched();return child;},verify:async()=>{enteredProof();await gate;return true;}},output:{write:()=>{}},diagnostics:{write:()=>{}}},
   (ctx,args,env,artifact,stream)=>cargoProcess(ctx,args,env,artifact,stream));
  await child.launched;child.stdout.write(JSON.stringify({reason:'compiler-artifact',executable,target:{kind:['bin'],name:'rbit'}}));await close(child);return completed;
 }});
 await entered;
 await assert.rejects(readdir(join(ctx.state,'bin')),error=>error.code==='ENOENT');
 await assert.rejects(acquire(join(ctx.cache,'.lease'),{timeout:0}),/Timed out/);
 releaseProof();const result=await pending;
 assert.equal(result.completion.method,'windows-job');
 assert.equal(await readFile(result.executables[0].path,'utf8'),'exact fresh binary');
 assert.notEqual(result.executables[0].path,join(ctx.cache,'a'.repeat(20),'shared','target','rbit.exe'));
});

test("unbranded receipts and absent or malformed leases never compare as completion proof",()=>{
 for(const receipt of [{},Object.freeze({method:'windows-job',treeStopped:true}),null,undefined,0,'receipt']) {
  for(const lease of [undefined,null,{}, {token:undefined}, {token:null}, {token:''}, {token:123}, {token:'foreign'}]) {
   assert.equal(verifiedCargoCompletion(receipt,lease),false);
  }
 }
});
test("a real branded receipt cannot be replayed against another real owned lease",async t=>{
 const ctx=await fixture(t);let oldReceipt;
 for(let index=0;index<2;index++) {
  await leased(join(ctx.cache,'.lease'),async lease=>{
   const child=fake();const pending=run(ctx,lease,child);await close(child);const receipt=await pending;
   assert.equal(verifiedCargoCompletion(receipt,lease),true);
   for(const malformed of [undefined,null,{}, {token:undefined}, {token:null}, {token:''}, {token:123}]) {
    assert.equal(verifiedCargoCompletion(receipt,malformed),false);
   }
   if(oldReceipt)assert.equal(verifiedCargoCompletion(oldReceipt,lease),false);
   oldReceipt=receipt;
  });
 }
});
