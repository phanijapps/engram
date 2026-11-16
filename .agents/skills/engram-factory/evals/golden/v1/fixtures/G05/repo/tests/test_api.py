import unittest
from api import request_id
class RequestIds(unittest.TestCase):
    def test_empty(self):
        self.assertEqual(request_id({"X-Request-ID": ""})[0], 400)
    def test_valid(self):
        self.assertEqual(request_id({"X-Request-ID": "abc"}), (200, {"request_id": "abc"}))
    def test_absent(self):
        self.assertEqual(request_id({})[0], 400)
