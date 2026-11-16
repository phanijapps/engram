"""Immutable brief revisions with serialized, fail-closed registry updates.

Actor and instruction fields record caller assertions; they do not authenticate
an owner or establish that a brief has been ratified. Callers must independently
verify authority before binding a brief to an authorized factory job.
"""

import fcntl
import hashlib
import json
import os
import re
import tempfile
from contextlib import contextmanager
from pathlib import Path


class RegistryError(ValueError):
    """A registry invariant or optimistic concurrency check failed."""


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _sync_dir(path):
    fd = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


class Registry:
    """State root is the existing .engram-factory directory.

    All public operations acquire board.lock internally; a caller must not hold
    that lock. Interrupted file/index writes fail closed as an orphan; repair
    requires explicit external reconciliation, never automatic adoption.
    """

    def __init__(self, state_dir):
        self.state = Path(state_dir).resolve(strict=True)
        if not self.state.is_dir():
            raise RegistryError("factory state root must be a directory")
        self.root = self.state / "briefs"
        if self.root.is_symlink():
            raise RegistryError("briefs directory cannot be a symlink")
        self.root.mkdir(exist_ok=True)
        if self.root.resolve() != self.root:
            raise RegistryError("briefs directory must remain inside state root")
        self.index = self.root / "index.json"
        self.lock = self.state / "board.lock"

    @contextmanager
    def _locked(self):
        flags = os.O_RDWR | os.O_CREAT | getattr(os, "O_NOFOLLOW", 0)
        fd = os.open(self.lock, flags, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX)
            yield
        finally:
            fcntl.flock(fd, fcntl.LOCK_UN)
            os.close(fd)

    def _read(self):
        if self.index.is_symlink():
            raise RegistryError("index cannot be a symlink")
        if self.index.exists():
            try:
                state = json.loads(self.index.read_text(encoding="utf-8"))
            except (ValueError, OSError) as exc:
                raise RegistryError("invalid brief registry index") from exc
        else:
            state = {"schema_version": 1, "revision": 0, "briefs": {}, "operations": {}}
        if (not isinstance(state, dict) or state.get("schema_version") != 1
                or type(state.get("revision")) is not int or state["revision"] < 0
                or not isinstance(state.get("briefs"), dict)
                or not isinstance(state.get("operations"), dict)):
            raise RegistryError("invalid brief registry structure")
        registered = set()
        for brief_id, versions in state["briefs"].items():
            if not re.fullmatch(r"brief-[0-9]{3,}", brief_id) or not isinstance(versions, list) or not versions:
                raise RegistryError("invalid brief identity")
            for number, row in enumerate(versions, 1):
                expected = brief_id + ("" if number == 1 else f"-r{number:03d}") + ".md"
                if (not isinstance(row, dict) or row.get("revision") != number
                        or row.get("path") != expected or not re.fullmatch(r"[0-9a-f]{64}", str(row.get("sha256", "")))):
                    raise RegistryError("invalid revision metadata")
                path = self.root / expected
                if path.is_symlink() or not path.is_file() or _sha(path.read_bytes()) != row["sha256"]:
                    raise RegistryError("registered brief missing or modified: " + expected)
                registered.add(expected)
        actual = {p.name for p in self.root.glob("brief-*.md")}
        if actual != registered:
            raise RegistryError("unregistered or missing brief file; explicit reconciliation required")
        return state

    def _write_index(self, state):
        data = (json.dumps(state, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode()
        fd, temp = tempfile.mkstemp(prefix=".brief-index-", dir=self.root)
        try:
            with os.fdopen(fd, "wb") as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temp, self.index)
            _sync_dir(self.root)
        finally:
            if os.path.exists(temp):
                os.unlink(temp)

    def _update(self, brief_id, content, actor, instruction, expected_revision, operation_id):
        if not isinstance(content, bytes) or not content.strip():
            raise RegistryError("brief content must be nonempty bytes")
        if any(not isinstance(x, str) or not x.strip() for x in (actor, instruction, operation_id)):
            raise RegistryError("actor, instruction and operation_id are required")
        if type(expected_revision) is not int or expected_revision < 0:
            raise RegistryError("expected_revision must be a nonnegative integer")
        request = _sha(json.dumps({"brief_id": brief_id, "content": _sha(content), "actor": actor,
                                   "instruction": instruction}, sort_keys=True).encode())
        with self._locked():
            state = self._read()
            old = state["operations"].get(operation_id)
            if old:
                if old.get("request") != request:
                    raise RegistryError("operation_id already used for a different request")
                return old["result"]
            if state["revision"] != expected_revision:
                raise RegistryError("stale registry revision")
            if brief_id is None:
                next_id = max((int(x[6:]) for x in state["briefs"]), default=0) + 1
                brief_id = f"brief-{next_id:03d}"
                state["briefs"][brief_id] = []
            elif brief_id not in state["briefs"]:
                raise RegistryError("unknown brief")
            versions = state["briefs"][brief_id]
            number = len(versions) + 1
            name = brief_id + ("" if number == 1 else f"-r{number:03d}") + ".md"
            path = self.root / name
            try:
                fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0), 0o600)
                with os.fdopen(fd, "wb") as stream:
                    stream.write(content)
                    stream.flush()
                    os.fsync(stream.fileno())
                _sync_dir(self.root)
            except FileExistsError as exc:
                raise RegistryError("brief file already exists; explicit reconciliation required") from exc
            row = {"revision": number, "path": name, "sha256": _sha(content), "actor": actor,
                   "instruction": instruction, "authority": "unverified caller assertion"}
            versions.append(row)
            state["revision"] += 1
            result = {"brief_id": brief_id, "brief_revision": number, "registry_revision": state["revision"], **row}
            state["operations"][operation_id] = {"request": request, "result": result}
            self._write_index(state)
            return result

    def create(self, content, *, actor, instruction, expected_revision, operation_id):
        return self._update(None, content, actor, instruction, expected_revision, operation_id)

    def supersede(self, brief_id, content, *, actor, instruction, expected_revision, operation_id):
        if not isinstance(brief_id, str) or not re.fullmatch(r"brief-[0-9]{3,}", brief_id):
            raise RegistryError("invalid brief_id")
        return self._update(brief_id, content, actor, instruction, expected_revision, operation_id)

    def list(self):
        with self._locked():
            return self._read()

    def get_locked(self, brief_id, revision=None):
        """Read a validated revision while the caller already holds board.lock.

        This helper does not acquire a lock. Calling it without the shared
        exclusive lock permits concurrent mutation between validation and read.
        """
        state = self._read()
        versions = state["briefs"].get(brief_id)
        if not versions or (revision is not None and (type(revision) is not int or not 1 <= revision <= len(versions))):
            raise RegistryError("unknown brief revision")
        row = versions[-1] if revision is None else versions[revision - 1]
        content = (self.root / row["path"]).read_bytes()
        if _sha(content) != row["sha256"]:
            raise RegistryError("registered brief changed during read")
        return {"brief_id": brief_id, **row}, content

    def get(self, brief_id, revision=None):
        with self._locked():
            return self.get_locked(brief_id, revision)
