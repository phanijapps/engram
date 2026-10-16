"""Development-only finite-f32 threshold selection and immutable run identities."""
import hashlib
import json
import math
import struct
import subprocess

F32_MAX = struct.unpack('!f', bytes.fromhex('7f7fffff'))[0]


def f32(value):
    value = struct.unpack('!f', struct.pack('!f', value))[0]
    if not math.isfinite(value):
        raise ValueError('nonfinite f32 strategy score')
    return value


def next_up(value):
    value = f32(value)
    if value == F32_MAX:
        return None
    if value == 0:
        return struct.unpack('!f', bytes.fromhex('00000001'))[0]
    bits = struct.unpack('!I', struct.pack('!f', value))[0]
    return struct.unpack('!f', struct.pack('!I', bits + (1 if value > 0 else -1)))[0]


def strategy_maximum(packet, variant):
    d = packet['diagnostics']
    cross = variant == 'balanced'
    expected = 'cross_encoder' if cross else 'none'
    state = 'ready' if cross else 'disabled'
    if d['reindexRequired'] or any(f['source'] != 'code_sources' for f in packet['sourceFailures']) or d['effectiveReranker'] != expected or d['requestedReranker'] != expected or d['scorerState'] != state:
        raise ValueError('cannot calibrate degraded, incompatible, or failed evidence')
    scores = []
    for item in packet['items']:
        if item['sourceCategory'] != 'implementation':
            raise ValueError('nonimplementation primary item')
        score = item['rerankScore'] if cross else item['fusionScore']
        if score is None:
            raise ValueError('missing strategy score')
        scores.append(f32(score))
    return max(scores) if scores else None


def select_threshold(negative_maxima):
    if not negative_maxima:
        raise ValueError('negative denominator must be nonempty')
    values = [None if x is None else f32(x) for x in negative_maxima]
    candidates = {-F32_MAX}
    candidates.update(y for x in values if x is not None and (y := next_up(x)) is not None)
    target = math.ceil(.90 * len(values))
    for threshold in sorted(candidates):
        rejected = sum(x is None or x < threshold for x in values)
        if rejected >= target:
            return threshold, rejected, target
    raise ValueError('no finite f32 threshold meets the fixed negative target')


def seal(template, threshold, development_labels, binary):
    artifact = {**template, 'threshold': threshold,
                'developmentLabelsSha256': digest(development_labels)}
    process = subprocess.run([str(binary)], input=json.dumps(artifact), capture_output=True, text=True, timeout=30)
    if process.returncode:
        raise ValueError('Rust calibration sealer failed: ' + process.stderr)
    result = json.loads(process.stdout)
    if not result['id'].startswith('code-calibration-v1/') or f32(result['threshold']) != f32(threshold):
        raise ValueError('calibration sealer returned invalid identity/threshold')
    return result


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
