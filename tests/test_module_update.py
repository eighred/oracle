"""Exercise module proposals with real Git trees and an in-process hosting fixture."""

import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools import module_update


class ModuleUpdateTests(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        self.root = Path(work.name)
        self.parent = self.root / "parent"
        self.source = self.root / "source"
        for repo in (self.source, self.parent):
            repo.mkdir()
            self.git(repo, "init", "-b", "main")
            self.git(repo, "config", "user.email", "dev@example.invalid")
            self.git(repo, "config", "user.name", "Fixture")
            self.git(repo, "config", "commit.gpgsign", "false")
        self.old = self.source_revision("initial")
        url = "https://github.com/eighred/fixture_pricing.git"
        self.local = ["-c", "protocol.file.allow=always", "-c",
                      f"url.{self.source.as_uri()}.insteadOf={url}"]
        self.git(self.parent, *self.local, "submodule", "add", "--name", "pricing", url, "modules/pricing")
        self.git(self.parent, "commit", "-m", "Pin pricing")
        self.base = self.git(self.parent, "rev-parse", "HEAD")
        self.new = self.source_revision("updated")
        self.proposals = []
        self.proposal_sha = None
        self.forward = True
        self.fail_once = False

    @staticmethod
    def git(repo, *args, env=None):
        return subprocess.check_output(["git", *args], cwd=repo, text=True, env=env,
                                       stderr=subprocess.PIPE).strip()

    def source_revision(self, value):
        (self.source / "contract").write_text(value, encoding="utf-8")
        self.git(self.source, "add", "contract")
        self.git(self.source, "commit", "-m", value)
        return self.git(self.source, "rev-parse", "HEAD")

    def api(self, route, token, body=None):
        if route.startswith("repos/eighred/fixture_pricing"):
            self.assertEqual(token, "MODULE_READ_TOKEN")
            self.assertIsNone(body)
            if route.endswith("/branches/main"):
                return {"protected": True, "commit": {"sha": self.new}}
            if "/compare/" in route:
                return {"status": "ahead" if self.forward else "diverged"}
            return {"full_name": "eighred/fixture_pricing", "private": True, "default_branch": "main"}
        self.assertEqual(token, "ORACLE_PR_TOKEN")
        self.assertTrue(route.startswith("repos/eighred/oracle/"))
        if route.endswith("/branches/main"):
            return {"commit": {"sha": self.base}}
        if "/pulls?" in route:
            return self.proposals
        if "/matching-refs/" in route:
            return [] if not self.proposal_sha else [{"ref": self.ref, "object": {"sha": self.proposal_sha}}]
        if "/compare/" in route:
            return {"status": "identical"}
        if "/git/commits/" in route:
            sha = route.split("/")[-1]
            parents = self.git(self.parent, "show", "-s", "--format=%P", sha).split()
            return {"tree": {"sha": self.git(self.parent, "rev-parse", sha+"^{tree}")},
                    "parents": [{"sha": p} for p in parents]}
        if route.endswith("/git/trees"):
            entry, = body["tree"]
            env = {**os.environ, "GIT_INDEX_FILE": str(self.root / "temporary-index")}
            self.git(self.parent, "read-tree", body["base_tree"], env=env)
            self.git(self.parent, "update-index", "--cacheinfo",
                     entry["mode"], entry["sha"], entry["path"], env=env)
            return {"sha": self.git(self.parent, "write-tree", env=env)}
        if route.endswith("/git/commits"):
            return {"sha": self.git(self.parent, "commit-tree", body["tree"], "-p",
                                    body["parents"][0], "-m", body["message"])}
        if route.endswith("/git/refs"):
            self.proposal_sha, self.ref = body["sha"], body["ref"]
            self.git(self.parent, "update-ref", self.ref, self.proposal_sha)
            return {}
        if route.endswith("/pulls"):
            if self.fail_once:
                self.fail_once = False
                raise IOError("interrupted before PR creation")
            pr = {"state": "open", "html_url": "fixture/pr/1"}
            self.proposals.append(pr)
            return pr
        self.fail(route)

    def propose(self, name="pricing"):
        with patch.object(module_update, "git", lambda *args: self.git(self.parent, *args)):
            module_update.propose(name, self.api)

    def test_clone_update_and_retry(self):
        clone = self.root / "full"
        self.git(self.root, *self.local, "clone", "--recurse-submodules", str(self.parent), str(clone))
        self.assertEqual(self.git(clone / "modules/pricing", "rev-parse", "HEAD"), self.old)
        self.fail_once = True
        with self.assertRaises(IOError):
            self.propose()
        self.propose()
        self.propose()
        self.assertEqual(len(self.proposals), 1)
        self.assertEqual(self.git(self.parent, "diff", "--name-only", self.base, self.proposal_sha),
                         "modules/pricing")
        self.assertEqual(self.git(self.parent, "rev-parse", "main"), self.base)
        self.git(self.parent, "merge", "--ff-only", self.proposal_sha)
        self.git(clone, *self.local, "pull", "--ff-only", "--recurse-submodules")
        self.assertEqual(self.git(clone / "modules/pricing", "rev-parse", "HEAD"), self.new)
        self.git(clone, "checkout", self.base)
        self.git(clone, *self.local, "submodule", "update", "--init", "--recursive")
        self.assertEqual(self.git(clone / "modules/pricing", "rev-parse", "HEAD"), self.old)

    def test_diverged_revision_refused(self):
        self.forward = False
        with self.assertRaisesRegex(ValueError, "non-forward"):
            self.propose()
        self.assertIsNone(self.proposal_sha)

    def test_unregistered_module_refused(self):
        with self.assertRaises(subprocess.CalledProcessError):
            self.propose("unknown")
        self.assertFalse(self.proposals)
