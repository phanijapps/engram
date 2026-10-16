#!/usr/bin/env python3
"""Explicitly provision admitted code models; normal runtime never downloads.

Choose --from-cache for an existing HF cache root, or --download for the pinned
public artifacts. The model/manifest/URLs are an installed admission list.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import tempfile
import urllib.parse
import urllib.request

MODELS = {'bge-small-en-v1.5':'Xenova/bge-small-en-v1.5',
          'jina-reranker-v1-turbo-en':'jinaai/jina-reranker-v1-turbo-en'}
HOSTS = {'huggingface.co','cdn-lfs.huggingface.co','cas-bridge.xethub.hf.co',
         'cas-bridge.xethub-eu.hf.co',
         'us.aws.cdn.hf.co','cdn-lfs.hf.co','cdn-lfs-us-1.hf.co','cdn-lfs-eu-1.hf.co'}
MANIFESTS = Path(__file__).resolve().parents[2] / 'adapters/models/local-assets/manifests'


def checked_url(url):
    parsed=urllib.parse.urlsplit(url)
    if parsed.scheme!='https' or parsed.hostname not in HOSTS or parsed.username is not None or parsed.password is not None or parsed.port not in (None,443):
        raise ValueError('model artifact URL/redirect is outside admitted HTTPS sources')
    return url


class AdmittedRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,req,fp,code,msg,headers,newurl):
        checked_url(newurl)
        return super().redirect_request(req,fp,code,msg,headers,newurl)


def manifest(model):
    data=json.loads((MANIFESTS/(model+'.json')).read_text())
    if data['schemaVersion']!=1 or data['artifactRepository']!=MODELS[model] or not re.fullmatch('[0-9a-f]{40}',data['revision']):
        raise ValueError('invalid admitted model manifest')
    paths=set()
    if not 1<=len(data['assets'])<=16: raise ValueError('invalid model asset count')
    for asset in data['assets']:
        path=asset['path']
        if not path or path.startswith('/') or '\\' in path or ':' in path or any(p in ('','.','..') for p in path.split('/')) or path in paths:
            raise ValueError('invalid model asset path')
        paths.add(path)
        if not 0<asset['bytes']<=1_000_000_000 or not re.fullmatch('[0-9a-f]{64}',asset['sha256']):
            raise ValueError('invalid model asset size/hash')
    if not set(data['externalData'])<=paths: raise ValueError('undeclared external model data')
    return data


def confined(path,root):
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError('model asset path escapes operator cache')


def open_regular(path,root):
    path=path.resolve(strict=True)
    confined(path,root)
    if not stat.S_ISREG(path.stat().st_mode): raise ValueError('model asset must be a regular file')
    descriptor=os.open(path,os.O_RDONLY|getattr(os,'O_NONBLOCK',0)|getattr(os,'O_NOFOLLOW',0))
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode): raise ValueError('model asset must be a regular file')
        return os.fdopen(descriptor,'rb')
    except BaseException:
        os.close(descriptor)
        raise


def write_verified(source,destination,asset,root):
    confined(destination.parent,root)
    destination.parent.mkdir(parents=True,exist_ok=True)
    confined(destination.parent,root)
    if destination.is_symlink(): raise ValueError('model asset destination is a symlink')
    temporary=None
    try:
        with tempfile.NamedTemporaryFile(prefix='.provision-',dir=destination.parent,delete=False) as target:
            temporary=Path(target.name)
            digest=hashlib.sha256(); count=0
            while True:
                block=source.read(min(1024*1024,asset['bytes']+1-count))
                if not block: break
                count+=len(block)
                if count>asset['bytes']: raise ValueError('model asset exceeds declared size')
                digest.update(block); target.write(block)
            if count!=asset['bytes'] or digest.hexdigest()!=asset['sha256']:
                raise ValueError('model asset size/digest mismatch')
            target.flush(); os.fsync(target.fileno())
        confined(destination.parent,root)
        os.replace(temporary,destination)
    finally:
        if temporary is not None: temporary.unlink(missing_ok=True)


def valid_existing(path,asset,root):
    if not path.exists(): return False
    confined(path,root)
    if path.is_symlink() or not path.is_file() or path.stat().st_size!=asset['bytes']: return False
    digest=hashlib.sha256()
    with open_regular(path,root) as source:
        for block in iter(lambda:source.read(1024*1024),b''): digest.update(block)
    return digest.hexdigest()==asset['sha256']


def main():
    configured_cache=os.environ.get('ENGRAM_MODEL_CACHE')
    if configured_cache is not None and not configured_cache:
        raise ValueError('ENGRAM_MODEL_CACHE must be a nonempty directory')
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--model',choices=list(MODELS),required=True)
    parser.add_argument('--cache',type=Path,default=Path(configured_cache) if configured_cache else Path.home()/'.cache/engram/models')
    mode=parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--from-cache',type=Path)
    mode.add_argument('--download',action='store_true')
    args=parser.parse_args(); data=manifest(args.model)
    args.cache.mkdir(parents=True,exist_ok=True); root=args.cache.resolve()
    directory=root/args.model/data['revision']
    opener=urllib.request.build_opener(AdmittedRedirect())
    for asset in data['assets']:
        destination=directory/asset['path']
        if valid_existing(destination,asset,root):
            print(asset['path'],'already verified',flush=True); continue
        if args.from_cache:
            source_root=args.from_cache.resolve(strict=True)
            source_path=source_root/('models--'+data['artifactRepository'].replace('/','--'))/'snapshots'/data['revision']/asset['path']
            confined(source_path,source_root)
            with open_regular(source_path,source_root) as source: write_verified(source,destination,asset,root)
        else:
            url=checked_url('https://huggingface.co/'+data['artifactRepository']+'/resolve/'+data['revision']+'/'+asset['path'])
            with opener.open(url,timeout=60) as source: write_verified(source,destination,asset,root)
        print(asset['path'],'verified',asset['bytes'],'bytes',flush=True)
    print(json.dumps({'model':data['modelId'],'revision':data['revision'],'cache':str(directory),'networkRequested':args.download}),flush=True)


if __name__=='__main__': main()
