import unittest
from plugin_b import value
class Plugin(unittest.TestCase):
    def test_value(self): self.assertEqual(value(), "b")
