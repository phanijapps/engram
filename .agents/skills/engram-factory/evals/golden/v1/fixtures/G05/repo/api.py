def request_id(headers):
    value = headers.get("X-Request-ID")
    if value is None:
        return 400, {"error": "missing X-Request-ID"}
    return 200, {"request_id": value}
