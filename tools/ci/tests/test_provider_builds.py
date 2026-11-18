import unittest
from tools.ci.check_provider_builds import SQLITE, package_names, violations, forbidden_dependencies, provider_subsets, compiler_units


class ProviderBuildTests(unittest.TestCase):
    def test_runtime_command_is_fixed_to_actual_repo_consumer_not_printed_boolean(self):
        from tools.ci.check_provider_builds import runtime_command,ROOT
        self.assertEqual(runtime_command('engram-provider-e2e','/owned/consumer','/owned/request'),['/owned/consumer','--request','/owned/request'])
        self.assertEqual(runtime_command('engram-node','/owned/addon.node','/owned/request'),['node',str(ROOT/'packages/node/test/provider-e2e.mjs'),'--request','/owned/request','--artifact','/owned/addon.node'])
        with self.assertRaises(ValueError):runtime_command('echo','/owned/artifact','/owned/request')
    def test_normal_conformance_edge_is_always_rejected(self):
        self.assertTrue(
            violations({"engram-node", "engram-conformance"}, SQLITE),
            "a production conformance edge must fail even without SQLite",
        )

    def test_injected_normal_sqlite_edge_is_rejected(self):
        production = package_names("engram-integration v0.1.0\nengram-store-sqlite v0.1.0\nrusqlite v0.32.0\n")
        self.assertTrue(violations(production, SQLITE), "injected normal SQLite production edge must be rejected")

    def test_dev_sqlite_edge_is_separate(self):
        production = package_names("engram-backend-pgvector v0.1.0\nengram-store-pgvector v0.1.0\n")
        development = production | {"engram-store-sqlite", "rusqlite"}
        self.assertFalse(violations(production, SQLITE))
        self.assertTrue(violations(development, SQLITE))

    def test_build_sqlite_and_transitive_native_edges_are_rejected(self):
        for package in SQLITE:
            self.assertTrue(violations({"engram-integration", package}, SQLITE), package)

    def test_neutral_embedding_graph_is_accepted(self):
        self.assertFalse(violations({"engram-embedding-fastembed", "fastembed", "ort"}, SQLITE))

    def test_every_nonempty_provider_subset_and_both_modes_are_explicit(self):
        self.assertEqual(len(provider_subsets("explicit-combinations")), 15)
        self.assertIn(("helixdb-service", "helixdb-embedded"), provider_subsets("single"))
        self.assertEqual(len(set(provider_subsets("explicit-combinations"))), 15)

    def test_unselected_postgres_and_conformance_do_not_hide_behind_sqlite_check(self):
        forbidden = forbidden_dependencies(("helixdb-embedded",))
        for edge in ("engram-conformance", "engram-backend-pgvector", "tokio-postgres", "engram-backend-sqlite", "rusqlite"):
            self.assertTrue(violations({edge}, forbidden), edge)
        self.assertFalse(violations({"engram-store-helixdb", "fastembed"}, forbidden))

    def test_production_compiler_units_are_separate_from_dev_metadata(self):
        import json
        message = {"reason":"compiler-artifact","package_id":"path+file:///fixture#engram-node@0.1.0","features":["helixdb-service"],"filenames":["/owned/native.so"]}
        packages, artifacts = compiler_units(json.dumps(message) + '\n' + json.dumps({"reason":"compiler-message","message":{"rendered":"dev metadata mentions engram-conformance"}}))
        self.assertEqual(packages, {"engram-node"})
        self.assertEqual(len(artifacts), 1)


class CargoGraphTests(unittest.TestCase):
    """Use real local Cargo graphs: no service, registry access or compilation."""

    def test_real_normal_build_and_dev_dependency_graphs(self):
        import os
        from pathlib import Path
        import subprocess
        import tempfile
        from unittest.mock import patch
        from tools.ci import check_provider_builds as checker

        for edge in ("dependencies", "build-dependencies", "dev-dependencies", "feature"):
            with self.subTest(edge=edge), tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR")) as directory:
                root = Path(directory)
                for name in ("fixture-root", "bridge", "rusqlite"):
                    crate = root / name
                    (crate / "src").mkdir(parents=True)
                    (crate / "src/lib.rs").write_text("")
                    manifest = f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n'
                    if name == "fixture-root":
                        if edge == "feature":
                            manifest += '[features]\nsqlite-edge = ["dep:bridge"]\n[dependencies]\nbridge = { path = "../bridge", optional = true }\n'
                        else:
                            manifest += f'[{edge}]\nbridge = {{ path = "../bridge" }}\n'
                    elif name == "bridge":
                        manifest += '[dependencies]\nrusqlite = { path = "../rusqlite" }\n'
                    (crate / "Cargo.toml").write_text(manifest)
                (root / "Cargo.toml").write_text('[workspace]\nmembers = ["fixture-root", "bridge", "rusqlite"]\nresolver = "2"\n')
                subprocess.run(["cargo", "+1.88.0", "generate-lockfile", "--offline"], cwd=root, check=True, capture_output=True)
                with patch.object(checker, "ROOT", root):
                    if edge == "feature":
                        self.assertFalse(checker.violations(checker.dependency_graph("fixture-root", "", "normal,build"), SQLITE))
                    features = "sqlite-edge" if edge == "feature" else ""
                    production = checker.dependency_graph("fixture-root", features, "normal,build")
                    tests = checker.dependency_graph("fixture-root", features, "normal,build,dev")
                self.assertIn("rusqlite", tests)
                self.assertEqual(bool(checker.violations(production, SQLITE)), edge != "dev-dependencies")
                self.assertEqual("rusqlite" in production, edge != "dev-dependencies")
