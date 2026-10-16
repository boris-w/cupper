"""Render $SP5_WORKDIR/results/*.json into one Markdown file (results/results.md).

Usage: python3 render.py
"""
from __future__ import annotations

import sys

from lib import RESULTS, load_result, md_table, sanitize


def ms(d, key):
    v = d.get(key)
    if not v:
        return ""
    return f"{v['median_ms']:.0f}" if v["median_ms"] >= 10 else f"{v['median_ms']:.1f}"


def ch(c):
    return f"{c['modify']}/{c['add']}/{c['delete']}"


def main() -> None:
    out = []
    env = load_result("env")
    if env:
        e = env["env"]
        out += ["## Environment", "", md_table(["item", "value"], [(k, v) for k, v in e.items()]), ""]
        out += ["## Sample repositories", "",
                md_table(["repo", "tracked files", "tracked MB", "index bytes", "HEAD"],
                         [(r["name"], r["files"], r["tracked_mb"], r["index_bytes"], r["head"]) for r in env["repos"]]),
                ""]
    b = load_result("bench_snapshot")
    if b:
        out += [f"## Snapshot cost (median of {b['runs']} runs, ms)", "",
                "changes = modified / new untracked / deleted files; every run writes new content.", ""]
        rows = []
        for r in b["repos"]:
            for sc, d in r["scenarios"].items():
                rows.append((r["repo"], sc, ch(d["changes"]), ms(d, "status"), ms(d, "snap_seed"), ms(d, "snap_keep"),
                             ms(d, "snap_private"), ms(d, "snap_empty"), ms(d, "stash_create"),
                             "yes" if d.get("user_index_untouched") else "NO",
                             "yes" if d.get("stash_create_touched_index") else "no",
                             f"{d['odb_growth']['loose_objects']} / {d['odb_growth']['loose_kb']} KB"))
        out += [md_table(["repo", "scenario", "changes", "status", "snap seed", "snap keep", "snap private ODB",
                          "snap empty", "stash create", "user index untouched", "stash touched index",
                          "new loose objects / size"], rows), ""]
        out += ["## Diff cost (ms)", "", "T0 = clean snapshot, T1 = scenario snapshot.", ""]
        rows = []
        for r in b["repos"]:
            for sc, d in r["scenarios"].items():
                if sc == "clean":
                    continue
                rows.append((r["repo"], sc, ms(d, "diff_raw"), ms(d, "diff_numstat"), ms(d, "diff_patch_all"),
                             f"{d['diff_patch_all']['bytes'] / 1024:.0f} KB", ms(d, "diff_one_file"),
                             ms(d, "snapshot_vs_worktree"), ms(d, "tree_vs_worktree_tracked_only"),
                             ms(d, "unstaged_numstat")))
        out += [md_table(["repo", "scenario", "diff-tree --raw -M", "--numstat", "full patch", "patch size",
                          "one file patch", "snapshot + diff vs now", "diff T0 (tracked only)",
                          "unstaged numstat"], rows), ""]
        out += ["## Single-hunk operations (ms, small scenario)", ""]
        rows = []
        for r in b["repos"]:
            d = r["scenarios"].get("small")
            if d and "hunk_stage" in d:
                rows.append((r["repo"], ms(d, "hunk_diff_file"), ms(d, "hunk_stage"), ms(d, "hunk_unstage"),
                             ms(d, "hunk_revert"), "yes" if d["hunk_ops_correct"] else "NO"))
        out += [md_table(["repo", "git diff -- file", "stage (apply --cached)", "unstage (apply --cached -R)",
                          "revert (apply -R)", "content correct"], rows), ""]
        if b.get("fsmonitor"):
            out += ["## With core.fsmonitor=true + core.untrackedCache=true (ms)", ""]
            rows = []
            for r in b["fsmonitor"]:
                for sc, d in r["scenarios"].items():
                    rows.append((r["repo"], sc, ms(d, "status"), ms(d, "snap_seed"), ms(d, "snap_keep")))
            out += [md_table(["repo", "scenario", "status", "snap seed", "snap keep"], rows), ""]
    w = load_result("bench_worktree")
    if w:
        out += [f"## Worktree operations (median of {w['runs']} runs, ms)", ""]
        out += [md_table(["repo", "add -b (checkout)", "add --no-checkout", "safety check (1st, racy)",
                          "safety check", "remove (clean)", "rebuild from branch"],
                         [(r["repo"], ms(r, "add_b"), ms(r, "add_no_checkout"), ms(r, "safety_check_first"),
                           ms(r, "safety_check"), ms(r, "remove"), ms(r, "rebuild")) for r in w["repos"]]), ""]
    sh = load_result("bench_shadow")
    if sh:
        out += ["## Alternative: separate shadow repository (ms)", ""]
        out += [md_table(["repo", "first snapshot", "shadow store MB", "next (clean)", "next (small)",
                          "same tree as temp-index snapshot"],
                         [(r["repo"], r["first_snapshot_ms"], r["shadow_store_mb"], ms(r, "next_clean"),
                           ms(r, "next_small"), "yes" if r["same_tree_as_temp_index"] else "NO")
                          for r in sh["repos"]]), ""]
    lg = load_result("compare_libgit2")
    if lg:
        out += [f"## libgit2 ({lg['version']}) vs git CLI (ms)", ""]
        rows = []
        cli = {r["repo"]: r for r in (b or {}).get("repos", [])}
        for r in lg["repos"]:
            for sc, d in r["scenarios"].items():
                c = cli.get(r["repo"], {}).get("scenarios", {}).get(sc, {})
                rows.append((r["repo"], sc, ms(d, "lg2_status"), ms(c, "status"), ms(d, "lg2_snapshot"),
                             ms(c, "snap_seed"), "yes" if d["same_tree_as_cli"] else "NO",
                             ms(d, "lg2_diff_with_hunks"), ms(d, "cli_diff_patch"),
                             ms(d, "lg2_hunk_stage"), ms(d, "cli_hunk_stage")))
        out += [md_table(["repo", "scenario", "lg2 status", "cli status", "lg2 snapshot", "cli snapshot",
                          "same tree id", "lg2 diff+hunks", "cli diff -p", "lg2 hunk stage", "cli hunk stage"],
                         rows), ""]
        wt = {r["repo"]: r for r in (w or {}).get("repos", [])}
        out += [md_table(["repo", "lg2 add_worktree", "cli worktree add -b"],
                         [(r["repo"], ms(r, "lg2_add_worktree"), ms(wt.get(r["repo"], {}), "add_b"))
                          for r in lg["repos"]]), ""]
        if lg.get("lfs"):
            out += [f"LFS filter check: `{lg['lfs']}`", ""]
        if lg.get("compat"):
            out += [md_table(["libgit2 on special repo", "result", "detail"],
                             [(c["case"], c["result"], str(c.get("detail", "")).replace("|", "\\|").replace("\n", " "))
                              for c in lg["compat"]]), ""]
    gx = load_result("compare_gix")
    if gx:
        out += [f"## gitoxide ({gx['version']}) vs git CLI (ms)", "",
                "gix snapshot is an emulation (status -> write blobs -> edit HEAD tree): no clean filters, "
                "assumes nothing staged.", ""]
        cli = {r["repo"]: r for r in (b or {}).get("repos", [])}
        rows = []
        for r in gx["repos"]:
            for sc, d in r["scenarios"].items():
                c = cli.get(r["repo"], {}).get("scenarios", {}).get(sc, {})
                rows.append((r["repo"], sc, ms(d, "gix_status"), ms(c, "status"), ms(d, "gix_snapshot"),
                             ms(c, "snap_seed"), "yes" if d["same_tree_as_cli"] else "NO", ms(d, "gix_diff"),
                             ms(d, "cli_diff_raw")))
        out += [md_table(["repo", "scenario", "gix status", "cli status", "gix snapshot (emulated)", "cli snapshot",
                          "same tree id", "gix tree diff", "cli diff-tree --raw"], rows), ""]
        wt = {r["repo"]: r for r in (w or {}).get("repos", [])}
        out += [md_table(["repo", "gix add_worktree", "cli worktree add -b", "worktree status per CLI afterwards"],
                         [(r["repo"], ms(r, "gix_wtadd"), ms(wt.get(r["repo"], {}), "add_b"),
                           (r.get("gix_worktree_status_by_cli") or "clean").replace("\n", " ")[:60])
                          for r in gx["repos"]]), ""]
        if gx.get("compat"):
            out += [md_table(["gix status on special repo", "result", "detail"],
                             [(c["case"], c["result"], c.get("detail", "")) for c in gx["compat"]]), ""]
    backends = [("git CLI", load_result("edge_hunk_git-cli")), ("libgit2", load_result("edge_hunk_libgit2")),
                ("libgit2+fix", load_result("edge_hunk_libgit2+fix"))]
    backends = [(n, r) for n, r in backends if r]
    if backends:
        out += ["## Hunk edge cases (✅ = result matches the expected content)", "",
                "libgit2+fix = libgit2 with each single-hunk header rewritten to new_start = old_start.", ""]
        keys, maps = [], []
        for _, res in backends:
            m = {}
            for r in res["rows"]:
                k = (r["case"], r["op"])
                if k not in keys:
                    keys.append(k)
                m[k] = r
            maps.append(m)
        rows = []
        for k in keys:
            marks, details = [], []
            for (bname, _), m in zip(backends, maps):
                r = m.get(k)
                marks.append(("✅" if r["ok"] else "❌") if r else "–")
                if r and r["detail"] and (not r["ok"] or bname == "git CLI"):
                    details.append(f"{bname}: {r['detail'][:160]}")
            rows.append([k[0], k[1], *marks, " / ".join(details)])
        out += [md_table(["case", "operation", *[n for n, _ in backends], "detail"],
                         [[str(x).replace("|", "\\|") for x in r] for r in rows]), ""]
    es = load_result("edge_snapshot")
    if es:
        out += ["## Snapshot boundary cases", ""]
        out += [md_table(["case", "check", "result", "detail"],
                         [(r["case"], r["check"], "yes" if r["ok"] else "NO", str(r["detail"]).replace("|", "\\|"))
                          for r in es["rows"]]), ""]
    ew = load_result("edge_worktree")
    if ew:
        out += ["## Worktree safety / cleanup cases", ""]
        out += [md_table(["case", "check", "result", "detail"],
                         [(r["case"], r["check"], r["result"], str(r["detail"]).replace("|", "\\|"))
                          for r in ew["rows"]]), ""]
    text = sanitize("\n".join(out))
    (RESULTS / "results.md").write_text(text + "\n")
    print(text)


if __name__ == "__main__":
    sys.exit(main())
