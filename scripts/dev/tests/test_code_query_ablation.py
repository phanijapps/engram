import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from code_query_ablation import QueryProbe, judge, matches

class AblationTests(unittest.TestCase):
    def test_hung_probe_is_bounded_and_reaped_even_when_terminate_is_ignored(self):
        probe = QueryProbe([sys.executable, "-u", "-c",
                            "import signal,sys,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); sys.stdin.readline(); time.sleep(10)"])
        try:
            with self.assertRaises(TimeoutError):
                probe.call("question", "none", timeout=0.1)
        finally:
            probe.close(grace=0.05)
        self.assertIsNotNone(probe.process.returncode)

    def test_duplicate_parents_do_not_inflate_offline_recall_or_ndcg(self):
        unit = {"path":"a.py","qualifiedAnchor":"Service.run","requiredRanges":[{"startByte":20,"endByte":40}]}
        parent = {"path":"a.py","anchor":"def Service::run","declarationRange":{"start":10,"end":50}}
        result = judge([parent,parent], [unit])
        self.assertEqual(result["offlineRequiredUnitRecall10"], 1)
        self.assertEqual(result["offlineNdcg10"], 1)
        self.assertEqual(result["offlineMrr10"], 1)
        self.assertFalse(matches({**parent,"path":"other.py"},unit))
        self.assertFalse(matches({**parent,"anchor":"class Service"},unit))
        self.assertFalse(matches({**parent,"declarationRange":{"start":10,"end":30}},unit))
        missed = judge([], [unit])
        self.assertEqual(missed["offlineRequiredUnitRecall10"],0)
        self.assertEqual(missed["offlineMrr10"],0)
        self.assertEqual(missed["offlineNdcg10"],0)

if __name__ == "__main__":
    unittest.main()
