"""Prepare and propose a registered private module update from the trusted Oracle checkout."""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

PARENT = "eighred/oracle"
SHA = re.compile(r"[0-9a-f]{40}")


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def api(endpoint, token_name, body=None):
    env = os.environ.copy()
    env["GH_TOKEN"] = env[token_name]
    cmd = ["gh", "api", endpoint]
    if body is not None:
        cmd += ["--method", "POST", "--input", "-"]
    return json.loads(subprocess.check_output(cmd, text=True, env=env,
                                               input=json.dumps(body) if body is not None else None))


def registration(name):
    if not re.fullmatch(r"[a-z][a-z0-9_]{0,62}", name):
        raise ValueError("Invalid registered module name")
    path = git("config", "--blob", "HEAD:.gitmodules", "--get", f"submodule.{name}.path")
    url = git("config", "--blob", "HEAD:.gitmodules", "--get", f"submodule.{name}.url")
    repo = re.fullmatch(r"https://github\.com/eighred/([a-z0-9][a-z0-9_-]*)\.git", url)
    if path != f"modules/{name}" or not repo or repo[1] == "oracle":
        raise ValueError("Expected a registered Eighred module at its canonical path")
    entry = git("ls-tree", "HEAD", "--", path)
    mode, _, record = entry.partition("\t")
    parts = mode.split()
    if record != path or len(parts) != 3 or parts[:2] != ["160000", "commit"] or not SHA.fullmatch(parts[2]):
        raise ValueError("No pinned Git submodule at the registered path")
    return repo[1], path, parts[2]


def propose(name, request=api):
    repo, path, old = registration(name)
    src = f"repos/eighred/{repo}"
    parent = f"repos/{PARENT}"

    def module_read(route):
        return request(src + route, "MODULE_READ_TOKEN")

    def oracle(route, body=None):
        return request(parent + route, "ORACLE_PR_TOKEN", body)

    meta = module_read("")
    if not meta["private"] or meta["full_name"] != f"eighred/{repo}" or meta["default_branch"] != "main":
        raise ValueError("The registered module must be private and use protected main")
    branch = module_read("/branches/main")
    new = branch["commit"]["sha"]
    if not branch["protected"] or not SHA.fullmatch(new):
        raise ValueError("Expected a protected source branch and a full commit SHA")
    if old == new:
        print("The parent already pins the current module revision.")
        return
    if module_read(f"/compare/{old}...{new}")["status"] != "ahead":
        raise ValueError("A non-forward module update requires a reviewed manual PR")
    base = git("rev-parse", "HEAD")
    if oracle("/branches/main")["commit"]["sha"] != base:
        raise ValueError("Oracle main moved. Run from its current revision.")
    head = f"automation/module-{name}-{new}"
    prior = oracle(f"/pulls?state=all&head=eighred:{head}&base=main")
    if prior:
        print("Existing proposal:", prior[0]["html_url"], prior[0]["state"])
        return

    entry = {"path": path, "mode": "160000", "type": "commit", "sha": new}

    def tree_on(commit):
        return oracle("/git/trees", {"base_tree": commit["tree"]["sha"], "tree": [entry]})["sha"]

    refs = oracle(f"/git/matching-refs/heads/{head}")
    exact = next((r for r in refs if r["ref"] == f"refs/heads/{head}"), None)
    if exact:
        # A previous attempt may have created the branch before its PR. Check the full tree.
        tip = oracle("/git/commits/" + exact["object"]["sha"])
        if len(tip["parents"]) != 1:
            raise ValueError("Unexpected existing proposal branch history")
        previous = tip["parents"][0]["sha"]
        relationship = oracle(f"/compare/{previous}...{base}")["status"]
        if relationship not in {"identical", "ahead"}:
            raise ValueError("Existing branch has an unrelated base")
        if tip["tree"]["sha"] != tree_on(oracle("/git/commits/" + previous)):
            raise ValueError("Existing branch includes unrelated changes")
    else:
        tree = tree_on(oracle("/git/commits/" + base))
        commit = oracle("/git/commits", {"message": f"Update {name} to {new[:12]}",
                                         "parents": [base], "tree": tree})
        oracle("/git/refs", {"ref": f"refs/heads/{head}", "sha": commit["sha"]})
    pr = oracle("/pulls", {
        "base": "main", "head": head, "title": f"Update {name} to {new[:12]}",
        "body": f"Advance the registered private module at `{path}` from `{old}` to `{new}`.\n\n"
                "Verify affected contracts and product integration against the pinned revision. "
                "The module owner retains its source and history. See #6.",
    })
    print(pr["html_url"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("registration", "propose"))
    parser.add_argument("module")
    args = parser.parse_args()
    if args.action == "registration":
        repo, _, _ = registration(args.module)
        with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
            print(f"repository={repo}", file=output)
    else:
        propose(args.module)


if __name__ == "__main__":
    try:
        main()
    except (KeyError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
