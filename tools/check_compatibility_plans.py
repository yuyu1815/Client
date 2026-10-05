#!/usr/bin/env python3
"""Small documentation boundary check; not a compatibility proof."""
import argparse
import hashlib
import re
import subprocess
from pathlib import Path

BASE = "a12e38d9ea09d290a0b1736fe48a1cdafe49325f"
PROTECTED = {
    "README.md": "74ba0856c8a6e14b193c5071dd6fa3dc2e42a8ee3a04b52e776c7a40f5f3fe21",
    "Client/docs/compatibility/README.md": "afa71bfc2228992393a1c6c0466b0edbc4201426ee7c210e83fec285949285cb",
    "Client/docs/compatibility/data.json": "80475006d86c919790edc5e05a2e834b4341d699f49f1b3a7a0c02951da755e5",
}
PLANS = """atmosphere audio block-entities block-rendering chat connection-and-protocol
hud interaction inventory item-rendering launcher living-entities menus
nonliving-entities particles physics resources server-gameplay server-world
singleplayer-lifecycle""".split()
ALLOWED = {f"docs/compatibility/plans/{name}.md" for name in PLANS} | {
    "docs/compatibility/plan.md", "tools/check_compatibility_plans.py"
}
SOURCE = re.compile(r"(?P<path>(?:[A-Za-z]:/)?[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*\.(?:rs|java|ts)):(?P<lines>\d+(?:[-–,]\d+)*)")
LINK = re.compile(r"\]\(([^)#]+\.md)(?:#[^)]*)?\)")


def line_numbers(spec):
    result = set()
    for part in spec.split(","):
        ends = part.replace("–", "-").split("-")
        lo, hi = int(ends[0]), int(ends[-1])
        if lo < 1 or hi < lo:
            raise ValueError(f"bad line range: {spec}")
        result.update(range(lo, hi + 1))
    return result


def check(root, reference):
    docs = root / "docs/compatibility"
    roadmap = docs / "plan.md"
    assert roadmap.is_file(), "missing docs/compatibility/plan.md"
    text = roadmap.read_text(encoding="utf-8")
    assert BASE in text, "roadmap is missing base commit metadata"
    for name in PLANS:
        assert (docs / "plans" / f"{name}.md").is_file(), f"missing plan: {name}"
        assert f"plans/{name}.md" in text, f"roadmap index missing: {name}"
    checked = 0
    for file in [roadmap, *sorted((docs / "plans").glob("*.md"))]:
        content = file.read_text(encoding="utf-8")
        for target in LINK.findall(content):
            assert (file.parent / target).resolve().is_file(), f"broken doc link: {file}: {target}"
        for match in SOURCE.finditer(content):
            name = match.group("path")
            if len(name) > 2 and name[1:3] == ":/":
                absolute = Path(name)
                try:
                    name = absolute.relative_to(reference).as_posix()
                except ValueError:
                    raise AssertionError(f"source citation outside reference root: {name}")
            candidates = [reference / name]
            if name.startswith("Client/"):
                candidates.append(reference / name)
            elif name.startswith("pomme-client/"):
                candidates.append(reference / "Client" / name)
            source = next((p for p in candidates if p.is_file()), None)
            assert source is not None, f"missing cited source: {match.group(0)}"
            total = len(source.read_text(encoding="utf-8", errors="replace").splitlines())
            lines = line_numbers(match.group("lines"))
            assert max(lines) <= total, f"source line out of range: {match.group(0)} ({total} lines)"
            checked += 1
    assert checked, "no explicit source citations found"
    for name, expected in PROTECTED.items():
        path = reference / name
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        assert actual == expected, f"protected userdata hash changed: {name}"
    changed = subprocess.run(
        ["git", "-C", str(root), "diff", "--name-only", BASE, "HEAD"],
        check=True, capture_output=True, text=True, encoding="utf-8",
    ).stdout.splitlines()
    unexpected = set(changed) - ALLOWED
    assert not unexpected, f"forbidden file changes: {sorted(unexpected)}"
    assert set(changed) >= {"docs/compatibility/plan.md", "tools/check_compatibility_plans.py"}, "roadmap/checker not included"
    print(f"OK: {len(PLANS)} plans + roadmap; {checked} source citations; protected hashes; docs-only diff ({len(changed)} files)")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference-root", type=Path, required=True,
                        help="read-only common root containing Client/ and source snapshots")
    args = parser.parse_args()
    # Runnable self-check for the only range parser used by citation validation.
    assert line_numbers("3-4,8–9") == {3, 4, 8, 9}
    assert len(PLANS) == 20 and BASE == "a12e38d9ea09d290a0b1736fe48a1cdafe49325f"
    check(Path(__file__).resolve().parents[1], args.reference_root.resolve())


if __name__ == "__main__":
    main()
