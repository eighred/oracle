"""Inspect and validate an Oracle checkout. All operations are local and read-only."""

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path
from urllib.parse import unquote, urlsplit

PACKAGE = Path(".agents/plugins/oracle-forge")
LICENSE_SHA256 = "f6a49da8cdb7464016d05e626c105429b35360fe59e1fa4261ad477386281141"
SKILLS = ("commands", "engineering-standard", "financial-verification", "issue-archaeology",
          "oracle-verify", "pr-delivery", "repository-organization", "security-audit", "stack-routing")
REQUIRED = (
    "README.md", "AGENTS.md", "CONTRIBUTING.md", "SECURITY.md", "LICENSE.md",
    ".github/CODEOWNERS", ".github/pull_request_template.md", ".github/ISSUE_TEMPLATE/work.yml",
    ".github/workflows/foundation.yml", ".github/dependabot.yml", ".gitleaks.toml",
    ".github/governance/main.json", ".github/governance/tag_creation.json",
    ".github/governance/tag_immutability.json", ".agents/skills/oracle-forge/SKILL.md",
)


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args])


def inventory(root):
    return {Path(name.decode("utf-8")) for name in git(root, "ls-files", "-z").split(b"\0") if name}


def validate(root):
    tracked = inventory(root)
    errors = []
    manifest_path = PACKAGE / ".codex-plugin/plugin.json"
    market_path = Path(".agents/plugins/marketplace.json")
    skill_files = [PACKAGE / "skills" / name / "SKILL.md" for name in SKILLS]
    expected = {*map(Path, REQUIRED), *skill_files, market_path, manifest_path,
                PACKAGE / "scripts/validate.py", Path(".codex/config.toml")}
    for path in sorted(expected - tracked):
        errors.append(f"Missing from Git index: {path.as_posix()}")
    if hashlib.sha256((root / "LICENSE.md").read_bytes()).hexdigest() != LICENSE_SHA256:
        errors.append("License bytes differ from the designated Eighred source")
    manifest = json.loads((root / manifest_path).read_text(encoding="utf-8"))
    market = json.loads((root / market_path).read_text(encoding="utf-8"))
    settings = tomllib.loads((root / ".codex/config.toml").read_text(encoding="utf-8"))
    entries = [item for item in market["plugins"] if item["name"] == "oracle-forge"]
    if len(entries) != 1 or entries[0]["source"] != {"source": "local", "path": "./" + PACKAGE.as_posix()}:
        errors.append("Marketplace must resolve the Oracle Forge package")
    if market["name"] != "oracle-local" or not settings["plugins"]["oracle-forge@oracle-local"]["enabled"]:
        errors.append("Project plugin selector does not resolve the enabled marketplace")
    if manifest["name"] != "oracle-forge" or manifest["skills"] != "./skills/":
        errors.append("Invalid plugin identity or skills directory")
    if not re.fullmatch(r"\d+\.\d+\.\d+", manifest["version"]):
        errors.append("Invalid package version")
    for path in skill_files:
        content = (root / path).read_text(encoding="utf-8")
        header = content.split("---\n", 2)
        if len(header) != 3 or header[0] or f"name: {path.parent.name}" not in header[1].splitlines():
            errors.append(f"Invalid skill identity: {path}")
    for path in sorted(tracked):
        if path.suffix.lower() != ".md" or path == Path("LICENSE.md"):
            continue
        if path.is_relative_to(PACKAGE):
            continue  # Package references are checked by its own validator.
        content = (root / path).read_text(encoding="utf-8")
        for dest in re.findall(r"\[[^\]\n]*\]\(([^()\s]+)\)", content):
            url = urlsplit(dest)
            if url.scheme in {"https", "http", "mailto"} or not url.path:
                continue
            link = (root / path.parent / unquote(url.path)).resolve()
            if not link.is_relative_to(root) or link.relative_to(root) not in tracked or not link.is_file():
                errors.append(f"Unresolved tracked file link in {path.as_posix()}: {dest}")
    return manifest, errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("doctor", "verify"))
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    args = parser.parse_args()
    try:
        root = Path(git(args.repo, "rev-parse", "--show-toplevel").decode().strip()).resolve()
        manifest, errors = validate(root)
        if errors:
            raise ValueError("\n".join(errors))
        if args.command == "doctor":
            revision = git(root, "rev-parse", "HEAD").decode().strip()
            print(json.dumps({"package": manifest["name"], "version": manifest["version"],
                              "checkout": str(root), "revision": revision, "skills": SKILLS}, indent=2))
        else:
            print("Verified license, repository entrypoints, plugin discovery, skills and tracked file links.")
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as exc:
        print(f"Oracle Forge: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
