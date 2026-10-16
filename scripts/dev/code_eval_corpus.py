"""Verify frozen source and labels before running any retrieval evaluation."""
import hashlib
import json
from pathlib import Path
import subprocess


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_artifact_paths(manifest_path, output, storage, corpus_roots=(), extra_inputs=()):
    """Reject writes into corpora or aliases of any frozen evaluation input."""
    manifest_path = Path(manifest_path).resolve(strict=True)
    manifest = json.loads(manifest_path.read_text())
    inputs = {manifest_path, *(Path(p).resolve() for p in extra_inputs)}
    inputs.update((manifest_path.parent / c["labelsFile"]).resolve() for c in manifest["corpora"])
    for section in ("authorizationBattery", "independentLabelReview"):
        if "file" in manifest.get(section, {}):
            inputs.add((manifest_path.parent / manifest[section]["file"]).resolve())
    output = Path(output)
    writes = [output.resolve(), output.with_suffix(".tmp").resolve(), Path(storage).resolve()]
    if any(path in inputs for path in writes):
        raise ValueError("evaluation artifact collides with frozen input")
    if any(path.is_relative_to(Path(root).resolve(strict=True)) for path in writes for root in corpus_roots):
        raise ValueError("store/output artifacts lie inside indexed corpus")


def verify_corpus(root, corpus):
    root = root.resolve(strict=True)
    names = subprocess.check_output(
        ["rg", "--files", "--hidden", "-g", "!.git/**"], cwd=root, text=True).splitlines()
    materialized = {p for p in names if (root / p).is_file() and not (root / p).is_symlink()}
    if materialized != set(corpus["files"]):
        raise ValueError(f"corpus inventory drift: {corpus['id']}")
    for relative, expected in corpus["files"].items():
        path = (root / relative).resolve(strict=True)
        if not path.is_relative_to(root) or sha256(path) != expected:
            raise ValueError(f"corpus drift: {corpus['id']}:{relative}")


def verify_labels(root, corpus, entries):
    """Check identity, bytes, ranges and split boundaries; never infer relevance."""
    root = root.resolve(strict=True)
    seen = set()
    for entry in entries:
        if entry["id"] in seen:
            raise ValueError("duplicate query id")
        seen.add(entry["id"])
        if entry["split"] not in ("development", "heldout", "original", "identifier"):
            raise ValueError("unknown split")
        units = entry["requiredUnits"]
        if (entry["polarity"] == "answerable") != bool(units):
            raise ValueError("answerable queries require nonempty source labels")
        for unit in units:
            path = (root / unit["path"]).resolve(strict=True)
            if not path.is_relative_to(root):
                raise ValueError("label path escapes corpus")
            raw = path.read_bytes()
            if sha256(path) != corpus["files"].get(unit["path"]):
                raise ValueError("label source not pinned")
            a, b = unit["startByte"], unit["endByte"]
            if not 0 <= a < b <= len(raw):
                raise ValueError("invalid whole-unit range")
            raw[:a].decode("utf-8")
            raw[a:b].decode("utf-8")
            if not unit["requiredRanges"] or unit["grade"] != 3:
                raise ValueError("required unit must have decisive ranges and grade3")
            for part in unit["requiredRanges"]:
                x, y = part["startByte"], part["endByte"]
                if not a <= x < y <= b:
                    raise ValueError("decisive range escapes callable")
                raw[:x].decode("utf-8")
                raw[x:y].decode("utf-8")


def load_frozen(manifest_path, corpus_id, corpus_root, split):
    manifest_path = Path(manifest_path).resolve(strict=True)
    manifest_bytes = manifest_path.read_bytes()
    manifest = json.loads(manifest_bytes)
    corpus = next(c for c in manifest["corpora"] if c["id"] == corpus_id)
    root = Path(corpus_root).resolve(strict=True)
    # Labels and judgments must never be in a scanned corpus.
    labels_path = (manifest_path.parent / corpus["labelsFile"]).resolve(strict=True)
    if labels_path.is_relative_to(root) or manifest_path.is_relative_to(root):
        raise ValueError("evaluation artifacts lie inside indexed corpus")
    if sha256(labels_path) != corpus["labelsSha256"]:
        raise ValueError("frozen label hash changed")
    entries = json.loads(labels_path.read_text())
    verify_corpus(root, corpus)
    verify_labels(root, corpus, entries)
    selected = [e for e in entries if e["split"] == split]
    by_id = {e["id"]: e for e in selected}
    order = manifest["queryOrder"][split]
    if len(order) != len(set(order)):
        raise ValueError("query order contains duplicate ids")
    ordered = [by_id[i] for i in order if i in by_id]
    if len(ordered) != len(selected) or {e["id"] for e in ordered} != set(by_id):
        raise ValueError("query order does not cover selected labels")
    manifest_digest = hashlib.sha256(manifest_bytes).hexdigest()
    if sha256(manifest_path) != manifest_digest:
        raise ValueError("manifest changed during verification")
    return manifest, corpus, ordered, manifest_digest
