"""Check the repository guard against representative checkout changes."""

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORGE = Path(".agents/plugins/oracle-forge/scripts/forge.py")


class FoundationTest(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.repo = Path(scratch.name)
        files = subprocess.check_output(["git", "-C", str(ROOT), "ls-files", "-z"])
        for raw in files.split(b"\0"):
            if raw:
                path = Path(raw.decode())
                target = self.repo / path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / path, target)
        self.git("init", "-b", "main")
        self.git("add", ".")

    def git(self, *args):
        subprocess.run(["git", *args], cwd=self.repo, check=True, capture_output=True)

    def verify(self):
        return subprocess.run([sys.executable, "-B", str(self.repo / FORGE), "verify"],
                              cwd=self.repo, capture_output=True, text=True)

    def test_clean_checkout(self):
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_license_byte_change(self):
        with (self.repo / "LICENSE.md").open("ab") as license_file:
            license_file.write(b"\n")
        result = self.verify()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("License bytes differ", result.stderr)

    def test_dangling_guide(self):
        with (self.repo / "README.md").open("a", encoding="utf-8") as guide:
            guide.write("\n[Guide](missing.md)\n")
        result = self.verify()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing.md", result.stderr)

    def test_untracked_skill(self):
        skill = ".agents/plugins/oracle-forge/skills/commands/SKILL.md"
        self.git("rm", "--cached", skill)
        result = self.verify()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing from Git index: " + skill, result.stderr)

    def test_misdirected_marketplace(self):
        market = self.repo / ".agents/plugins/marketplace.json"
        market.write_text(market.read_text(encoding="utf-8").replace("./.agents/", "./wrong/"),
                          encoding="utf-8")
        result = self.verify()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Marketplace must resolve", result.stderr)


if __name__ == "__main__":
    unittest.main()
