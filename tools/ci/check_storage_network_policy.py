#!/usr/bin/env python3
"""Attempt-observation controls; real provider acceptance belongs to TASK-013."""
from __future__ import annotations

from contextlib import contextmanager
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

AMBIENT_METRICS = ("saved-basic", "saved-full", "env-basic", "env-full")
# Definitions only: current PLT-001 still rejects disabled-vector service access.
PROPOSED_PROFILES = {
    "embedded-offline": "zero outbound attempts; zero model resolver/initializer events",
    "service-disabled-vectors": "only explicit selected storage/auth; no model or telemetry",
}


class ObservationError(RuntimeError):
    """Missing or incomplete observation cannot establish zero attempts."""


@dataclass(frozen=True)
class Observation:
    attempts: tuple[str, ...]
    trace: Path


def attempted_network(trace: str) -> tuple[str, ...]:
    attempts = []
    for line in trace.splitlines():
        if re.search(r"\b(connect|send|sendto|sendmsg|sendmmsg)\(", line):
            # AF_UNIX operations are local IPC; unresolved destinations fail closed.
            if "AF_UNIX" not in line:
                attempts.append(line)
    return tuple(attempts)


def read_observation(trace: Path, returncode: int) -> Observation:
    if returncode != 0 or not trace.is_file():
        raise ObservationError("trace/command failed or trace is missing")
    content = trace.read_text()
    if "+++ exited with 0 +++" not in content or "strace:" in content:
        raise ObservationError("trace is incomplete or tracer reported an error")
    if "<unfinished ...>" in content or re.search(r"<\.\.\. \w+ resumed>", content):
        raise ObservationError("split syscall trace is incomplete; cannot establish zero attempts")
    supported = {"socket", "socketpair", "bind", "listen", "accept", "accept4",
                 "connect", "getsockname", "getpeername", "getsockopt", "setsockopt",
                 "recvfrom", "recvmsg", "recvmmsg", "sendto", "sendmsg", "sendmmsg",
                 "send", "recv", "shutdown"}
    observed, completed = set(), set()
    for line in content.splitlines():
        matched = re.fullmatch(r"(?:(\d+)\s+)?(.*)", line)
        pid, body = matched.groups()
        if pid in completed:
            raise ObservationError("trace record follows process completion")
        observed.add(pid)
        if body == "+++ exited with 0 +++":
            completed.add(pid)
        elif body.startswith("+++"):
            raise ObservationError("malformed or unsuccessful process completion")
        elif re.fullmatch(r"--- SIG[A-Z0-9]+ \{.*\} ---", body):
            continue
        else:
            call = re.fullmatch(r"([a-z_][a-z_0-9]*)\(.*\)\s+=\s+(?:-?\d+|0x[0-9a-f]+)(?:\s.*)?", body)
            if call is None or call.group(1) not in supported:
                raise ObservationError("unknown or incomplete syscall trace record")
    if observed != completed:
        raise ObservationError("each observed process requires its own successful completion")
    return Observation(attempted_network(content), trace)


def trace_command(command: list[str], directory: Path, *, env: dict[str, str] | None = None,
                  deny_attempts: bool = False, tracer: str = "strace") -> Observation:
    if sys.platform != "linux" or not shutil.which(tracer):
        raise ObservationError("native syscall tracing unavailable; target remains NOT_RUN")
    directory.mkdir(parents=True, exist_ok=True)
    trace = directory / "network.trace"
    if trace.exists():
        raise ObservationError("refusing to overwrite an existing observation")
    args = [tracer, "-f", "-s", "0", "-e", "trace=network", "-o", str(trace)]
    if deny_attempts:
        args += ["-e", "inject=connect:error=EACCES", "-e", "inject=sendto:error=EACCES"]
    try:
        result = subprocess.run(args + ["--"] + command, env=env, capture_output=True,
                                timeout=30, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ObservationError("tracer execution failed") from error
    (directory / "command.stdout").write_bytes(result.stdout)
    (directory / "command.stderr").write_bytes(result.stderr)
    return read_observation(trace, result.returncode)


def assert_offline(observation: Observation, model_events: Path) -> None:
    if observation.attempts:
        raise ObservationError("outbound network attempt observed, including denied attempts")
    if not model_events.is_file():
        raise ObservationError("model resolver/initializer probe is missing")
    events = json.loads(model_events.read_text())
    if not isinstance(events, list) or events:
        raise ObservationError("model resolver/initializer invocation observed")


@contextmanager
def telemetry_fixture(directory: Path, variant: str):
    """Change child-only environment and owned preference bytes, never global prefs."""
    if variant not in AMBIENT_METRICS:
        raise ValueError("unknown ambient metrics fixture")
    root = directory / "helix-home"
    root.mkdir(parents=True)
    preferences = root / "metrics.toml"
    kind, level = variant.split("-")
    preferences.write_text(f'level = "{level if kind == "saved" else "off"}"\ninstallation_id = "00000000-0000-4000-8000-000000000001"\nuser_id = "engram-test-user"\n')
    before = preferences.read_bytes()
    (directory / "preferences.before").write_bytes(before)
    env = os.environ.copy()
    for name in tuple(env):
        if name.startswith("HELIX_TELEMETRY_"):
            env.pop(name)
    env["HELIX_HOME"] = str(root)
    if kind == "env":
        env["HELIX_TELEMETRY_LEVEL"] = level
    try:
        yield env, preferences
    finally:
        after = preferences.read_bytes() if preferences.exists() else None
        if after is not None:
            (directory / "preferences.after").write_bytes(after)
        (directory / "preference-hashes.json").write_text(json.dumps({
            "before": hashlib.sha256(before).hexdigest(),
            "after": hashlib.sha256(after).hexdigest() if after is not None else None,
        }) + "\n")
        if before != after:
            raise ObservationError("saved preferences changed during observation")


def main() -> int:
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", required=True)
    parser.parse_args()
    # Run the real positive controls, never a provider success claim.
    result = subprocess.run([sys.executable, "-m", "unittest",
                             "tools.ci.tests.test_storage_network_policy"])
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
