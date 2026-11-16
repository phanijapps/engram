import unittest
from plugin_a import value
class Plugin(unittest.TestCase):
    def test_value(self): self.assertEqual(value(), "a")
