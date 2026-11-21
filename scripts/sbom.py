#!/usr/bin/env python3
"""Writes the SBOM of one release build as CycloneDX 1.5 JSON.

Usage: cargo metadata --locked --format-version 1 --filter-platform <target> | sbom.py <version> <target> [Cargo.lock]

Lists every third-party crate that the shipped programs (app, daemon, CLI) build with for that
target: normal and build dependencies, no dev dependencies. The output has no time stamp, so the
same lock gives the same file.
"""

import json
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SHIPPED = ("natamux-app", "natamux-daemon", "natamux-cli")
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


def purl(package):
    base = f"pkg:cargo/{package['name']}@{package['version']}"
    source = package.get("source") or ""
    if source.startswith("git+"):
        # git+https://host/repo?rev=<sha>#<sha>: the commit is the part after '#'.
        url, _, commit = source[4:].partition("#")
        return f"{base}?vcs_url=git%2B{url.split('?')[0]}%40{commit}"
    return base


def shipped(metadata):
    """Ids of the third-party packages the shipped programs reach without dev dependencies."""
    packages = {p["id"]: p for p in metadata["packages"]}
    ours = set(metadata["workspace_members"])
    edges = {}
    for node in metadata["resolve"]["nodes"]:
        edges[node["id"]] = [
            dep["pkg"]
            for dep in node["deps"]
            if any(kind["kind"] in (None, "build") for kind in dep["dep_kinds"])
        ]
    seen, todo = set(), [i for i in ours if packages[i]["name"] in SHIPPED]
    while todo:
        current = todo.pop()
        if current not in seen:
            seen.add(current)
            todo.extend(edges.get(current, []))
    return packages, edges, sorted(seen - ours, key=lambda i: (packages[i]["name"], packages[i]["version"]))


def sbom(metadata, version, target, checksums):
    packages, edges, ids = shipped(metadata)
    refs = {i: purl(packages[i]) for i in ids}
    components = []
    for i in ids:
        package = packages[i]
        component = {
            "type": "library",
            "bom-ref": refs[i],
            "name": package["name"],
            "version": package["version"],
            "purl": refs[i],
        }
        if package.get("license"):
            component["licenses"] = [{"expression": package["license"]}]
        checksum = checksums.get((package["name"], package["version"], package.get("source")))
        if checksum:
            component["hashes"] = [{"alg": "SHA-256", "content": checksum}]
        if package.get("repository"):
            component["externalReferences"] = [{"type": "vcs", "url": package["repository"]}]
        components.append(component)
    app = f"pkg:generic/natamux@{version}?target={target}"
    top = sorted({refs[d] for i in metadata["workspace_members"] for d in edges.get(i, []) if d in refs})
    dependencies = [{"ref": app, "dependsOn": top}] + [
        {"ref": refs[i], "dependsOn": sorted({refs[d] for d in edges.get(i, []) if d in refs})} for i in ids
    ]
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "component": {"type": "application", "bom-ref": app, "name": "natamux", "version": version},
            "properties": [{"name": "natamux:target", "value": target}],
        },
        "components": components,
        "dependencies": dependencies,
    }


def lock_checksums(text):
    lock = tomllib.loads(text)
    return {
        (p["name"], p["version"], p.get("source")): p["checksum"]
        for p in lock.get("package", [])
        if "checksum" in p
    }


def main():
    if len(sys.argv) not in (3, 4):
        print(__doc__, file=sys.stderr)
        return 2
    lock = pathlib.Path(sys.argv[3]) if len(sys.argv) == 4 else ROOT / "Cargo.lock"
    out = sbom(json.load(sys.stdin), sys.argv[1], sys.argv[2], lock_checksums(lock.read_text()))
    json.dump(out, sys.stdout, indent=1, sort_keys=True)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
