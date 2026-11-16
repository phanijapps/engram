"""Spec author interface; shared state machine lives in author_session."""
from types import SimpleNamespace
import author_session


def session(api, jobdir, job):
    return author_session.session(api, jobdir, job, phase="spec")


def open_session(namespace, jobdir, job, author, reviewers_file):
    return author_session.open_session(namespace, jobdir, job, author, reviewers_file, phase="spec")


def context(namespace, jobdir, job, revision):
    return author_session.context(namespace, jobdir, job, revision, phase="spec")


def check(namespace, jobdir, job):
    return author_session.check(namespace, jobdir, job, phase="spec")


def operation_path(job, operation):
    return author_session.operation_path(job, operation, phase="spec")


def replay(namespace, jobdir, job, args):
    return author_session.replay(namespace, jobdir, job, args, phase="spec")


def submit(namespace, jobdir, job, revision, args):
    return author_session.submit(namespace, jobdir, job, revision, args, phase="spec")
