import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).with_name("skilldo_converge.py")
SPEC = importlib.util.spec_from_file_location("skilldo_converge", SCRIPT)
converge = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(converge)


class ConvergeRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def test_make_tar_never_overwrites_an_existing_archive(self):
        source = self.root / "loose-skill"
        source.mkdir()
        (source / "SKILL.md").write_text("recovery content")
        backup_dir = self.root / "backups"
        backup_dir.mkdir()
        stamp = "20261009-120000"
        collision_id = "a" * 32
        unique_id = "b" * 32
        old_archive = backup_dir / f"converge-copies-{stamp}-{collision_id}.tar.gz"
        old_archive.write_bytes(b"previous recovery archive")
        item = {"path": str(source), "action": "symlink"}

        with mock.patch.object(converge, "HOME", str(self.root)), mock.patch.object(
            converge.time, "strftime", return_value=stamp
        ), mock.patch.object(
            converge.uuid, "uuid4", side_effect=[mock.Mock(hex=collision_id), mock.Mock(hex=unique_id)]
        ):
            created = converge.make_tar([item], str(backup_dir))

        self.assertEqual(old_archive.read_bytes(), b"previous recovery archive")
        self.assertNotEqual(created, str(old_archive))
        self.assertTrue(Path(created).is_file())
        self.assertGreater(Path(created).stat().st_size, 0)

    def test_make_tar_preserves_symlink_without_archiving_external_target(self):
        source = self.root / "loose-skill"
        source.mkdir()
        external = self.root / "outside-secret.txt"
        external.write_text("must not be copied into the archive")
        link = source / "external-link"
        link.symlink_to(external)
        item = {"path": str(source), "action": "symlink"}

        with mock.patch.object(converge, "HOME", str(self.root)):
            archive = converge.make_tar([item], str(self.root / "backups"))

        with converge.tarfile.open(archive, "r:gz") as tf:
            member = tf.getmember("loose-skill/external-link")
            self.assertTrue(member.issym())
            self.assertEqual(member.linkname, str(external))
            self.assertNotIn("outside-secret.txt", tf.getnames())

    def test_quarantine_uses_a_new_destination_and_preserves_collision(self):
        with mock.patch.object(converge, "HOME", str(self.root)):
            source = self.root / "tools" / "demo"
            source.mkdir(parents=True)
            (source / "user.txt").write_text("current copy")
            snapshot = converge.source_snapshot(str(source))
            backup_dir = self.root / "backups"
            collision = backup_dir / "quarantine" / "tools" / "demo-fixed"
            collision.mkdir(parents=True)
            old_copy = collision / "demo"
            old_copy.mkdir()
            (old_copy / "user.txt").write_text("older recovery copy")

            with mock.patch.object(
                converge.uuid,
                "uuid4",
                side_effect=[mock.Mock(hex="fixed"), mock.Mock(hex="fresh")],
            ):
                moved_to = Path(converge.quarantine(str(source), str(backup_dir)))

        self.assertEqual((old_copy / "user.txt").read_text(), "older recovery copy")
        self.assertEqual((moved_to / "user.txt").read_text(), "current copy")
        self.assertFalse(source.exists())

    def test_symlink_failure_and_restore_failure_reports_quarantine_path(self):
        with mock.patch.object(converge, "HOME", str(self.root)):
            source = self.root / "tools" / "demo"
            source.mkdir(parents=True)
            (source / "user.txt").write_text("recoverable copy")
            central = self.root / "central" / "demo"
            central.mkdir(parents=True)
            (central / "SKILL.md").write_text("central copy")
            item = {
                "name": "demo", "path": str(source), "central": str(central),
                "action": "symlink", "source_snapshot": converge.source_snapshot(str(source)),
            }
            real_move = converge.shutil.move
            calls = 0

            def fail_restore(src, dst):
                nonlocal calls
                calls += 1
                if calls == 2:
                    raise OSError("simulated quarantine restore failure")
                return real_move(src, dst)

            with mock.patch.object(converge.os, "symlink", side_effect=OSError("link failed")), mock.patch.object(
                converge.shutil, "move", side_effect=fail_restore
            ):
                ok, message = converge.apply_one(item, str(self.root / "backups"))

        self.assertFalse(ok)
        self.assertIn("原副本安全保存在隔离区", message)
        quarantined = Path(message.split("隔离区 ", 1)[1].split("，自动", 1)[0])
        self.assertEqual((quarantined / "user.txt").read_text(), "recoverable copy")
        self.assertFalse(source.exists())

    def _migrate_item(self):
        source = self.root / "tools" / "demo"
        source.mkdir(parents=True)
        (source / "SKILL.md").write_text("original C-class content")
        central = self.root / "central" / "demo"
        item = {
            "name": "demo",
            "path": str(source),
            "central": str(central),
            "action": "migrate",
            "source_snapshot": converge.source_snapshot(str(source)),
        }
        return source, central, item

    def _assert_migration_restored(self, source, central):
        self.assertTrue((source / "SKILL.md").is_file())
        self.assertEqual((source / "SKILL.md").read_text(), "original C-class content")
        self.assertFalse(source.is_symlink())
        self.assertFalse(central.exists())

    def test_c_class_refuses_competing_center_destination_without_moving_source(self):
        source, central, item = self._migrate_item()
        real_mkdir = converge.os.mkdir

        def create_competing_center(path, mode=0o777):
            if os.fspath(path) == str(central):
                real_mkdir(path, mode)
                (central / "owner.txt").write_text("existing center owner")
                raise FileExistsError("simulated competing destination")
            return real_mkdir(path, mode)

        with mock.patch.object(converge.os, "mkdir", side_effect=create_competing_center):
            ok, message = converge.apply_one(item, str(self.root / "backups"))

        self.assertFalse(ok)
        self.assertTrue((source / "SKILL.md").is_file())
        self.assertEqual((central / "owner.txt").read_text(), "existing center owner")
        self.assertFalse((central / "SKILL.md").exists())

    def test_c_class_move_failure_rolls_source_back_and_removes_own_reservation(self):
        source, central, item = self._migrate_item()
        real_move = converge.shutil.move
        failed_once = False

        def move_then_fail(src, dst):
            nonlocal failed_once
            result = real_move(src, dst)
            if not failed_once and os.fspath(src) == str(source / "SKILL.md"):
                failed_once = True
                raise OSError("simulated post-move failure")
            return result

        with mock.patch.object(converge.shutil, "move", side_effect=move_then_fail):
            ok, _message = converge.apply_one(item, str(self.root / "backups"))

        self.assertFalse(ok)
        self._assert_migration_restored(source, central)

    def test_apply_rejects_source_changed_after_scan(self):
        source, central, item = self._migrate_item()
        (source / "SKILL.md").write_text("changed after scan")

        ok, message = converge.apply_one(item, str(self.root / "backups"))

        self.assertFalse(ok)
        self.assertIn("扫描后发生变化", message)
        self.assertEqual((source / "SKILL.md").read_text(), "changed after scan")
        self.assertFalse(central.exists())

    def test_preflight_marks_changed_sources_skipped_before_archive(self):
        source, _central, item = self._migrate_item()
        (source / "SKILL.md").write_text("changed after scan")
        planned = [item]

        converge.validate_plan_sources(planned)

        self.assertEqual(planned[0]["action"], "skip")
        self.assertIn("扫描后发生变化", planned[0]["reason"])

    def test_apply_lock_is_cross_process_and_dry_run_does_not_lock(self):
        env = os.environ.copy()
        env["HOME"] = str(self.root)
        code = (
            "import importlib.util,sys; "
            "s=importlib.util.spec_from_file_location('converge',sys.argv[1]); "
            "m=importlib.util.module_from_spec(s); s.loader.exec_module(m); "
            "\ntry:\n with m.apply_lock(True): pass\n"
            "except m.ApplyLockError as e:\n print(e); sys.exit(17)\n"
        )
        with mock.patch.object(converge, "HOME", str(self.root)):
            with converge.apply_lock(True):
                with converge.apply_lock(False):
                    pass
                result = subprocess.run(
                    [sys.executable, "-c", code, str(SCRIPT)],
                    capture_output=True, text=True, env=env, check=False,
                )

        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertIn("another converge --apply", result.stdout)

    def test_c_class_migration_restores_original_when_symlink_creation_fails(self):
        source, central, item = self._migrate_item()
        with mock.patch.object(converge.os, "symlink", side_effect=OSError("simulated link failure")):
            ok, message = converge.apply_one(item, str(self.root / "backups"))

        self.assertFalse(ok)
        self.assertIn("已从中心恢复原目录", message)
        self._assert_migration_restored(source, central)

    def test_c_class_migration_restores_original_when_link_verification_fails(self):
        source, central, item = self._migrate_item()
        realpath = os.path.realpath

        def wrong_link_path(value):
            if os.fspath(value) == str(source):
                return "/simulated/wrong/link"
            return realpath(value)

        with mock.patch.object(converge.os.path, "realpath", side_effect=wrong_link_path):
            ok, message = converge.apply_one(item, str(self.root / "backups"))

        self.assertFalse(ok)
        self.assertIn("已从中心恢复原目录", message)
        self._assert_migration_restored(source, central)


if __name__ == "__main__":
    unittest.main()
