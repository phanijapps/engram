#!/usr/bin/env python3
"""Local evaluation-only OpenAI-compatible bridge for the pinned HF checkpoint.
Requires torch 2.14.1, transformers 5.20.1, sentence-transformers[image] 6.1.0.
Inputs must already contain the task/document prefixes; this server adds none.
It binds only to loopback, uses text-only FP32 CUDA inference and never loads
remote Python code. It does not provide a production model-serving service.
"""
import argparse
import importlib.metadata
import json
from pathlib import Path
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np
import torch
from sentence_transformers import SentenceTransformer

REVISION = '914f7f89142e33e77833254d9c9b90c3cef7303b'
MODEL = f'google/embeddinggemma-2@{REVISION}'


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--checkpoint', type=Path, required=True)
    ap.add_argument('--ready-file', type=Path, required=True)
    args = ap.parse_args()
    assert torch.cuda.is_available(), 'CUDA is required for this evaluation profile'
    torch.set_num_threads(8)
    torch.set_float32_matmul_precision('highest')
    start = time.monotonic()
    model = SentenceTransformer(
        str(args.checkpoint), device='cuda', local_files_only=True,
        trust_remote_code=False, config_kwargs={'vision_config':None,'audio_config':None},
        model_kwargs={'dtype':torch.float32,'attn_implementation':'sdpa'},
    )
    model.max_seq_length = 8192
    model.eval()
    lock = threading.Lock()
    state = {'model':MODEL,'revision':REVISION,'device':torch.cuda.get_device_name(),
             'dtype':str(model.dtype),'dimensions':model.get_embedding_dimension(),
             'maxSequenceLength':model.max_seq_length,'encoders':'text-only',
             'versions':{n:importlib.metadata.version(n) for n in ['torch','transformers','sentence-transformers']},
             'loadSeconds':time.monotonic()-start,'requests':0,'texts':0}

    def encode(texts, dimensions=768):
        assert dimensions in [128,256,512,768]
        with lock, torch.inference_mode():
            values = model.encode(texts,prompt='',batch_size=8,show_progress_bar=False,
                                  normalize_embeddings=True,truncate_dim=dimensions,convert_to_numpy=True)
        assert values.shape == (len(texts),dimensions) and np.isfinite(values).all()
        assert np.allclose(np.linalg.norm(values,axis=1),1.0,atol=1e-5)
        return values

    query = 'task: code retrieval | query: Find a function that retrieves a user by identifier'
    passage = 'title: none | text: function getUser(id) { return db.find(id); }'
    batches = encode([query,passage])
    singles = np.stack([encode([text])[0] for text in [query,passage]])
    assert np.allclose(batches,singles,atol=1e-5),float(np.max(np.abs(batches-singles)))
    assert not np.allclose(batches[0],batches[1])
    state['smoke'] = {'finite':True,'normalized':True,'dimensions':768,
                      'batchSingleMaxAbsoluteError':float(np.max(np.abs(batches-singles))),
                      'queryPassageCosine':float(batches[0] @ batches[1])}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*args):
            pass

        def reply(self,code,value):
            raw=json.dumps(value).encode()
            self.send_response(code)
            self.send_header('Content-Type','application/json')
            self.send_header('Content-Length',str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)

        def do_GET(self):
            self.reply(200,state) if self.path=='/health' else self.reply(404,{'error':'not found'})

        def do_POST(self):
            if self.path!='/v1/embeddings':
                return self.reply(404,{'error':'not found'})
            count=int(self.headers.get('Content-Length','0'))
            if not 0<count<=4*1024*1024:
                return self.reply(400,{'error':'invalid size'})
            body=json.loads(self.rfile.read(count))
            texts=body.get('input')
            if (body.get('model')!=MODEL or not isinstance(texts,list) or not 1<=len(texts)<=256
                    or not all(isinstance(t,str) and len(t.encode())<=1024*1024 for t in texts)):
                return self.reply(400,{'error':'invalid embedding request'})
            dimensions=body.get('dimensions',768)
            if dimensions not in [128,256,512,768]:
                return self.reply(400,{'error':'unsupported dimension'})
            start=time.monotonic()
            try:
                vectors=encode(texts,dimensions)
            except Exception as error:
                print('inference failure',type(error).__name__,flush=True)
                return self.reply(500,{'error':'inference failed'})
            state['requests']+=1
            state['texts']+=len(texts)
            print('embedded',len(texts),'in',round(time.monotonic()-start,3),'seconds',flush=True)
            self.reply(200,{'object':'list','model':MODEL,
                           'data':[{'object':'embedding','index':i,'embedding':v.tolist()} for i,v in enumerate(vectors)]})

    server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
    state['endpoint']=f'http://127.0.0.1:{server.server_port}/v1'
    args.ready_file.write_text(json.dumps(state,indent=2)+'\n')
    print('READY',json.dumps(state),flush=True)
    try:
        server.serve_forever()
    finally:
        server.server_close()


if __name__=='__main__':
    main()
