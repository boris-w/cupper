"""Print one table row per message from every *.server.jsonl in a log directory."""

import glob
import json
import os
import statistics
import sys


def row(r):
    v = r.get("verdict") or {}
    d = v.get("detail") or {}
    info = d.get("info") or {}
    err = d.get("error") or {}
    if isinstance(err, str):
        err = {"message": err}
    status = err.get("status", d.get("status", ""))
    msg = err.get("message", d.get("message", "")) or ", ".join(d.get("reasons") or [])
    who = r.get("msg", "").replace("hello from ", "")
    return [
        who[:44],
        str(r.get("peer_pid_now")),
        "chk" if r.get("checked") else "-",
        {True: "ACCEPT", False: "reject", None: ""}[v.get("accept")] if r.get("checked") else "",
        v.get("stage", ""),
        str(status),
        (msg or "")[:70],
        str(info.get("identifier") or ""),
        "+".join(info.get("dynamic_status_names") or []),
        "conn=" + ("ACCEPT" if r.get("connection_accepted") else "reject"),
    ]


def main(d):
    for f in sorted(glob.glob(os.path.join(d, "*.server.jsonl"))):
        name = os.path.basename(f).replace(".server.jsonl", "")
        recs = [json.loads(l) for l in open(f) if l.strip()]
        print(f"\n### {name}")
        if recs and recs[0].get("policy"):
            print("policy:", json.dumps(recs[0]["policy"]))
        if name.startswith("H-"):
            t = [
                (r["verdict"]["detail"].get("copy_us", 0), r["verdict"]["detail"].get("check_us", 0))
                for r in recs
                if r.get("checked") and r.get("verdict")
            ]
            copy = [a for a, _ in t]
            total = [b for _, b in t]
            print(f"n={len(t)} copy_guest median={statistics.median(copy)}us; "
                  f"copy+check median={statistics.median(total)}us max={max(total)}us")
            continue
        for r in recs:
            print(" | ".join(row(r)))


main(sys.argv[1])
