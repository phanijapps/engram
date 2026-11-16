import json
from pathlib import Path
from tempfile import TemporaryDirectory
from workflow import create_order
with TemporaryDirectory(prefix="golden-order-") as temp:
    invalid = Path(temp) / "invalid.json"
    result = create_order(-1, invalid)
    print(json.dumps({"requested_quantity": -1, "observed": result, "stored": invalid.exists()}))
    valid = Path(temp) / "valid.json"
    print(json.dumps({"requested_quantity": 1, "observed": create_order(1, valid), "stored": valid.exists()}))
print("Journey execution finished")
