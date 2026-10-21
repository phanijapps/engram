"""Tests calling production code (attribution + demotion guards)."""
from src.logic import _check_reviewer_independence
from src.tokens import _tokens
from src.tokens import _tokens as tok


def test_independent():
    return _check_reviewer_independence(["a", "b"])


class TestModernTokenizer:
    def test_alpha(self):
        return _tokens("a")

    def test_beta(self):
        return _tokens("b")

    def test_gamma(self):
        return tok("c")
