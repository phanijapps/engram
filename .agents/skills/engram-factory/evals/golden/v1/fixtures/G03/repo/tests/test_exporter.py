import unittest
from pathlib import Path
class ExistingContract(unittest.TestCase):
    def test_entrypoint_exists(self):
        self.assertTrue(Path("exporter.py").is_file())
