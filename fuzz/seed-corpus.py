#!/usr/bin/env python3
"""Generate deterministic, non-secret archive seeds; preserve discovered corpus files."""
from pathlib import Path
import hashlib,json
root=Path(__file__).resolve().parent

def header(path,size):
 b=bytearray(512); name=path; prefix=''
 if len(path)>100: prefix,name=path.rsplit('/',1)
 b[:len(name)]=name.encode(); b[345:345+len(prefix)]=prefix.encode()
 for offset,width,value in [(100,8,0o644),(108,8,0),(116,8,0),(124,12,size),(136,12,0)]:
  b[offset:offset+width]=(f'{value:0{width-1}o}'+chr(0)).encode()
 b[148:156]=b' '*8; b[156]=ord('0'); b[257:265]=b'ustar'+bytes([0])+b'00'
 b[148:156]=(f'{sum(b):07o}'+chr(0)).encode(); return bytes(b)

def archive(length,long_path,adapter=0):
 body=b'x'*length; path=('p'*110+'/file') if long_path else 'data/file'
 manifest={'schema_version':1,'plugin':{'id':'example.fuzz','version':'1.0.0','host_api':'^1'},'kind':{'type':'data'},'files':[{'path':path,'size':len(body),'sha256':hashlib.sha256(body).hexdigest()}],'budgets':{'memory_bytes':1024,'concurrent_calls':1,'call_timeout_ms':1000,'storage_bytes':0,'event_queue_capacity':0}}
 if adapter: manifest['kind']={'type':'native','target':'aarch64-apple-darwin','entrypoint':path} if adapter==1 else {'type':'wasm','entrypoint':path}
 descriptor=json.dumps(manifest,separators=(',',':'),sort_keys=True).encode()
 out=bytearray()
 for name,data in [('package.json',descriptor),(path,body)]:
  out.extend(header(name,len(data))); out.extend(data); out.extend(bytes((-len(data))%512))
 out.extend(bytes(1024)); return bytes(out)

raw=root/'corpus/archive_bytes'; structured=root/'corpus/archive_structured'
raw.mkdir(parents=True,exist_ok=True); structured.mkdir(parents=True,exist_ok=True)
for index,length in enumerate([0,1,511,512,513,65535,65536,65537]):
 for long_path in [False,True]:
  for adapter in range(3):
   if adapter and length==0: continue
   (raw/f'valid-{adapter}-{length}-{int(long_path)}').write_bytes(archive(length,long_path,adapter))
 for selector in range(20):
  for adapter in range(3): (structured/f'mode-{selector}-{index}-{adapter}').write_bytes(bytes([selector,index,index%2,adapter]))
valid=archive(1,False)
(raw/'truncated-header').write_bytes(valid[:511]); (raw/'truncated-body').write_bytes(valid[:700]); (raw/'single-zero').write_bytes(valid[:-512]); (raw/'extra-zero').write_bytes(valid+bytes(512)); (raw/'trailer').write_bytes(valid+b'x')
print('Prepared44 valid +5 incomplete/extra-data raw seeds and480 structured selector seeds across all adapter kinds.')
