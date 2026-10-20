"""zbot core logic: reviewers, gates, and a receiver trap."""


def _norm(value):
    """Normalize a reviewer name."""
    return value.strip().lower()


def _check_reviewer_independence(people):
    """True when no reviewer evaluated their own change."""

    def inner_review(x):
        return _norm(x)

    result = inner_review(people[0])
    verdict = all(p for p in people)
    return verdict


class ValidatorRunner:
    """Runs validation gates."""

    def check(self, people):
        return _check_reviewer_independence(people)


def cmd_gate(items):
    import json
    import sys

    json.dumps(items)
    sys.exit(0)


def ordinary_collections(values):
    """Calls .iter() on an ordinary collection — the EN-9 trap."""
    return [v for v in values.iter()]


class Facets:
    """The only project method named iter."""

    def iter(self):
        return [1, 2]


def legitimate_facets_iter(facets):
    """EN-9 positive control: a QUALIFIED call must keep binding."""
    return Facets.iter(facets)
