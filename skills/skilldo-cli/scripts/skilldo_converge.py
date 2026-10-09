#!/usr/bin/env python3
"""skilldo_converge.py — 把游离在工具目录里的 Skill 实体收敛成「中心一份 + 软链」。

背景：SkillDo 的目标形态是「一份物理副本在 ~/.skillshub，各工具目录只放指向它的软链」。
历史上 `skilldo update` 会把软链物化成实目录副本，于是同一 skill 在多处各留一份实体；
此外还有一批「工具目录里有实体、但从未登记成 target」的游离副本（orphan），
`skilldo list` 完全看不到它们。

本脚本只干一件事：把这些游离实体收敛成软链。分三类处理：

  A 类  中心有实体，且与副本逐字节一致      → 直接换软链（零内容损失）
  B 类  中心有实体，但内容有差异            → 默认「中心为准」换软链；若副本更新则跳过待判
  C 类  中心没有实体                        → 先把实体迁进中心，再换软链

硬规则（脚本强制，不可绕过）：
  · **git 仓库内的工具根一律跳过**。软链不入 git，把仓库里的实目录换成软链会让内容
    从仓库消失。默认跳过并在报告里点名；确需处理要显式 --include-repos。
  · 只隔离、不删除。原目录 mv 进备份区，`tar` 另存一份。
  · 默认 dry-run。真改文件必须显式 --apply。

只读依赖 `skilldo list --json`。退出码：0 干净 / 1 有待处理项 / 2 环境问题。

用法：
    python3 skilldo_converge.py                      # dry-run，列出计划
    python3 skilldo_converge.py --class A,B          # 只看 A/B 类
    python3 skilldo_converge.py --apply              # 真正执行
    python3 skilldo_converge.py --apply --backup-dir ~/.skilldo-backups/xxx
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
import tarfile
import time
import uuid
from collections import Counter, defaultdict

HOME = os.path.expanduser("~")
CENTRAL = os.path.join(HOME, ".skillshub")

# 比对时要忽略的条目：不属于 skill 内容本身
SKIP_NAMES = {".git", ".skilldo-cache.json", ".workbuddy", ".DS_Store",
              "__pycache__", ".pytest_cache"}

STATE_A = "A"   # 中心有 · 完全一致
STATE_B = "B"   # 中心有 · 有差异
STATE_C = "C"   # 中心无实体


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


def repo_root(path: str) -> str | None:
    """返回包含 path 的 git 工作区根；不在仓库内返回 None。"""
    proc = subprocess.run(["git", "-C", path, "rev-parse", "--show-toplevel"],
                          capture_output=True, text=True)
    if proc.returncode == 0 and proc.stdout.strip():
        return proc.stdout.strip()
    return None


def newer_mtime(root: str) -> float:
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


def source_snapshot(root: str) -> str:
    """Hash a tree without following links so apply can detect scan-time drift."""
    records = []

    def visit(path: str, relative: str) -> None:
        info = os.lstat(path)
        identity = (stat.S_IMODE(info.st_mode), info.st_dev, info.st_ino, info.st_mtime_ns)
        if stat.S_ISLNK(info.st_mode):
            records.append((relative, "symlink", *identity, os.readlink(path)))
        elif stat.S_ISDIR(info.st_mode):
            records.append((relative, "directory", *identity))
            for entry in sorted(os.scandir(path), key=lambda item: item.name):
                child_relative = os.path.join(relative, entry.name) if relative else entry.name
                visit(entry.path, child_relative)
        elif stat.S_ISREG(info.st_mode):
            digest = hashlib.sha256()
            with open(path, "rb") as source:
                for chunk in iter(lambda: source.read(1024 * 1024), b""):
                    digest.update(chunk)
            records.append((relative, "file", *identity, digest.hexdigest()))
        else:
            raise ValueError(f"unsupported special file in Skill source: {path}")

    visit(root, "")
    encoded = json.dumps(records, ensure_ascii=False, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def contents_match(a: str, b: str) -> bool:
    cmd = ["diff", "-rq"]
    for name in sorted(SKIP_NAMES):
        cmd.append(f"--exclude={name}")
    cmd += [a, b]
    return subprocess.run(cmd, capture_output=True).returncode == 0


def scan(binary: str, db: str | None) -> dict:
    skills = load_skills(binary, db)
    declared, roots = set(), set()
    for it in skills:
        central = it.get("centralPath")
        if central:
            roots.add(os.path.dirname(central))
        for t in it.get("targets", []):
            p = t.get("targetPath")
            if not p:
                continue
            declared.add(p)
            roots.add(os.path.dirname(p))

    central_names = {n for n in os.listdir(CENTRAL) if os.path.isdir(os.path.join(CENTRAL, n))}
    central_real = os.path.realpath(CENTRAL)

    items = []
    for root in sorted(roots):
        if not os.path.isdir(root) or os.path.realpath(root) == central_real:
            continue
        rr = repo_root(root)
        for entry in sorted(os.listdir(root)):
            if entry.startswith("."):
                continue
            path = os.path.join(root, entry)
            if os.path.islink(path) or not os.path.isdir(path):
                continue
            if path in declared:
                continue
            central = os.path.join(CENTRAL, entry)
            if entry not in central_names:
                state, newer = STATE_C, False
            elif contents_match(path, central):
                state, newer = STATE_A, False
            else:
                state = STATE_B
                # 亚秒级 mtime 抖动（rsync/复制常见）不该判成「副本更新」，
                # 否则会把内容其实一致的副本永远卡在待判状态。留 2 秒容差。
                newer = newer_mtime(path) > newer_mtime(central) + 2
            items.append({
                "name": entry, "path": path, "root": root, "central": central,
                "state": state, "central_exists": entry in central_names,
                "copy_newer": newer, "in_git_repo": rr,
                "source_snapshot": source_snapshot(path),
            })
    return {"items": items, "roots": sorted(roots), "declared": sorted(declared)}


def plan(items: list[dict], classes: set[str], include_repos: bool) -> list[dict]:
    out = []
    for it in items:
        if it["state"] not in classes:
            continue
        if it["in_git_repo"] and not include_repos:
            it = {**it, "action": "skip", "reason": f"在 git 仓库内（{it['in_git_repo']}）"}
        elif it["state"] == STATE_B and it["copy_newer"]:
            it = {**it, "action": "skip",
                  "reason": "副本比中心新，需先人工判定方向（回灌中心后再换软链）"}
        elif it["state"] == STATE_C:
            it = {**it, "action": "migrate", "reason": "迁实体进中心后换软链"}
        else:
            it = {**it, "action": "symlink", "reason": "备份后换成指向中心的软链"}
        out.append(it)
    return out


def quarantine(path: str, backup_dir: str) -> str:
    rel = os.path.relpath(path, HOME)
    relative_parent, leaf = os.path.split(rel)
    parent = os.path.join(backup_dir, "quarantine", relative_parent)
    os.makedirs(parent, exist_ok=True)

    # Reserve a fresh container atomically. Never remove or reuse an existing
    # quarantine entry: it may be the only recovery copy from an earlier run.
    for _ in range(100):
        container = os.path.join(parent, f"{leaf}-{uuid.uuid4().hex}")
        try:
            os.mkdir(container, mode=0o700)
        except FileExistsError:
            continue
        dest = os.path.join(container, leaf)
        try:
            shutil.move(path, dest)
            return dest
        except Exception:
            # Only remove our empty reservation. If a partial move left data,
            # retain it for recovery and fail closed.
            try:
                os.rmdir(container)
            except OSError:
                pass
            raise
    raise FileExistsError("could not reserve a unique quarantine destination")


def move_c_source_to_new_central(path: str, central: str) -> None:
    """Reserve a new central directory before moving any C-class source data."""
    if os.path.lexists(central):
        raise FileExistsError(f"central destination already exists: {central}")
    os.makedirs(os.path.dirname(central), exist_ok=True)
    # mkdir is an atomic no-replace reservation. shutil.move(path, central)
    # could instead nest path inside a destination that appeared concurrently.
    os.mkdir(central, mode=0o700)
    source_entries = sorted(os.listdir(path))
    try:
        for name in source_entries:
            source_entry = os.path.join(path, name)
            central_entry = os.path.join(central, name)
            if os.path.lexists(central_entry):
                raise FileExistsError(f"central entry appeared during migration: {central_entry}")
            shutil.move(source_entry, central_entry)
        os.rmdir(path)
    except Exception as exc:
        rollback_errors = []
        if not os.path.lexists(path):
            try:
                os.mkdir(path)
            except Exception as rollback_exc:  # noqa: BLE001
                rollback_errors.append(rollback_exc)
        if os.path.isdir(path):
            for name in source_entries:
                source_entry = os.path.join(path, name)
                central_entry = os.path.join(central, name)
                if os.path.lexists(central_entry) and not os.path.lexists(source_entry):
                    try:
                        shutil.move(central_entry, source_entry)
                    except Exception as rollback_exc:  # noqa: BLE001
                        rollback_errors.append(rollback_exc)
                elif os.path.lexists(central_entry) and os.path.lexists(source_entry):
                    rollback_errors.append(
                        FileExistsError(f"both source and central entries exist: {source_entry}")
                    )
        if not rollback_errors:
            try:
                os.rmdir(central)
            except OSError as rollback_exc:
                # An unexpected entry appeared after our reservation. Preserve
                # it and report the central path rather than deleting anything.
                if os.listdir(central):
                    rollback_errors.append(rollback_exc)
        if rollback_errors:
            details = "; ".join(str(error) for error in rollback_errors)
            raise RuntimeError(
                f"migration failed ({exc}); rollback incomplete; original data may remain at "
                f"{central}: {details}"
            ) from exc
        raise


class ApplyLockError(RuntimeError):
    pass


@contextlib.contextmanager
def apply_lock(enabled: bool):
    """Serialize apply runs across processes; dry-runs intentionally stay unlocked."""
    if not enabled:
        yield
        return
    try:
        import fcntl
    except ImportError as exc:  # pragma: no cover - supported runtime is macOS/Linux
        raise ApplyLockError("cross-process apply locking is unavailable on this platform") from exc

    lock_path = os.path.join(HOME, ".skilldo-converge.lock")
    fd = os.open(
        lock_path,
        os.O_CREAT | os.O_RDWR | getattr(os, "O_NOFOLLOW", 0),
        0o600,
    )
    locked = False
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            locked = True
        except BlockingIOError as exc:
            raise ApplyLockError("another converge --apply is already running") from exc
        yield
    finally:
        if locked:
            fcntl.flock(fd, fcntl.LOCK_UN)
        os.close(fd)


def apply_one(it: dict, backup_dir: str) -> tuple[bool, str]:
    path, central = it["path"], it["central"]
    expected_snapshot = it.get("source_snapshot")
    if not expected_snapshot:
        return False, "缺少扫描时的源目录快照，拒绝执行"
    try:
        if source_snapshot(path) != expected_snapshot:
            return False, "源目录在扫描后发生变化，未处理"
    except Exception as exc:  # noqa: BLE001
        return False, f"无法重新校验源目录，未处理: {exc}"
    migrated = False
    quarantined = None
    try:
        if it["action"] == "symlink":
            quarantined = quarantine(path, backup_dir)
            os.symlink(central, path)
            if os.path.realpath(path) != os.path.realpath(central):
                raise RuntimeError("软链校验失败")
            return True, "已换软链"
        if it["action"] == "migrate":
            move_c_source_to_new_central(path, central)
            migrated = True
            os.symlink(central, path)
            if os.path.realpath(path) != os.path.realpath(central):
                raise RuntimeError("软链校验失败")
            return True, "已迁入中心并换软链"
        return False, "动作未知，跳过"
    except Exception as exc:  # noqa: BLE001
        if migrated:
            # C-class migration has no quarantine copy, so restore the moved
            # central entity itself if link creation or verification fails.
            rollback_error = None
            try:
                if os.path.islink(path) and os.readlink(path) == central:
                    os.unlink(path)
                elif os.path.lexists(path):
                    raise FileExistsError(f"refusing to replace unexpected path: {path}")
                if not os.path.lexists(path):
                    shutil.move(central, path)
            except Exception as rollback_exc:  # noqa: BLE001
                rollback_error = rollback_exc
            if rollback_error:
                return False, f"失败：{exc}；恢复原目录失败，中心副本保留在 {central}: {rollback_error}"
            return False, f"失败：{exc}；已从中心恢复原目录"

        # For quarantined A/B copies, restore only when the original path is
        # still vacant; never replace an unexpected file or directory.
        if quarantined:
            try:
                if not os.path.lexists(path) and os.path.isdir(quarantined):
                    os.makedirs(os.path.dirname(path), exist_ok=True)
                    shutil.move(quarantined, path)
                    return False, f"失败：{exc}；已恢复原副本"
            except Exception as restore_exc:  # noqa: BLE001
                return False, (
                    f"失败：{exc}；原副本安全保存在隔离区 {quarantined}，"
                    f"自动恢复失败: {restore_exc}"
                )
            if os.path.isdir(quarantined):
                return False, f"失败：{exc}；原副本安全保存在隔离区 {quarantined}"
            return False, f"失败：{exc}；源位置与隔离副本均需人工核对: {quarantined}"
        return False, f"失败：{exc}"


def make_tar(items: list[dict], backup_dir: str) -> str | None:
    todo = [i for i in items if i["action"] in ("symlink", "migrate")]
    if not todo:
        return None
    os.makedirs(backup_dir, exist_ok=True)
    stamp = time.strftime("%Y%m%d-%H%M%S")
    for _ in range(100):
        tar_path = os.path.join(
            backup_dir, f"converge-copies-{stamp}-{uuid.uuid4().hex}.tar.gz"
        )
        try:
            # O_EXCL reserves the path atomically, so an older archive is
            # never truncated even if names collide or two runs overlap.
            fd = os.open(tar_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        except FileExistsError:
            continue
        try:
            with os.fdopen(fd, "wb") as archive_file:
                with tarfile.open(
                    fileobj=archive_file, mode="w:gz", dereference=False
                ) as tf:
                    for it in todo:
                        tf.add(it["path"], arcname=os.path.relpath(it["path"], HOME))
            return tar_path
        except Exception:
            # This path was exclusively created by this invocation. Remove
            # only its incomplete archive; never touch a pre-existing file.
            try:
                os.unlink(tar_path)
            except OSError:
                pass
            raise
    raise FileExistsError("could not reserve a unique backup archive")


def human(report: dict, planned: list[dict], applied: bool, tar_path: str | None) -> None:
    counts = Counter(it["state"] for it in report["items"])
    print(f"游离副本体检 · 共 {len(report['items'])} 处")
    print(f"  A 中心有 · 完全一致   {counts.get(STATE_A, 0):4d}")
    print(f"  B 中心有 · 有差异     {counts.get(STATE_B, 0):4d}")
    print(f"  C 中心无实体         {counts.get(STATE_C, 0):4d}")
    print()

    by_action = defaultdict(list)
    for it in planned:
        by_action[it["action"]].append(it)

    for action, label in (("symlink", "换软链"), ("migrate", "迁入中心"),
                          ("skip", "跳过")):
        group = by_action.get(action)
        if not group:
            continue
        print(f"【{label}】{len(group)} 处")
        for it in sorted(group, key=lambda x: (x["state"], x["name"])):
            mark = f"  {it['state']} "
            extra = f"  ← {it['reason']}" if action == "skip" else ""
            print(f"{mark}{it['name']:46s} {it['path'].replace(HOME + '/', '')}{extra}")
        print()
    if tar_path:
        print(f"备份：{tar_path}")
        print(f"隔离区：{os.path.join(os.path.dirname(tar_path), 'quarantine')}")
        print()


def validate_plan_sources(planned: list[dict]) -> None:
    """Mark sources changed since scan as skipped before creating any archive."""
    for item in planned:
        if item["action"] not in ("symlink", "migrate"):
            continue
        try:
            unchanged = source_snapshot(item["path"]) == item.get("source_snapshot")
        except Exception as exc:  # noqa: BLE001
            item["action"] = "skip"
            item["reason"] = f"无法校验扫描后的源目录，拒绝执行: {exc}"
            continue
        if not unchanged:
            item["action"] = "skip"
            item["reason"] = "源目录在扫描后发生变化，拒绝执行"


def execute(args: argparse.Namespace, binary: str) -> int:
    report = scan(binary, args.db)
    planned = plan(report["items"], {c.strip().upper() for c in args.classes.split(",") if c.strip()}, args.include_repos)

    backup_dir = args.backup_dir or os.path.join(
        HOME, ".skilldo-backups", time.strftime("%Y%m%d") + "-converge")

    tar_path = None
    results = []
    if args.apply:
        validate_plan_sources(planned)
        tar_path = make_tar(planned, backup_dir)
        for it in planned:
            if it["action"] == "skip":
                results.append({**it, "ok": None, "msg": "跳过（未处理）"})
                continue
            ok, msg = apply_one(it, backup_dir)
            results.append({**it, "ok": ok, "msg": msg})

    if args.json:
        print(json.dumps({
            "dryRun": not args.apply,
            "counts": dict(Counter(i["state"] for i in report["items"])),
            "planned": planned,
            "results": results,
            "backupTar": tar_path,
            "backupDir": backup_dir if args.apply else None,
        }, ensure_ascii=False, indent=2))
    else:
        if args.apply:
            ok = sum(1 for r in results if r.get("ok"))
            fail = [r for r in results if r.get("ok") is False]
            print(f"{'=' * 60}\n执行结果：成功 {ok} / 失败 {len(fail)}")
            for r in fail:
                print(f"  ! {r['name']} @ {r['path'].replace(HOME + '/', '')} — {r['msg']}")
            print(f"{'=' * 60}\n")
        human(report, planned, args.apply, tar_path)

    todo = [i for i in planned if i["action"] != "skip"]
    return 1 if todo else 0


def main() -> int:
    ap = argparse.ArgumentParser(description="把游离 Skill 实体收敛成「中心一份 + 软链」")
    ap.add_argument("--apply", action="store_true", help="真正执行（默认 dry-run）")
    ap.add_argument("--class", dest="classes", default="A,B,C",
                    help="要处理的类别，逗号分隔，默认 A,B,C")
    ap.add_argument("--include-repos", action="store_true",
                    help="连 git 仓库内的工具根一起处理（危险，默认跳过）")
    ap.add_argument("--backup-dir", help="备份目录，默认 ~/.skilldo-backups/<日期>-converge")
    ap.add_argument("--json", action="store_true", help="输出结构化 JSON")
    ap.add_argument("--skilldo", help="skilldo 可执行文件路径")
    ap.add_argument("--db", help="覆盖 SQLite 路径")
    args = ap.parse_args()

    classes = {c.strip().upper() for c in args.classes.split(",") if c.strip()}
    bad = classes - {STATE_A, STATE_B, STATE_C}
    if bad:
        print(f"未知类别：{','.join(sorted(bad))}", file=sys.stderr)
        return 2

    binary = find_skilldo(args.skilldo)
    try:
        with apply_lock(args.apply):
            return execute(args, binary)
    except ApplyLockError as exc:
        print(str(exc), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
