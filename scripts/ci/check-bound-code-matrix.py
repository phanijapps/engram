#!/usr/bin/env python3
"""Fail-closed operation/backend/transport matrix runner for bound code.

This runner is deliberately an executor, not a replacement for the behavioral
fixtures owned by TASK-026.  Each row has an independently persisted receipt.
Credentials stay in a private env file and are scrubbed before a receipt is
written or printed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable
from urllib.parse import parse_qsl, urlparse

ROOT = Path(__file__).resolve().parents[2]
MARKER = "engram.factory.job=010-project-bound-branch-indexing; disposable=true"
PG_ENV_FILE = "ENGRAM_TEST_PG_ENV_FILE"
PINNED_CONTAINER_ID = "583ce534a623cdfd58400583b441941429e9ae9533b634bb59b1060e61f44bc5"
PINNED_DATABASE = "engram_test_ec536887ebee"
PINNED_LABELS = {"engram.factory.job": "010-project-bound-branch-indexing", "engram.factory.disposable": "true", "engram.factory.owner": "engram-builder"}
URL_QUERY_KEYS = {"sslmode", "application_name", "connect_timeout"}
OPERATIONS = ("binding", "scan", "retrieval", "graph", "source", "reset", "backfill")
TRANSPORTS = ("rust", "native", "mcp-stdio", "mcp-oneshot")


@dataclass(frozen=True)
class Row:
    operation: str
    backend: str
    transport: str
    required: bool

    @property
    def name(self) -> str:
        return f"{self.operation}/{self.backend}/{self.transport}"


def redact(value: str) -> str:
    """Remove URI credentials and common secret assignment values."""
    import re

    value = re.sub(r"([a-z][a-z0-9+.-]*://)[^\s/@]+@", r"\1[REDACTED]@", value, flags=re.I)
    return re.sub(r"(?i)((?:password|passwd|token|secret|api[_-]?key)\s*[=:]\s*)(?:\"[^\"]*\"|'[^']*'|\S+)", r"\1[REDACTED]", value)


def safe_argv(argv: Iterable[str]) -> list[str]:
    return [redact(str(part)) for part in argv]


def load_private_pg_env(path: str, url_name: str) -> dict[str, str]:
    fixture = Path(path)
    if not fixture.is_absolute() or fixture.is_symlink() or not fixture.is_file():
        raise ValueError("PG fixture env file must be an absolute regular file")
    info = fixture.stat()
    parent = fixture.parent.stat()
    if stat.S_IMODE(info.st_mode) != 0o600 or stat.S_IMODE(parent.st_mode) & 0o077:
        raise ValueError("PG fixture env file custody rejected")
    if info.st_uid != os.geteuid() or parent.st_uid != os.geteuid():
        raise ValueError("PG fixture env file ownership rejected")
    values: dict[str, str] = {}
    for raw in fixture.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if "=" not in line:
            raise ValueError("PG fixture env file has malformed assignment")
        key, value = line.split("=", 1)
        if key not in {url_name, "PG_FIXTURE_CONTAINER_ID", "PG_FIXTURE_ROTATION_UTC", "PG_FIXTURE_PASSWORD_STATE"}:
            raise ValueError("PG fixture env file has unrecognized key")
        values[key] = value.strip().strip("\"").strip("'")
    if not values.get(url_name):
        raise ValueError(f"PG fixture env file lacks {url_name}")
    return values


def validate_pg_url(url: str) -> None:
    parsed = urlparse(url)
    if parsed.scheme not in {"postgres", "postgresql"} or not parsed.hostname or not parsed.path:
        raise ValueError("PG fixture URL is not a PostgreSQL database URL")
    if parsed.username is None or parsed.password is None:
        raise ValueError("PG fixture URL lacks private credentials")
    if any(key not in URL_QUERY_KEYS for key, _ in parse_qsl(parsed.query)):
        raise ValueError("PG fixture URL has unsupported query parameter")


def native_helper_argv(recipe: str) -> list[str]:
    """Invoke the TASK-020 immutable artifact helper instead of rebuilding here."""
    if recipe not in {"sqlite", "pgvector"}:
        raise ValueError(f"unknown native recipe: {recipe}")
    helper = ROOT / "packages/node/test/helpers/build-native-recipes.mjs"
    program = (
        "import {withRecipeArtifacts} from './packages/node/test/helpers/build-native-recipes.mjs';"
        f"withRecipeArtifacts((a) => process.stdout.write(JSON.stringify({{recipe:{json.dumps(recipe)},path:a.paths[{json.dumps(recipe)}],digest:a.digests[{json.dumps(recipe)}]}})));"
    )
    return ["node", "--input-type=module", "--eval", program]


def matrix_rows(require_native: bool, require_mcp: bool, require_pg: bool) -> list[Row]:
    rows: list[Row] = []
    for operation in OPERATIONS:
        for backend in ("sqlite", "pgvector"):
            for transport in TRANSPORTS:
                required = backend == "sqlite" or require_pg
                if transport == "native":
                    required = required and require_native
                elif transport.startswith("mcp"):
                    required = required and require_mcp
                rows.append(Row(operation, backend, transport, required))
    return rows


@dataclass(frozen=True)
class CommandSpec:
    argv: list[str]
    marker: str
    surface: str = "transport"
    transport_surface: str | None = None


def cargo_spec(package: str, target: str, filter_name: str, marker: str, backend: str) -> CommandSpec:
    argv = ["cargo", "test", "-p", package]
    if backend == "sqlite" and package == "engram-backend-sqlite":
        argv.extend(["--no-default-features"])
    if package == "engram-integration":
        argv.extend(["--features", "sqlite"])
    if package == "engram-mcp":
        argv.extend(["--features", "pgvector"])
    argv.extend(["--test", target, filter_name, "--", "--test-threads=1"])
    return CommandSpec(argv, marker)


def default_command(row: Row, sqlite_root: Path, pg_url_name: str) -> CommandSpec:
    """Select one focused behavioral proof for this exact operation cell.

    Rust operation labels map to their owned real-store targets.  Native and
    MCP rows use their public surface where shipped.  Reset has no public
    native/MCP forwarding until R5, so it is deliberately a coordinator proof
    with a machine-readable transport gap rather than a false transport pass.
    """
    rust = {
        "binding": ("engram-backend-sqlite" if row.backend == "sqlite" else "engram-backend-pgvector", "bound_recipe", "persisted_binding", "persisted_binding"),
        "scan": ("engram-integration", "bound_source_status", "durable_scan_stage_publish_fetch", "durable_scan_stage_publish_fetch"),
        "retrieval": ("engram-conformance", "bound_retrieval", "real_vector_lane_constrains_before_top_k", "real_vector_lane_constrains_before_top_k"),
        "graph": ("engram-conformance", "bound_graph", "eligible_graph_budgets", "eligible_graph_budgets"),
        "source": ("engram-integration", "bound_source_status", "exact_bytes_and_spans", "exact_bytes_and_spans"),
        # These Pg Rust proofs must not use the SQLite integration/conformance
        # fixtures above.  Each selected case opens the real pgvector recipe.
        "pg_scan": ("engram-backend-pgvector", "bound_recipe", "task_019::authorized_registration", "authorized_registration"),
        "pg_retrieval": ("engram-backend-pgvector", "bound_recipe", "task_019::required_handles", "required_handles"),
        "pg_graph": ("engram-backend-pgvector", "bound_recipe", "task_019::required_handles", "required_handles"),
        "pg_source": ("engram-backend-pgvector", "bound_recipe", "task_019::required_handles", "required_handles"),
        "reset": ("engram-conformance", "code_reset_recovery", "real_recipe_bundle_retires_code_and_preserves_ordinary_lexical_records", "real_recipe_bundle_retires_code_and_preserves_ordinary_lexical_records"),
        "backfill": ("engram-mcp", "bound_transports", "task_022::backfill_not_rescan", "backfill_not_rescan"),
    }
    if row.transport == "rust":
        key = f"pg_{row.operation}" if row.backend == "pgvector" and row.operation in {"scan", "retrieval", "graph", "source"} else row.operation
        package, target, filter_name, marker = rust[key]
        spec = cargo_spec(package, target, filter_name, marker, row.backend)
        if row.operation == "reset" and row.backend == "pgvector":
            package, target, filter_name, marker = "engram-backend-pgvector", "code_reset_recovery", "task_023::authorization_hash_and_ambiguous_ownership_are_behavioral", "authorization_hash_and_ambiguous_ownership_are_behavioral"
            spec = cargo_spec(package, target, filter_name, marker, row.backend)
            spec.argv.extend(["--ignored"])
        return spec
    if row.transport == "native":
        native = {
            "binding": "deny_by_default_and_authorized_surface",
            "scan": "bound_ingestion_pipeline",
            "retrieval": "bound_retrieval_eligible_results",
            "graph": "bound_graph_traversal_answer",
            "source": "bound_source_fetch_exact_bytes",
            "reset": "bound_reset_preview_apply_outcome",
            "backfill": "bound_embedding_backfill_outcome",
        }
        test_file = f"code-binding.{row.backend}.test.ts"
        return CommandSpec(["pnpm", "--filter", "@engram/node", "exec", "vitest", "run", test_file, "-t", native[row.operation], "--reporter", "verbose"], native[row.operation])
    mcp = {
        "binding": "task_021::stdio_dispatch_parity" if row.transport == "mcp-stdio" else "task_022::oneshot_dispatch_parity",
        "scan": "task_021::stdio_dispatch_parity" if row.transport == "mcp-stdio" else "task_022::oneshot_dispatch_parity",
        "retrieval": "task_021::stdio_dispatch_parity" if row.transport == "mcp-stdio" else "task_022::oneshot_dispatch_parity",
        "graph": "task_021::stdio_dispatch_parity" if row.transport == "mcp-stdio" else "task_022::oneshot_dispatch_parity",
        "source": "task_021::stdio_dispatch_parity" if row.transport == "mcp-stdio" else "task_022::oneshot_dispatch_parity",
        "reset": "task_021::stdio_dispatch_parity" if row.transport == "mcp-stdio" else "task_022::oneshot_dispatch_parity",
        "backfill": "task_021::stdio_backfill_not_rescan" if row.transport == "mcp-stdio" else "task_022::backfill_not_rescan",
    }
    marker = mcp[row.operation].split("::")[-1]
    return CommandSpec(cargo_spec("engram-mcp", "bound_transports", mcp[row.operation], marker, row.backend).argv, marker)


class MatrixRunner:
    def __init__(self, run: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run):
        self.run = run

    def preflight_pg(self, url_name: str, environment: dict[str, str]) -> dict[str, str]:
        path = environment.get(PG_ENV_FILE)
        if not path:
            raise ValueError(f"{PG_ENV_FILE} is required for PG rows")
        fixture = load_private_pg_env(path, url_name)
        if environment.get(url_name) != fixture[url_name]:
            raise ValueError("PG URL must be injected from the approved private env file")
        parsed = urlparse(fixture[url_name])
        validate_pg_url(fixture[url_name])
        # This is the r5 private-fixture identity pattern: inspect the fixed
        # disposable container, bind the private URL to its loopback mapping,
        # and query its database COMMENT before a child operation can start.
        if fixture.get("PG_FIXTURE_CONTAINER_ID", PINNED_CONTAINER_ID) != PINNED_CONTAINER_ID:
            raise ValueError("PG fixture identity mismatch; refusing matrix operation")
        try:
            inspected = self.run(["docker", "inspect", PINNED_CONTAINER_ID], text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env={"PATH": environment.get("PATH", "")}, cwd=ROOT)
            info = json.loads(inspected.stdout)[0]
            ports = info.get("NetworkSettings", {}).get("Ports", {}).get("5432/tcp") or []
            loopback_port = next((str(item["HostPort"]) for item in ports if item.get("HostIp") == "127.0.0.1"), None)
            valid_identity = (inspected.returncode == 0 and info.get("Id") == PINNED_CONTAINER_ID and info.get("State", {}).get("Running") is True and info.get("Config", {}).get("Labels", {}) == PINNED_LABELS and info.get("Mounts") == [] and info.get("HostConfig", {}).get("Tmpfs") == {"/var/lib/postgresql/data": "rw,size=2g"} and loopback_port == str(parsed.port) and parsed.hostname == "127.0.0.1" and parsed.path.lstrip("/") == PINNED_DATABASE)
        except (OSError, ValueError, KeyError, IndexError, TypeError, json.JSONDecodeError) as error:
            raise ValueError("PG fixture identity probe unavailable; refusing matrix operation") from error
        if not valid_identity:
            raise ValueError("PG fixture identity mismatch; refusing matrix operation")
        from urllib.parse import unquote
        probe_env = dict(environment)
        probe_env.update(fixture)
        probe_env.update({"PGUSER": unquote(parsed.username or ""), "PGPASSWORD": unquote(parsed.password or ""), "PGDATABASE": PINNED_DATABASE})
        try:
            probe = self.run(["docker", "exec", "--env", "PGPASSWORD", "--env", "PGUSER", "--env", "PGDATABASE", PINNED_CONTAINER_ID, "psql", "-Atqc", "SELECT shobj_description(oid,'pg_database') FROM pg_database WHERE datname=current_database()"], text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=probe_env, cwd=ROOT)
        except OSError as error:
            raise ValueError("PG marker probe unavailable; refusing matrix operation") from error
        if probe.returncode or probe.stdout.strip() != MARKER:
            raise ValueError("PG database marker mismatch; refusing matrix operation")
        return fixture

    def execute(self, rows: list[Row], commands: Callable[[Row], CommandSpec], receipt_dir: Path, environment: dict[str, str], fail_on_skip: bool) -> int:
        receipt_dir.mkdir(parents=True, exist_ok=True)
        failed = skipped_required = transport_gaps = 0
        for ordinal, row in enumerate(rows, 1):
            receipt = {"row": row.name, "required": row.required, "argv": [], "exit": None, "status": "skipped", "output": "", "marker": None, "surface": "transport", "transport_surface": None}
            try:
                spec = commands(row)
                receipt["argv"] = safe_argv(spec.argv)
                receipt["marker"] = spec.marker
                receipt["surface"] = spec.surface
                receipt["transport_surface"] = spec.transport_surface
                result = self.run(spec.argv, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=environment, cwd=ROOT)
                receipt["exit"] = result.returncode
                receipt["output"] = redact(result.stdout or "")[-4096:]
                marker_seen = spec.marker in receipt["output"]
                # A marker printed by an ignored Cargo case is not behavioral
                # evidence.  Every selected suite must execute at least one
                # test, regardless of process exit status or marker presence.
                import re
                executed_tests = sum(int(count) for count in re.findall(r"\b(\d+) passed\b", receipt["output"]))
                receipt["executed_tests"] = executed_tests
                if result.returncode == 0 and marker_seen and executed_tests > 0:
                    receipt["status"] = "passed-transport-gap" if spec.transport_surface else "passed"
                    transport_gaps += spec.transport_surface is not None
                else:
                    receipt["status"] = "failed"
                    if result.returncode == 0:
                        reason = f"missing behavioral marker: {spec.marker}" if not marker_seen else "selected suite executed zero tests"
                        receipt["output"] = (receipt["output"] + f"\n{reason}")[-4096:]
                    failed += 1
            except FileNotFoundError as error:
                receipt["output"] = redact(str(error))
                receipt["status"] = "skipped"
            except ValueError as error:
                receipt["output"] = redact(str(error))
                receipt["status"] = "skipped"
            if receipt["status"] == "skipped" and row.required:
                skipped_required += 1
            payload = json.dumps(receipt, sort_keys=True) + "\n"
            receipt_path = receipt_dir / f"{ordinal:03d}-{row.operation}-{row.backend}-{row.transport}.json"
            receipt_path.write_text(payload, encoding="utf-8")
        manifest_receipts = []
        for receipt_path in sorted(receipt_dir.glob("[0-9][0-9][0-9]-*.json")):
            receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
            manifest_receipts.append({
                "exit": receipt["exit"],
                "path": receipt_path.name,
                "required": receipt["required"],
                "row": receipt["row"],
                "sha256": hashlib.sha256(receipt_path.read_bytes()).hexdigest(),
                "status": receipt["status"],
            })
        (receipt_dir / "manifest.json").write_text(
            json.dumps({"receipt_count": len(manifest_receipts), "transport_gap_count": transport_gaps, "receipts": manifest_receipts}, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        if failed or (fail_on_skip and skipped_required):
            return 1
        return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sqlite-temp", action="store_true", help="create a temporary file-backed SQLite root")
    parser.add_argument("--pg-url-env", default="ENGRAM_TEST_PG_URL", help="environment variable containing the approved PG URL")
    parser.add_argument("--require-native", action="store_true")
    parser.add_argument("--require-mcp", action="store_true")
    parser.add_argument("--fail-on-skip", action="store_true")
    parser.add_argument("--receipt-dir", type=Path)
    parser.add_argument("--stub-command", help="synthetic control-only executable; runs once per configured row")
    args = parser.parse_args(argv)
    if not args.sqlite_temp:
        parser.error("--sqlite-temp is required; SQLite rows must use temporary file stores")
    environment = dict(os.environ)
    require_pg = bool(environment.get("ENGRAM_TEST_REQUIRE_PG"))
    runner = MatrixRunner()
    if require_pg:
        try:
            environment.update(runner.preflight_pg(args.pg_url_env, environment))
        except ValueError as error:
            print(f"refused PG matrix: {redact(str(error))}", file=sys.stderr)
            return 2
    elif args.fail_on_skip:
        # A required PG request with no usable credentials is a refusal, not a green skip.
        print("refused PG matrix: ENGRAM_TEST_REQUIRE_PG=1 and approved credentials are required", file=sys.stderr)
        return 2
    root_context = tempfile.TemporaryDirectory(prefix="engram-bound-code-matrix-")
    with root_context as temp:
        sqlite_root = Path(temp)
        receipt_dir = args.receipt_dir or sqlite_root / "receipts"
        rows = matrix_rows(args.require_native, args.require_mcp, require_pg)
        def commands(row: Row) -> CommandSpec:
            if row.backend == "pgvector" and not require_pg:
                raise ValueError("PG credentials/service unavailable")
            if args.stub_command:
                return CommandSpec([args.stub_command, row.name], row.name)
            return default_command(row, sqlite_root, args.pg_url_env)
        return runner.execute(rows, commands, receipt_dir, environment, args.fail_on_skip)


if __name__ == "__main__":
    raise SystemExit(main())
