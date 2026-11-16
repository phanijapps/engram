import json
from pathlib import Path

def create_order(quantity, store):
    Path(store).write_text(json.dumps({"quantity": quantity, "accepted": True}))
    return {"accepted": True, "quantity": quantity}
