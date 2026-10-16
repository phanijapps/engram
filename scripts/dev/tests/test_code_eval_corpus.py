"""Construction checks for immutable code-evaluation inputs."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from code_eval_corpus import load_frozen, verify_corpus, verify_labels, validate_artifact_paths


class FrozenCorpusTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / "corpus"
        self.root.mkdir()
        self.raw = "def example():\n    return 'é'\n".encode()
        (self.root / "a.py").write_bytes(self.raw)
        self.corpus = {"id": "test", "files": {"a.py": hashlib.sha256(self.raw).hexdigest()}}
        self.entry = {"id": "one", "split": "heldout", "polarity": "answerable",
                      "query": "What is returned?", "requiredUnits": [{"path": "a.py", "grade": 3,
                      "startByte": 0, "endByte": len(self.raw),
                      "requiredRanges": [{"startByte": 15, "endByte": len(self.raw)}]}]}

    def test_source_hash_and_inventory_are_both_frozen(self):
        verify_corpus(self.root, self.corpus)
        (self.root / "extra.py").write_text("pass\n")
        with self.assertRaisesRegex(ValueError, "inventory drift"):
            verify_corpus(self.root, self.corpus)
        (self.root / "extra.py").unlink()
        (self.root / "a.py").write_bytes(self.raw + b"# changed")
        with self.assertRaisesRegex(ValueError, "corpus drift"):
            verify_corpus(self.root, self.corpus)

    def test_ranges_are_exact_utf8_and_within_the_callable(self):
        verify_labels(self.root, self.corpus, [self.entry])
        unit = self.entry["requiredUnits"][0]
        unit["requiredRanges"][0]["startByte"] = self.raw.index('é'.encode()) + 1
        with self.assertRaises(UnicodeDecodeError):
            verify_labels(self.root, self.corpus, [self.entry])
        unit["requiredRanges"][0]["startByte"] = -1
        with self.assertRaisesRegex(ValueError, "escapes callable"):
            verify_labels(self.root, self.corpus, [self.entry])

    def test_answerable_requires_labels_and_negatives_cannot_borrow_them(self):
        self.entry["polarity"] = "negative"
        with self.assertRaises(ValueError):
            verify_labels(self.root, self.corpus, [self.entry])
        self.entry["requiredUnits"] = []
        verify_labels(self.root, self.corpus, [self.entry])

    def test_label_hash_and_query_order_are_checked(self):
        labels = self.base / "labels.json"
        labels.write_text(json.dumps([self.entry]))
        self.corpus.update(labelsFile="labels.json", labelsSha256=hashlib.sha256(labels.read_bytes()).hexdigest())
        manifest = self.base / "manifest.json"
        x = {"corpora": [self.corpus], "queryOrder": {"heldout": ["one"]}}
        manifest.write_text(json.dumps(x))
        self.assertEqual(len(load_frozen(manifest, "test", self.root, "heldout")[2]), 1)
        x["queryOrder"]["heldout"] = []
        manifest.write_text(json.dumps(x))
        with self.assertRaisesRegex(ValueError, "query order"):
            load_frozen(manifest, "test", self.root, "heldout")
        labels.write_text("[]")
        with self.assertRaisesRegex(ValueError, "label hash"):
            load_frozen(manifest, "test", self.root, "heldout")

    def test_order_cannot_replace_a_hard_query_with_an_easy_duplicate(self):
        second = {**self.entry, "id": "two", "query": "A different hard question?"}
        labels = self.base / "labels.json"
        labels.write_text(json.dumps([self.entry, second]))
        self.corpus.update(labelsFile="labels.json", labelsSha256=hashlib.sha256(labels.read_bytes()).hexdigest())
        manifest = self.base / "manifest.json"
        manifest.write_text(json.dumps({"corpora": [self.corpus], "queryOrder": {"heldout": ["one", "one"]}}))
        with self.assertRaisesRegex(ValueError, "duplicate ids"):
            load_frozen(manifest, "test", self.root, "heldout")

    def test_artifact_output_cannot_overwrite_frozen_inputs_or_enter_corpus(self):
        manifest = self.base / "manifest.json"
        manifest.write_text(json.dumps({"corpora": [{"labelsFile": "labels.json"}],
            "independentLabelReview": {"file": "review.md"}, "authorizationBattery": {"file": "authorization.json"}}))
        for name in ["manifest.json", "labels.json", "review.md", "authorization.json"]:
            with self.assertRaisesRegex(ValueError, "collides"):
                validate_artifact_paths(manifest, self.base / name, self.base / "store", [self.root])
        # A temp checkpoint can collide even if the final output does not.
        temp_manifest = self.base / "checkpoint.tmp"
        temp_manifest.write_text(manifest.read_text())
        with self.assertRaisesRegex(ValueError, "collides"):
            validate_artifact_paths(temp_manifest, self.base / "checkpoint.json", self.base / "store")
        with self.assertRaisesRegex(ValueError, "inside indexed corpus"):
            validate_artifact_paths(manifest, self.root / "results.json", self.base / "store", [self.root])
        validate_artifact_paths(manifest, self.base / "results.json", self.base / "store", [self.root])


if __name__ == "__main__":
    unittest.main()
