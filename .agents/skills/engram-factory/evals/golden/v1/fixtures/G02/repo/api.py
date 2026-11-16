def reports(role):
    return {"status": 200, "items": ["quarterly"]}

def export_report(role, dry_run=False):
    if role != "admin":
        return {"status": 403, "error": "forbidden"}
    return {"status": 200, "written": not dry_run}
