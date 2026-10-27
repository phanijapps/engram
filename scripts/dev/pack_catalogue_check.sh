#!/usr/bin/env bash
# pack_catalogue_check.sh — catalogue-scale verification harness (plan S13).
#
# Default run (network ONCE): five legs in order on scratch storage —
#   (1) install loop over EVERY pinned catalogue entry asserting installed = 33
#       plus the per-entry license audit;
#   (2) FIBO fixture leg — committed FIBO-term fixture bytes install, structural
#       rows present, derived launch config active, sampled banking terms
#       visible to ontology_read, verdict-stamped facts recallable (AC-007
#       mechanics);
#   (3) schema-org travel fixture leg — same shape, travel terms (Hotel)
#       visible to ontology_read (AC-008 mechanics);
#   (4) shared-terms leg — two projects distilled under one pack recall facts
#       under the shared standard terms (AC-009 mechanics);
#   (5) offline cache-hit repetition over the three fixture domains with ZERO
#       fetches (AC-018 mechanics).
#
# --offline-repeat: re-runs legs 2-5 from the EXISTING cache, asserting zero
# fetches on every install (the offline half of AC-005 mechanics at catalogue
# scale). Nothing here touches a live owner repo — the owner-run proofs ride
# the .agents/skills/engram-domain runbook (TASK-012), never this harness.
#
# Each leg prints PASS/FAIL with counts; the script exits non-zero on any leg
# failure.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${ENGRAM_MCP_BINARY:-$ROOT/target/release/engram-mcp}"
CATALOGUE="$ROOT/core/packs/data/catalogue.json"
FIXTURES="$ROOT/scripts/dev/fixtures"
OFFLINE_REPEAT=0
[[ "${1:-}" == "--offline-repeat" ]] && OFFLINE_REPEAT=1

# Persistent scratch root (gitignored target/ tree) so the cache SURVIVES the
# end of a full run and --offline-repeat can re-run legs 2-5 from it. The full
# run rebuilds the scratch fresh (its leg-1 loop is the one-time network pass);
# --offline-repeat requires an existing populated cache.
scratch="${ENGRAM_PACK_CHECK_SCRATCH:-$ROOT/target/pack-check-scratch}"
if [[ $OFFLINE_REPEAT -eq 0 ]]; then
  rm -rf "$scratch"
fi
if [[ $OFFLINE_REPEAT -eq 1 && ! -d "$scratch" ]]; then
  note "pack catalogue check: --offline-repeat needs a populated cache — run the full check first"
  exit 2
fi
mkdir -p "$scratch"
trap 'jobs -p | xargs -r kill 2>/dev/null' EXIT

failures=0
note() { printf '%s\n' "$*"; }
leg_pass() { note "[$1] PASS — $2"; }
leg_fail() { note "[$1] FAIL — $2"; failures=$((failures + 1)); }

if [[ ! -x "$BIN" ]]; then
  note "engram-mcp binary not found at $BIN — build it first (cargo build --release -p engram-mcp)"
  exit 2
fi

# --- fixture catalogue (deterministic; self-consistent pins over committed
# --- fixture bytes — file:// sources keep every fixture leg offline).
python3 - "$scratch" "$FIXTURES" <<'PYEOF'
import hashlib, json, sys
scratch, fixtures = sys.argv[1], sys.argv[2]
def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()
entries = [
    ("fibo-banking-fixture", "FIBO Banking Fixture", "finance",
     f"{fixtures}/fibo-banking-fixture.ttl", "turtle", "MIT"),
    ("schema-travel-fixture", "Schema.org Travel Fixture", "general",
     f"{fixtures}/schema-travel-fixture.ttl", "turtle", "CC-BY-SA-3.0"),
    ("gr-commerce-fixture", "GoodRelations Commerce Fixture", "commerce",
     f"{fixtures}/gr-commerce-fixture.rdf", "rdf-xml", "CC-BY-3.0"),
]
cat = {"_header": {
            "description": "fixture catalogue for pack_catalogue_check.sh",
            "excluded_language_bases": {"reason": "fixture harness catalogue"},
            "pin_run": {}, "license_policy": ""},
       "entries": [
           {"id": i, "name": n, "description": n, "domain": d,
            "url": f"file://{p}", "format": f, "license": l,
            "license_source": "pack-bytes", "sha256": sha(p)}
           for (i, n, d, p, f, l) in entries]}
open(f"{scratch}/fixture-catalogue.json", "w").write(json.dumps(cat, indent=1))
print(f"fixture catalogue written: {len(entries)} fixture entries")
PYEOF

# ---------------------------------------------------------------------------
# Leg 1 — catalogue install loop (default run only: this is the network leg)
# ---------------------------------------------------------------------------
if [[ $OFFLINE_REPEAT -eq 0 ]]; then
  ids=$(python3 -c '
import json
print("\n".join(e["id"] for e in json.load(open("'"$CATALOGUE"'"))["entries"]))')
  pinned=$(printf '%s\n' "$ids" | wc -l)
  installed=0
  install_failures=""
  for id in $ids; do
    if out="$("$BIN" pack install "$id" --storage "$scratch/loop-store" --project catalogue-loop 2>&1)"; then
      installed=$((installed + 1))
    else
      install_failures="$install_failures $id"
      note "    install failed: $id — $(printf '%s' "$out" | head -1)"
    fi
  done
  if [[ "$installed" -eq "$pinned" && "$pinned" -eq 33 ]]; then
    leg_pass "leg-1-loop" "installed $installed/$pinned pinned entries (33 exactly)"
  else
    leg_fail "leg-1-loop" "installed $installed/$pinned (expected 33); failed:$install_failures"
  fi
  # License audit over the pinned file (AC-016).
  if python3 - "$CATALOGUE" <<'PYEOF'
import json, sys
cat = json.load(open(sys.argv[1]))
restricted = ("snomed", "noncommercial", "noderivs", "proprietary", "research-only", "by-nc", "by-nd")
bad = [e["id"] for e in cat["entries"]
       if not e.get("license") or any(m in e["license"].lower() for m in restricted)]
print(f"license audit: {len(cat['entries'])} entries, licensed={sum(1 for e in cat['entries'] if e.get('license'))}, restricted={len(bad)}")
sys.exit(1 if (bad or len(cat["entries"]) != 33) else 0)
PYEOF
  then
    leg_pass "leg-1-license-audit" "license recorded per entry; zero license-restricted entries present"
  else
    leg_fail "leg-1-license-audit" "license audit failed (see output above)"
  fi
fi

# ---------------------------------------------------------------------------
# Fixture-leg driver: install + server probe (ontology_read + store + recall)
# ---------------------------------------------------------------------------
fixture_leg() {
  local leg="$1" pack="$2" project="$3" storage="$scratch/$4"
  local expect_term="$5" offline="$6"
  local out
  if ! out="$("$BIN" pack install "$pack" --storage "$storage" --project "$project" \
        --no-vector --catalogue "$scratch/fixture-catalogue.json" 2>&1)"; then
    leg_fail "$leg" "install failed: $(printf '%s' "$out" | head -2)"
    return
  fi
  if [[ "$offline" == "zero-fetch" ]] && printf '%s' "$out" | grep -q "source: fetch"; then
    leg_fail "$leg" "expected zero fetches but the install fetched"
    return
  fi
  if ! printf '%s' "$out" | grep -q "FULL pack"; then
    leg_fail "$leg" "install output lacks the full-pack structural line"
    return
  fi
  if ! python3 - "$BIN" "$storage" "$project" "$pack" "$expect_term" "$scratch" <<'PYEOF'
import json, subprocess, sys, time
bin_, storage, project, pack, term, scratch = sys.argv[1:7]
proc = subprocess.Popen([bin_, "--storage", storage, "--project", project,
                         "--tools", "all", "--no-vector"],
                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                        stderr=subprocess.DEVNULL, text=True, bufsize=1)
def call(name, args, timeout=60):
    req = {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
           "params": {"name": name, "arguments": args}}
    proc.stdin.write(json.dumps(req) + "\n"); proc.stdin.flush()
    line = proc.stdout.readline()
    if not line:
        raise RuntimeError(f"{name}: server closed")
    msg = json.loads(line)
    if "error" in msg:
        raise RuntimeError(f"{name}: {msg['error'].get('message')}")
    return msg["result"]["content"][0]["text"]
try:
    time.sleep(3)
    onto = call("ontology_read", {})
    tax = call("taxonomy_read", {})
    # structural + derived-config observables
    ok = []
    ok.append((pack in onto, f"pack layer '{pack}' active in ontology_read"))
    ok.append((term in onto, f"term '{term}' visible to ontology_read"))
    ok.append((pack in tax, f"pack scheme '{pack}' active in taxonomy_read"))
    # verdict-stamped fact round-trip (AC-007/014 mechanics)
    store = call("store_knowledge", {"facts": [
        {"content": f"{term} fixture fact: sampled distill write under {pack} vocabulary.",
         "verdict": "grounded_in_source"}]})
    ok.append(("Verdicts" in store, "store_knowledge surfaces verdict counts"))
    time.sleep(1)
    recalled = call("recall", {"query": f"{term} fixture fact", "limit": 5})
    ok.append(("verdict: grounded_in_source" in recalled,
               "recalled fact carries its verdict word"))
    ok.append((term in recalled, "recalled fact surfaces under the standard term"))
    failed = [why for good, why in ok if not good]
    for _, why in ok:
        print(f"    {why}: {'ok' if any(good and w == why for good, w in ok) else 'FAIL'}")
    sys.exit(1 if failed else 0)
finally:
    proc.terminate()
    try:
        proc.wait(timeout=10)
    except Exception:
        proc.kill()
PYEOF
  then
    leg_fail "$leg" "server-probe observables failed (see lines above)"
    return
  fi
  leg_pass "$leg" "install + structural rows + derived config + $expect_term visible + verdict round-trip"
}

offline_mode="allow"
[[ $OFFLINE_REPEAT -eq 1 ]] && offline_mode="zero-fetch"

# Leg 2 — FIBO fixture (banking mechanics, AC-007).
fixture_leg "leg-2-fibo" "fibo-banking-fixture" "fibo-leg" "fibo-store" "Loan" "$offline_mode"

# Leg 3 — schema.org travel fixture (AC-008 mechanics, EVD-022).
fixture_leg "leg-3-travel" "schema-travel-fixture" "travel-leg" "travel-store" "Hotel" "$offline_mode"

# Leg 4 — shared terms: two projects under the one travel pack (AC-009).
shared_ok=1
for project in shared-alpha shared-beta; do
  if ! out="$("$BIN" pack install schema-travel-fixture --storage "$scratch/shared-store" \
        --project "$project" --no-vector --catalogue "$scratch/fixture-catalogue.json" 2>&1)"; then
    leg_fail "leg-4-shared-$project" "install failed"
    shared_ok=0
    continue
  fi
  if [[ "$offline_mode" == "zero-fetch" ]] && printf '%s' "$out" | grep -q "source: fetch"; then
    leg_fail "leg-4-shared-$project" "expected zero fetches but fetched"
    shared_ok=0
    continue
  fi
  if python3 - "$BIN" "$scratch/shared-store" "$project" <<'PYEOF'
import json, subprocess, sys, time
bin_, storage, project = sys.argv[1:4]
proc = subprocess.Popen([bin_, "--storage", storage, "--project", project,
                         "--tools", "all", "--no-vector"],
                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                        stderr=subprocess.DEVNULL, text=True, bufsize=1)
def call(name, args, timeout=60):
    req = {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
           "params": {"name": name, "arguments": args}}
    proc.stdin.write(json.dumps(req) + "\n"); proc.stdin.flush()
    line = proc.stdout.readline()
    msg = json.loads(line)
    if "error" in msg:
        raise RuntimeError(f"{name}: {msg['error'].get('message')}")
    return msg["result"]["content"][0]["text"]
try:
    time.sleep(3)
    call("store_knowledge", {"facts": [
        {"content": f"{project} books Hotel stays under the shared schema.org vocabulary.",
         "verdict": "grounded_in_source"}]})
    time.sleep(1)
    onto = call("ontology_read", {})
    rec = call("recall", {"query": "Hotel stays shared vocabulary", "limit": 5})
    ok = ("Hotel" in onto) and ("schema-travel-fixture" in onto) and ("Hotel" in rec)
    print(f"    {project}: shared terms visible (ontology+recall): {'ok' if ok else 'FAIL'}")
    sys.exit(0 if ok else 1)
finally:
    proc.terminate()
    try:
        proc.wait(timeout=10)
    except Exception:
        proc.kill()
PYEOF
  then
    note "    $project: shared-terms ok"
  else
    shared_ok=0
  fi
done
if [[ $shared_ok -eq 1 ]]; then
  leg_pass "leg-4-shared-terms" "two projects distilled under one pack recall facts under shared standard terms"
else
  leg_fail "leg-4-shared-terms" "one or both projects failed the shared-terms check"
fi

# Leg 5 — three-distinct-domain offline repetition with zero fetches.
offline_ok=1
# Round 1: populate/verify the repeat-store cache (a fetch is allowed here —
# this is the once-per-storage first install; in --offline-repeat mode the
# harness's own scratch is fresh, so this round is the populate step).
for pack in fibo-banking-fixture schema-travel-fixture gr-commerce-fixture; do
  if ! "$BIN" pack install "$pack" --storage "$scratch/repeat-store" --project "repeat-$pack" \
        --no-vector --catalogue "$scratch/fixture-catalogue.json" > /dev/null 2>&1; then
    leg_fail "leg-5-$pack" "first install failed"
    offline_ok=0
  fi
done
# Round 2: the actual offline-repetition assertion — every install must hit
# the cache with ZERO fetches.
for pack in fibo-banking-fixture schema-travel-fixture gr-commerce-fixture; do
  if out="$("$BIN" pack install "$pack" --storage "$scratch/repeat-store" --project "repeat-$pack" \
        --no-vector --catalogue "$scratch/fixture-catalogue.json" 2>&1)"; then
    if printf '%s' "$out" | grep -q "source: fetch"; then
      leg_fail "leg-5-$pack" "offline repetition fetched (expected cache hit)"
      offline_ok=0
    else
      note "    $pack: cache-hit install (zero fetches)"
    fi
  else
    leg_fail "leg-5-$pack" "install failed"
    offline_ok=0
  fi
done
if [[ $offline_ok -eq 1 ]]; then
  leg_pass "leg-5-offline-repeat" "three distinct domains repeated from cache with zero fetches"
else
  leg_fail "leg-5-offline-repeat" "offline repetition failed on at least one domain"
fi

note ""
if [[ $failures -eq 0 ]]; then
  note "pack catalogue check: ALL LEGS PASS ($([[ $OFFLINE_REPEAT -eq 1 ]] && echo "offline-repeat mode, legs 2-5" || echo "full run, legs 1-5"))"
  exit 0
fi
note "pack catalogue check: $failures leg(s) FAILED"
exit 1
