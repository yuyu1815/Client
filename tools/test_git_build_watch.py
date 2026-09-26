"""Check Steel's compiled build script against Git layouts without changing this repo.

After building pomme-client, pass the steel-core build_script_build executable:
    python3 tools/test_git_build_watch.py /path/to/build_script_build
"""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
STEEL = ROOT / "third_party/SteelMC/steel-core"
BINARY = Path(sys.argv[1]).resolve()


def git(directory, *args):
    return subprocess.check_output(
        ["git", "-C", str(directory), *args], stderr=subprocess.PIPE, text=True
    ).strip()


def check(directory, missing=False):
    env = os.environ.copy()
    for key in ("GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE"):
        env.pop(key, None)
    env["CARGO_MANIFEST_DIR"] = str(STEEL)
    env["GIT_DIR"] = str(directory / "missing") if missing else git(directory, "rev-parse", "--absolute-git-dir")
    env["GIT_WORK_TREE"] = str(directory)
    output = subprocess.check_output([str(BINARY)], cwd=STEEL, env=env, text=True)
    watched = {Path(line.split("=", 1)[1]) for line in output.splitlines()
               if line.startswith("cargo:rerun-if-changed=")}
    assert all((STEEL / path).exists() for path in watched), watched
    if missing:
        assert "cargo:rustc-env=GIT_HASH=unknown" in output
    else:
        assert f"cargo:rustc-env=GIT_HASH={git(directory, 'rev-parse', 'HEAD')}" in output
        for name in ("HEAD", "refs", "packed-refs"):
            expected = Path(git(directory, "rev-parse", "--path-format=absolute", "--git-path", name))
            if expected.exists():
                assert expected in watched, (name, watched)


with tempfile.TemporaryDirectory() as temporary:
    repo = Path(temporary) / "repo"
    repo.mkdir()
    git(repo, "init", "-q")
    git(repo, "-c", "user.name=Build test", "-c", "user.email=test@example.invalid",
        "commit", "-q", "--allow-empty", "-m", "test")
    check(repo)
    git(repo, "pack-refs", "--all")
    check(repo)
    linked = Path(temporary) / "linked"
    git(repo, "worktree", "add", "--detach", str(linked))
    assert (linked / ".git").is_file()
    check(linked)
    check(repo, missing=True)

assert (STEEL.parent / ".git").is_file(), "Expected this checkout to contain the Steel submodule"
check(STEEL.parent)
print("Git watch checks passed: normal, packed refs, worktree, no Git, submodule")
