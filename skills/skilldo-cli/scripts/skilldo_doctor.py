#!/usr/bin/env python3
"""skilldo_doctor.py — 用文件系统的真实形态体检 SkillDo 的同步目标。

背景：`skilldo list` 的 status 读的是 SQLite 里的记录，不是磁盘。历史上
`skilldo update` 会把本该是软链的目标物化成实目录副本，DB 却仍记着
`mode=symlink`，于是 573 个目标全部报 `ok`，实际 63% 是实体副本。
`skilldo device status` 同样不查这一层漂移。

本脚本反过来做：先问 DB「目标应该在哪些路径」，再下到磁盘逐个判形态。

只读。不会修改任何文件。退出码：0 = 干净，1 = 有漂移，2 = 环境问题。

用法：
    python3 skilldo_doctor.py                # 人类可读报告
    python3 skilldo_doctor.py --json         # 结构化输出
    python3 skilldo_doctor.py --orphans      # 额外列出游离副本（含 A/B/C 分类）
    python3 skilldo_doctor.py --unregistered # 额外列出「软链指向中心但 DB 无登记」
    python3 skilldo_doctor.py --root ~/.codex/skills   # 只查指定工具根

三类盲区互补，缺一类就漏：
    scan()              走 DB 的目标列表   → 看不见「磁盘有、DB 无」的路径
    scan_orphans()      只查实目录         → 软链一律跳过
    scan_unregistered() 只查软链           → 补上缺口
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from collections import Counter, defaultdict

HOME = os.path.expanduser("~")

# 形态分类
LINK_OK = "link->central"      # 软链，指向中心目录且中心实体存在 —— 唯一合规形态
LINK_DANGLING = "link-dangling"  # 软链指向中心，但中心那侧没有实体
LINK_ELSEWHERE = "link->elsewhere"  # 软链，但指向别处
REAL_DIR = "real-dir"          # 实目录副本（要收敛的对象）
NOT_DIR = "not-a-dir"          # 路径存在但不是目录
MISSING = "missing"            # 目标不存在

DRIFT = {LINK_DANGLING, LINK_ELSEWHERE, REAL_DIR, NOT_DIR, MISSING}

# 游离副本的分类
ORPHAN_A = "A"   # 中心有实体 · 与副本逐字节一致
ORPHAN_B = "B"   # 中心有实体 · 内容有差异
ORPHAN_C = "C"   # 中心没有实体

# 比对时忽略的条目
SKIP_NAMES = {".git", ".skilldo-cache.json", ".workbuddy", ".DS_Store",
              "__pycache__", ".pytest_cache"}


def find_skilldo(explicit: str | None) -> str:
    for cand in (explicit, os.environ.get("SKILLDO_BIN"),
                 os.path.join(HOME, ".local", "bin", "skilldo")):
        if cand and os.path.isfile(cand) and os.access(cand, os.X_OK):
            return cand
    found = shutil.which("skilldo")
    if found:
        return found
    print("找不到 skilldo 可执行文件。用 --skilldo /路径 或设置 SKILLDO_BIN。",
          file=sys.stderr)
    sys.exit(2)


def load_skills(binary: str, db: str | None) -> list[dict]:
    cmd = [binary, "list", "--json"]
    if db:
        cmd += ["--db", db]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        print(f"`skilldo list --json` 失败：{proc.stderr.strip()}", file=sys.stderr)
        sys.exit(2)
    data = json.loads(proc.stdout)
    if isinstance(data, list):
        return data
    return data.get("skills") or data.get("items") or []


def classify(path: str, central: str | None) -> str:
    if os.path.islink(path):
        if central and os.path.realpath(path) == os.path.realpath(central):
            # realpath 对悬空链直接返回目标路径，所以要另行确认中心那侧是否真的存在
            return LINK_OK if os.path.isdir(path) else LINK_DANGLING
        return LINK_ELSEWHERE
    if os.path.isdir(path):
        return REAL_DIR
    if os.path.exists(path):
        return NOT_DIR
    return MISSING


def scan(skills: list[dict]) -> dict:
    targets, declared, roots = [], {}, set()
    for it in skills:
        name = it.get("name")
        central = it.get("centralPath")
        if central:
            roots.add(os.path.dirname(central))
        for t in it.get("targets", []):
            p = t.get("targetPath")
            if not p:
                continue
            roots.add(os.path.dirname(p))
            declared[p] = name
            targets.append({
                "skill": name,
                "tool": t.get("tool"),
                "path": p,
                "central": central,
                "kind": classify(p, central),
                "dbMode": t.get("mode"),
                "dbStatus": t.get("status"),
            })
    return {"targets": targets, "declared": declared, "roots": roots}


def repo_root(path: str) -> str | None:
    """返回包含 path 的 git 工作区根；不在仓库内返回 None。

    受版本控制的工具根里的实目录**不能**换成软链——软链不入 git，
    换完内容就从仓库里消失了。这类根要在报告里单独点名。
    """
    proc = subprocess.run(["git", "-C", path, "rev-parse", "--show-toplevel"],
                          capture_output=True, text=True)
    if proc.returncode == 0 and proc.stdout.strip():
        return proc.stdout.strip()
    return None


def contents_match(a: str, b: str) -> bool:
    cmd = ["diff", "-rq"]
    for name in sorted(SKIP_NAMES):
        cmd.append(f"--exclude={name}")
    cmd += [a, b]
    return subprocess.run(cmd, capture_output=True).returncode == 0


def newest_mtime(root: str) -> float:
    newest = 0.0
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_NAMES]
        for f in filenames:
            if f in SKIP_NAMES:
                continue
            try:
                newest = max(newest, os.path.getmtime(os.path.join(dirpath, f)))
            except OSError:
                pass
    return newest


def scan_orphans(roots: set[str], declared: dict, central_names: set[str]) -> list[dict]:
    """游离副本：工具目录里有实体，但该路径没被登记成任何 skill 的 target。

    顺带分类，让「能不能直接收敛」一眼可见：
        A 中心有 · 完全一致  → 换软链零损失
        B 中心有 · 有差异    → 先判方向
        C 中心无实体         → 要纳管得先迁实体进中心
    """
    central_dir = os.path.join(HOME, ".skillshub")
    out = []
    for r in sorted(roots):
        if not os.path.isdir(r):
            continue
        # 中心目录自己不算游离
        if os.path.realpath(r) == os.path.realpath(central_dir):
            continue
        in_repo = repo_root(r)
        for e in sorted(os.listdir(r)):
            if e.startswith("."):
                continue
            p = os.path.join(r, e)
            if os.path.islink(p) or not os.path.isdir(p):
                continue
            if p in declared:
                continue
            central = os.path.join(central_dir, e)
            item = {"path": p, "name": e, "root": r,
                    "centralExists": e in central_names, "inRepo": in_repo}
            if e not in central_names:
                item["state"] = ORPHAN_C
                item["copyNewer"] = False
            elif contents_match(p, central):
                item["state"] = ORPHAN_A
                item["copyNewer"] = False
            else:
                item["state"] = ORPHAN_B
                # 留 2 秒容差，避免亚秒级 mtime 抖动被当成「副本更新」
                item["copyNewer"] = newest_mtime(p) > newest_mtime(central) + 2
            out.append(item)
    return out


def scan_unregistered(roots: set[str], declared: dict,
                      central_names: set[str]) -> list[dict]:
    """未登记软链：工具目录里是**指向中心的软链**，但该路径没被登记成任何 target。

    与前两类互补——`scan()` 走 DB 看不见它，`scan_orphans()` 只查实目录也会跳过它。

    危害不在于「坏了」，而在于「静默游离」：工具照样读得到、doctor 默认也不报，
    但 skilldo 不知道它存在 —— `update` 不会更新它、`delete` 不会清它、
    `list` 不显示它。迁实体进中心后手工 `ln -s`，必然落入这一类，
    除非补一次 `skilldo sync --skill <name> --tool <key>`。

    `pointsToCentral=False` 的要单独留意：那是指向别处的软链，通常是旧版
    skilldo 留下的**相对**链接（指向早已消失的目录）。
    """
    central_dir = os.path.join(HOME, ".skillshub")
    out = []
    for r in sorted(roots):
        if not os.path.isdir(r):
            continue
        if os.path.realpath(r) == os.path.realpath(central_dir):
            continue
        in_repo = repo_root(r)
        for e in sorted(os.listdir(r)):
            if e.startswith("."):
                continue
            p = os.path.join(r, e)
            if not os.path.islink(p):
                continue
            if p in declared:
                continue
            expected = os.path.realpath(os.path.join(central_dir, e))
            out.append({
                "path": p, "name": e, "root": r,
                "linkTarget": os.readlink(p),
                "pointsToCentral": os.path.realpath(p) == expected,
                "centralExists": e in central_names,
                "inRepo": in_repo,
            })
    return out


def central_skill_names() -> set[str]:
    central = os.path.join(HOME, ".skillshub")
    if not os.path.isdir(central):
        return set()
    return {n for n in os.listdir(central)
            if os.path.isdir(os.path.join(central, n))}


def human(report: dict, orphans: list[dict] | None,
          unregistered: list[dict] | None = None,
          only_root: str | None = None) -> None:
    ts = report["targets"]
    if only_root:
        rp = os.path.realpath(os.path.expanduser(only_root))
        ts = [t for t in ts
              if os.path.dirname(os.path.realpath(t["path"])) == rp]
    counts = Counter(t["kind"] for t in ts)
    total = len(ts)
    print(f"SkillDo 同步目标体检 · 共 {total} 个路径")
    print(f"  中心仓库: {os.path.join(HOME, '.skillshub')}")
    print()
    label = {
        LINK_OK: "软链 → 中心（合规）",
        LINK_DANGLING: "软链悬空（中心无实体）",
        LINK_ELSEWHERE: "软链 → 别处（异常）",
        REAL_DIR: "实目录副本（应换软链）",
        NOT_DIR: "路径存在但非目录",
        MISSING: "目标缺失",
    }
    for k in (LINK_OK, LINK_DANGLING, LINK_ELSEWHERE, REAL_DIR, NOT_DIR, MISSING):
        v = counts.get(k, 0)
        if v:
            print(f"  {label[k]:24s} {v:4d}")
    print()

    # DB 谎报检测：DB 说 symlink+ok，磁盘却不是软链
    lying = [t for t in ts
             if t["kind"] in DRIFT
             and t["dbMode"] == "symlink"
             and t["dbStatus"] == "ok"]
    drifted = [t for t in ts if t["kind"] in DRIFT]
    if drifted:
        print(f"有漂移的目标 {len(drifted)} 个"
              f"（其中 {len(lying)} 个被 DB 谎报为 symlink/ok）:")
        by_root = defaultdict(list)
        for t in drifted:
            by_root[os.path.dirname(t["path"])].append(t)
        for r in sorted(by_root):
            items = by_root[r]
            kinds = Counter(i["kind"] for i in items)
            desc = ", ".join(f"{k}×{v}" for k, v in kinds.most_common())
            print(f"  {r.replace(HOME + '/', ''):34s} {len(items):3d}  {desc}")
            for i in sorted(items, key=lambda x: x["skill"] or ""):
                extra = ""
                if i["kind"] == LINK_ELSEWHERE:
                    extra = " → " + os.readlink(i["path"])
                print(f"      {i['kind']:<16} {i['skill']:<28} {i['path'].replace(HOME + '/', '')}{extra}")
        print()
    else:
        print("✔ 所有目标形态合规。")
        print()

    if orphans is not None:
        if orphans:
            counts = Counter(o["state"] for o in orphans)
            print(f"游离副本 {len(orphans)} 个"
                  f"（工具目录里有实体，但未登记为该 skill 的 target）:")
            print(f"  A 中心有 · 完全一致   {counts.get(ORPHAN_A, 0):4d}  → 换软链零损失")
            print(f"  B 中心有 · 有差异     {counts.get(ORPHAN_B, 0):4d}  → 先判方向")
            print(f"  C 中心无实体         {counts.get(ORPHAN_C, 0):4d}  → 先迁实体进中心")
            print()
            blocked = [o for o in orphans if o.get("inRepo")]
            actable = [o for o in orphans if not o.get("inRepo")]
            for title, group in (("可收敛（不在任何 git 仓库内）", actable),
                                 ("跳过（工具根在 git 仓库内，软链会让内容离开仓库）", blocked)):
                if not group:
                    continue
                print(f"  ── {title} · {len(group)} 处 ──")
                grouped = defaultdict(list)
                for o in group:
                    grouped[o["name"]].append(o)
                for name in sorted(grouped, key=lambda x: (-len(grouped[x]), x)):
                    items = grouped[name]
                    state = items[0]["state"]
                    note = {"A": "一致", "B": "有差异", "C": "中心无实体"}[state]
                    if items[0].get("copyNewer"):
                        note = "副本更新"
                    tools = sorted(os.path.dirname(p["path"]).replace(HOME + "/", "")
                                   for p in items)
                    print(f"    {state} {name:48s} {len(tools)} 处 [{note}]: "
                          f"{', '.join(tools)}")
                print()
        else:
            print("✔ 没有游离副本。")
            print()

    if unregistered is not None:
        if unregistered:
            stray = [u for u in unregistered if u["pointsToCentral"]]
            away = [u for u in unregistered if not u["pointsToCentral"]]
            print(f"未登记软链 {len(unregistered)} 个"
                  f"（工具目录里是指向中心的软链，但 DB 没有这条 target）:")
            print(f"  指向中心、DB 未登记   {len(stray):4d}  → 补 skilldo sync 即纳入管理")
            print(f"  指向别处              {len(away):4d}  → 手动核实该不该重指中心")
            print()
            for title, group in (("指向中心 · 缺 DB 登记", stray),
                                 ("指向别处 · 需人工核实", away)):
                if not group:
                    continue
                print(f"  ── {title} · {len(group)} 处 ──")
                grouped = defaultdict(list)
                for u in group:
                    grouped[u["name"]].append(u)
                for name in sorted(grouped, key=lambda x: (-len(grouped[x]), x)):
                    tools = sorted(os.path.dirname(u["path"]).replace(HOME + "/", "")
                                   for u in grouped[name])
                    print(f"      {name:48s} {len(tools)} 处: {', '.join(tools)}")
                print()
        else:
            print("✔ 没有未登记软链。")
            print()


def main() -> int:
    ap = argparse.ArgumentParser(description="用文件系统真实形态体检 SkillDo 同步目标")
    ap.add_argument("--json", action="store_true", help="输出结构化 JSON")
    ap.add_argument("--orphans", action="store_true", help="额外扫描游离副本")
    ap.add_argument("--unregistered", action="store_true",
                    help="额外扫描「软链指向中心但 DB 无 target 登记」")
    ap.add_argument("--root", help="只体检指定工具根目录")
    ap.add_argument("--skilldo", help="skilldo 可执行文件路径")
    ap.add_argument("--db", help="覆盖 SQLite 路径")
    args = ap.parse_args()

    binary = find_skilldo(args.skilldo)
    skills = load_skills(binary, args.db)
    rep = scan(skills)

    orphans = None
    if args.orphans:
        orphans = scan_orphans(rep["roots"], rep["declared"], central_skill_names())

    unregistered = None
    if args.unregistered:
        unregistered = scan_unregistered(rep["roots"], rep["declared"],
                                         central_skill_names())

    drifted = [t for t in rep["targets"] if t["kind"] in DRIFT]
    code = 1 if drifted else 0

    if args.json:
        payload = {
            "ok": not drifted,
            "centralPath": os.path.join(HOME, ".skillshub"),
            "counts": dict(Counter(t["kind"] for t in rep["targets"])),
            "total": len(rep["targets"]),
            "drifted": drifted,
            "dbLying": [t["path"] for t in drifted
                        if t["dbMode"] == "symlink" and t["dbStatus"] == "ok"],
        }
        if orphans is not None:
            payload["orphans"] = orphans
        if unregistered is not None:
            payload["unregistered"] = unregistered
        print(json.dumps(payload, ensure_ascii=False, indent=2))
    else:
        human(rep, orphans, unregistered, args.root)

    return code


if __name__ == "__main__":
    sys.exit(main())
