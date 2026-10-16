"""Exact accepted T0 measurements and reviewed v1 input lineage."""
import json
from code_eval_corpus import sha256
from code_evidence_eval_support import ROOT, inputs

RECEIPT = ROOT/'docs/research/reliable-code-intelligence-baseline-2026-10-14.json'
RECEIPT_SHA256 = 'a24ec8625d99590be3c93f4ccc50a698be9b5f76f218ba109f910aaa17ee8685'
REVIEWED_MANIFEST_SHA256 = 'a78489ada1947213c5a5719a7ac9661c9ea37e66168bd95f0f3bfbc856de8feb'


def require_baselines(args, manifest, manifest_hash):
    # T0 ran before blind label-range review. Its historical manifest hashes are
    # provenance, not a reason to replace or rerun the accepted measurements.
    # Admit only the exact committed receipt and final reviewed v1 inputs.
    if manifest_hash != REVIEWED_MANIFEST_SHA256 or sha256(RECEIPT) != RECEIPT_SHA256:
        raise ValueError('accepted baseline receipt or reviewed manifest drift')
    receipt = json.loads(RECEIPT.read_text())
    if receipt['baselineCommit'] != manifest['baselineCommit']:
        raise ValueError('accepted baseline commit mismatch')
    found = {str(RECEIPT): RECEIPT_SHA256}
    def accepted(path):
        name=path.stem
        if sha256(path) != receipt['collectionSummary'][name]['rawReportSha256']:
            raise ValueError('accepted baseline raw artifact drift: '+str(path))
        report=json.loads(path.read_text())
        if report != receipt['reports'][name]:
            raise ValueError('accepted baseline receipt content mismatch: '+str(path))
        return report
    for name, split in [('engram','original'),('engram','identifier'),('engram','development'),('flask','development'),('vite','development')]:
        path = args.base/'results'/f'{name}-baseline-{split}.json'
        report = accepted(path)
        if report['runState'] != 'complete' or report['binarySha256'] != manifest['baselineBinarySha256'] or any(c['failed'] for c in report['cases']):
            raise ValueError('completed frozen baseline prerequisite failed: '+str(path))
        corpus=next(c for c in manifest['corpora'] if c['id']==name)
        if report['corpus']!=name or report['revision']!=corpus['revision'] or report['mode']!='baseline' or report['split']!=split:
            raise ValueError('baseline corpus/revision/mode/split mismatch')
        expected = [e['id'] for e in inputs(args,split)[2] if e['corpus']==name]
        if [c['id'] for c in report['cases']] != expected:
            raise ValueError('baseline case inventory/order mismatch')
        found[str(path)] = sha256(path)
    auth=args.base/'results/authorization-baseline.json'
    authorization=accepted(auth)
    if authorization.get('runState')!='complete' or not authorization.get('passed') or len(authorization['cases'])!=4 or authorization['fixtureSha256']!=manifest['authorizationBattery']['sha256'] or authorization['binarySha256']!=manifest['baselineBinarySha256']:
        raise ValueError('frozen authorization baseline prerequisite failed')
    found[str(auth)]=sha256(auth)
    if sha256(args.base/'baseline-engram-mcp') != manifest['baselineBinarySha256']:
        raise ValueError('baseline binary drift')
    return found

