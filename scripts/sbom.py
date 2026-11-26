#!/usr/bin/env python3
"""Writes the SBOM of one release build as CycloneDX 1.5 JSON.

Usage: cargo metadata --locked --format-version 1 --filter-platform <target> | sbom.py <version> <target> [Cargo.lock] [--rustc "<rustc -V line>"]

Lists every third-party crate that the shipped programs (app, daemon, CLI) build with for that
target: normal and build dependencies, no dev dependencies. The output has no time stamp, so the
same lock gives the same file. The SQLite that libsqlite3-sys compiles in is its own component; the
version table below needs one line for each new libsqlite3-sys version (the script fails without it).
"""

import json
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SHIPPED = ("natamux-app", "natamux-daemon", "natamux-cli")
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
# libsqlite3-sys version -> SQLite it bundles (sqlite3/bindgen_bundled_version.rs of that crate).
SQLITE = {"0.39.1": "3.54.1"}


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


def sbom(metadata, version, target, checksums, rustc=None):
    packages, edges, ids = shipped(metadata)
    refs = {i: purl(packages[i]) for i in ids}
    components = []
    sqlite_refs = {}
    for i in ids:
        package = packages[i]
        if package["name"] == "libsqlite3-sys":
            if package["version"] not in SQLITE:
                raise ValueError(f"libsqlite3-sys {package['version']} is not in the SQLITE table of sbom.py")
            sqlite_refs[i] = f"pkg:generic/sqlite@{SQLITE[package['version']]}"
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
    for sqlite in sorted(set(sqlite_refs.values())):
        components.append({
            "type": "library",
            "bom-ref": sqlite,
            "name": "sqlite",
            "version": sqlite.rsplit("@", 1)[1],
            "purl": sqlite,
            "licenses": [{"license": {"id": "blessing"}}],
        })
    app = f"pkg:generic/natamux@{version}?target={target}"
    top = sorted({refs[d] for i in metadata["workspace_members"] for d in edges.get(i, []) if d in refs})
    dependencies = [{"ref": app, "dependsOn": top}] + [
        {"ref": refs[i], "dependsOn": sorted({refs[d] for d in edges.get(i, []) if d in refs} | {sqlite_refs.get(i)} - {None})}
        for i in ids
    ] + [{"ref": sqlite, "dependsOn": []} for sqlite in sorted(set(sqlite_refs.values()))]
    metadata_out = {
        "component": {"type": "application", "bom-ref": app, "name": "natamux", "version": version},
        "properties": [{"name": "natamux:target", "value": target}],
    }
    if rustc:
        name, _, rest = rustc.strip().partition(" ")
        metadata_out["tools"] = {"components": [{"type": "application", "name": name, "version": rest.split(" ")[0]}]}
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": metadata_out,
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
    args, rustc = sys.argv[1:], None
    if "--rustc" in args:
        at = args.index("--rustc")
        rustc = args[at + 1] if at + 1 < len(args) else None
        args = args[:at] + args[at + 2 :]
    if len(args) not in (2, 3) or (rustc is None and "--rustc" in sys.argv):
        print(__doc__, file=sys.stderr)
        return 2
    lock = pathlib.Path(args[2]) if len(args) == 3 else ROOT / "Cargo.lock"
    try:
        out = sbom(json.load(sys.stdin), args[0], args[1], lock_checksums(lock.read_text()), rustc)
    except ValueError as error:
        print(f"sbom: {error}", file=sys.stderr)
        return 1
    json.dump(out, sys.stdout, indent=1, sort_keys=True)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
