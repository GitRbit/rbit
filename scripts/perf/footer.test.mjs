import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import http from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import test from "node:test";
import { requestSample, validateConfig } from "./measure.mjs";
import { footerSsrMs, renderHeaderMs, viewHeaderMs } from "./http-timing.mjs";
import { serverEnvironment } from "./server-environment.mjs";

const footer = value => `<span data-render-time>${value}</span>`;
test("footer and total-header parsers reject ambiguity and keep distinct units", () => {
  for (const [value, expected] of [["412 µs",0.412],["500 μs",0.5],["500 &#181;s",0.5],["500 &micro;s",0.5],["3.1 ms",3.1],["1.20 s",1200]]) assert.equal(footerSsrMs(footer(value)),expected);
  for (const value of ["",footer("NaN ms"),footer("-1 ms"),footer("1.2.3 ms"),footer("1 ms")+footer("2 ms")]) assert.equal(footerSsrMs(value),null);
  assert.equal(renderHeaderMs("database;dur=1, render;dur=2.125"),2.125);
  for (const value of ["",null,"render;dur=NaN","render;dur=-1","render;dur=1.2.3","render;dur=1, render;dur=2","render;dur=invalid, render;dur=2"]) assert.equal(renderHeaderMs(value),null);
});

async function server(t, handler) {
  const instance=http.createServer(handler);
  await new Promise(done=>instance.listen(0,"127.0.0.1",done));
  t.after(()=> { instance.closeAllConnections(); return new Promise(done=>instance.close(done)); });
  return `http://127.0.0.1:${instance.address().port}`;
}

test("streamed footer beyond the prefix and split UTF8 remains a raw SSR metric", async t=> {
  const html=`<main id="main">${"x".repeat(70000)}</main>${footer("412 µs")}`;
  const bytes=Buffer.from(html); const cut=bytes.indexOf(Buffer.from("µ"))+1;
  const base=await server(t,(_request,response)=> {
    response.writeHead(200,{"Content-Type":"text/html","Server-Timing":"render;dur=2.5"});
    response.write(bytes.subarray(0,cut)); setImmediate(()=>response.end(bytes.subarray(cut)));
  });
  const sample=await requestSample(base, {requireFooter:true,requireTiming:true},{timeoutMs:1000,maxBytes:100000});
  assert.equal(sample.valid,true);
  assert.equal(sample.footerSsrMs,0.412); assert.equal(sample.renderMs,2.5);
  assert.equal(sample.bodyBytes,bytes.length);
});

test("304 contains current header timing but no current SSR footer, and missing metrics invalidate strict cases",async t=> {
  const base=await server(t,(request,response)=> {
    response.writeHead(request.headers["if-none-match"] ? 304 : 200,{"Content-Type":"text/html",ETag:'W/"fixture"',"Server-Timing":"render;dur=3.25"});
    response.end(request.headers["if-none-match"] ? undefined : `<main id="main">fixture</main>${footer("500 µs")}`);
  });
  const options={timeoutMs:1000,maxBytes:1024};
  const initial=await requestSample(base,{requireTiming:true,requireFooter:true},options);
  const conditional=await requestSample(base,{mode:"revalidate",requireFooter:true,requireTiming:true},{...options,ifNoneMatch:initial.responseHeaders.etag});
  assert.equal(conditional.valid,true); assert.equal(conditional.status,304); assert.equal(conditional.bodyBytes,0);
  assert.equal(conditional.footerSsrMs,null); assert.equal(conditional.renderMs,3.25);
  const missing=await server(t,(_request,response)=> {response.writeHead(200,{"Content-Type":"text/html"});response.end('<main id="main">fixture</main>');});
  assert.equal((await requestSample(missing,{requireFooter:true,requireTiming:true},options)).valid,false);
});

test("paired authenticated CLI keeps passwords/cookies private and records revalidation primes separately",async t=> {
  const directory=await mkdtemp(join(tmpdir(),"rbit-footer-fixture-"));
  t.after(()=>rm(directory,{recursive:true,force:true}));
  const password="private-password-sentinel",cookie="private-cookie-sentinel";
  let logins=0;
  const base=await server(t,(request,response)=> {
    if(request.url==="/login") {
      let body="";request.on("data",chunk=>body+=chunk);request.on("end",()=> {
        assert.equal(new URLSearchParams(body).get("password"),password);logins++;
        response.writeHead(303,{Location:"/","Set-Cookie":`rbit_session=${cookie}; HttpOnly; Path=/`});response.end();
      });return;
    }
    assert.equal(request.headers.cookie,`rbit_session=${cookie}`);
    const conditional=Boolean(request.headers["if-none-match"]);
    response.writeHead(conditional ? 304 : 200,{"Content-Type":"text/html",ETag:'W/"fixture"',"Server-Timing":"render;dur=2, view;dur=0.500"});
    response.end(conditional ? undefined : `<main id="main">fixture</main>${footer("500 µs")}`);
  });
  await writeFile(join(directory,"private-credentials.txt"),`perf:${password}\n`);
  const variants=["before","after"].map(name=>({name,baseUrl:base,metadata:`${name}.json`,credentialsFile:"private-credentials.txt"}));
  for(const variant of variants) await writeFile(join(directory,variant.metadata),JSON.stringify({baseUrl:base,profile:"release",executableSha256:"synthetic-only"}));
  const config={variants,rounds:3,warmups:1,fixture:directory,output:"report.json",notes:"synthetic protocol fixture, not performance evidence",pages:[{name:"conditional",path:"/",mode:"revalidate",requireTiming:true,requireFooter:true}]};
  const input=join(directory,"config.json");await writeFile(input,JSON.stringify(config));
  await promisify(execFile)(process.execPath,[join(import.meta.dirname,"measure.mjs"),input]);
  assert.equal(logins,2);
  const text=await readFile(join(directory,"report.json"),"utf8");const report=JSON.parse(text);
  const raw=await readFile(join(directory,"report.json.jsonl"),"utf8");
  for(const secret of [password,cookie,"private-credentials.txt"]) {assert.ok(!text.includes(secret));assert.ok(!raw.includes(secret));}
  assert.equal(report.valid,true);assert.equal(report.samples.filter(sample=>sample.phase==="cache-prime").length,2);
  assert.equal(report.summary[0].renderMs.count,3);assert.equal(report.summary[0].renderMs.median,2);
  assert.equal(report.summary[0].viewMs.count,3);assert.equal(report.summary[0].viewMs.median,0.5);
  assert.equal(report.summary[0].footerSsrMs.count,0);
  assert.equal(report.summary[0].bodyBytes.median,0);
  assert.throws(()=>validateConfig({...config,password}),/never embed/);
  assert.throws(()=>validateConfig({...config,variants:[{...variants[0],Cookie:cookie},variants[1]]}),/unknown variant/);
});


test("isolated launcher refuses private configs and ignores inherited application settings",async t=> {
  const directory=await mkdtemp(join(tmpdir(),"rbit-launch-settings-"));
  t.after(()=>rm(directory,{recursive:true,force:true}));
  const setup=join(directory,"setup-token.txt");await writeFile(setup,"private-setup-sentinel");
  const options={isolated:true,sourceDir:directory,port:3305,baseUrl:"http://127.0.0.1:3305",dataDir:join(directory,"data")};
  const env=serverEnvironment(options,{PATH:"retained",RBIT_PERF_SETUP_TOKEN_FILE:setup,RBIT_MAIL__PASSWORD:"user-mail-secret",RBIT_SERVER__BIND:"127.0.0.1:3100",RBIT_STORAGE__DATA_DIR:"user-data",RBIT_SSH__ENABLED:"true"});
  assert.equal(env.PATH,"retained");assert.equal(env.RBIT_MAIL__PASSWORD,undefined);
  assert.equal(env.RBIT_SECURITY__SETUP_TOKEN,"private-setup-sentinel");
  assert.equal(env.RBIT_SERVER__BIND,"127.0.0.1:3305");assert.equal(env.RBIT_STORAGE__DATA_DIR,options.dataDir);
  assert.equal(env.RBIT_SSH__ENABLED,"false");assert.equal(env.RBIT_UPDATES__CHECK_FOR_UPDATES,"false");
  assert.equal(env.RBIT_PERF_SETUP_TOKEN_FILE,undefined);
  await writeFile(join(directory,"rbit.toml"),"private configuration");
  assert.throws(()=>serverEnvironment(options,{}),/private application config/);
});


test("view is an independent footer interval and never falls back to total render",async t=> {
  assert.equal(viewHeaderMs("render;dur=9, view;dur=0.500"),0.5);
  assert.equal(renderHeaderMs("view;dur=0.500, render;dur=9"),9);
  for(const value of ["render;dur=9","view;dur=NaN","view;dur=1.2.3","view;dur=1, view;dur=2","view;dur=invalid, view;dur=2"]) assert.equal(viewHeaderMs(value),null);
  const base=await server(t,(request,response)=> {
    const candidate=request.url==="/candidate";
    const revalidated=Boolean(request.headers["if-none-match"]);
    response.writeHead(revalidated ? 304 : 200,{"Content-Type":"text/html",ETag:'W/"fixture"',"Server-Timing":candidate ? "render;dur=9, view;dur=0.500" : "render;dur=9"});
    response.end(revalidated ? undefined : `<main id="main">fixture</main>${footer("500 µs")}`);
  });
  const options={timeoutMs:1000,maxBytes:1024};
  const baseline=await requestSample(base+"/baseline",{requireFooter:true,requireTiming:true},options);
  assert.equal(baseline.valid,true);assert.equal(baseline.viewMs,null);assert.equal(baseline.footerSsrMs,0.5);assert.equal(baseline.renderMs,9);
  const absent=await requestSample(base+"/baseline",{requireFooter:true,requireTiming:true},{...options,requireViewTiming:true});
  assert.equal(absent.valid,false);
  const candidate=await requestSample(base+"/candidate",{requireFooter:true,requireTiming:true},{...options,requireViewTiming:true});
  assert.equal(candidate.valid,true);assert.equal(candidate.viewMs,0.5);assert.equal(candidate.renderMs,9);assert.equal(candidate.footerSsrMs,0.5);
  const conditional=await requestSample(base+"/candidate",{mode:"revalidate",requireFooter:true,requireTiming:true},{...options,requireViewTiming:true,ifNoneMatch:candidate.responseHeaders.etag});
  assert.equal(conditional.valid,true);assert.equal(conditional.footerSsrMs,null);assert.equal(conditional.viewMs,0.5);assert.equal(conditional.renderMs,9);
});
