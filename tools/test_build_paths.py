"""Check just recipe path handling without downloading or running Java/Cargo.
Run: python3 tools/test_build_paths.py
"""
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap

ROOT = Path(__file__).resolve().parents[1]
recipes = (ROOT / "justfile").read_text()
openal = textwrap.dedent(recipes.split("\nopenal:\n", 1)[1].split("\nclient-dev", 1)[0])
stategen = recipes.split('stategen version="26.2":\n', 1)[1]
jdk_setup = textwrap.dedent(stategen.split('    classes=', 1)[0])
assert '\njdk := ""\n' in recipes

with tempfile.TemporaryDirectory() as temporary:
    work = Path(temporary)
    tools = work / "tools with spaces"
    tools.mkdir()
    capture = work / "arguments.txt"
    python = tools / "python3"
    python.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$CAPTURE"\n')
    python.chmod(0o755)
    env = os.environ.copy()
    env.update(PATH=str(tools) + os.pathsep + env["PATH"], CAPTURE=str(capture))
    for target in (None, "relative build dir", str(work / "absolute build dir")):
        env.pop("CARGO_TARGET_DIR", None)
        if target is not None:
            env["CARGO_TARGET_DIR"] = target
        subprocess.run(["bash", "-c", openal], cwd=work, env=env, check=True)
        base = target or "target"
        assert capture.read_text().splitlines() == [
            "tools/fetch_openal.py", f"{base}/debug", f"{base}/dev-fast", f"{base}/release"
        ]

    for override, home, expected in (
        ("", str(work / "JDK home"), str(work / "JDK home/bin")),
        (str(work / "custom bin"), str(work / "other JDK"), str(work / "custom bin")),
        ("", None, None),
    ):
        env.pop("JAVA_HOME", None)
        if home is not None:
            env["JAVA_HOME"] = home
        script = jdk_setup.replace("{{ version }}", "26.2").replace("{{ jdk }}", override)
        result = subprocess.run(
            ["bash", "-c", script + '\nprintf "%s" "$jdk"'],
            cwd=work, env=env, text=True, capture_output=True,
        )
        if expected is None:
            assert result.returncode != 0 and "Set JAVA_HOME" in result.stderr
        else:
            assert result.returncode == 0, result.stderr
            assert result.stdout == expected, result.stdout

print("Build paths passed: default/relative/absolute target, spaces, JAVA_HOME, override, missing JDK")
