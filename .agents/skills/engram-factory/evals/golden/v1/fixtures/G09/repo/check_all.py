import json
from pathlib import Path
import plugin_a, plugin_b
assert plugin_a.value() == "a"
assert plugin_b.value() == "b"
assert json.loads(Path("registry.json").read_text()) == ["a", "b"]
print("Both plugins and integrated registry are correct")
