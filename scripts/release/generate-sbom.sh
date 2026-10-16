#!/usr/bin/env bash
# Writes CycloneDX SBOMs into <out-dir>:
#   <crate>.cdx.json  one per Rust workspace member (cargo-cyclonedx, must be on PATH)
#   frontend.cdx.json for apps/desktop (@cyclonedx/cdxgen, reads pnpm-lock.yaml)
#
# pnpm 10 has no `pnpm sbom` command, so the frontend SBOM uses cdxgen.
# Run `pnpm install --frozen-lockfile` in apps/desktop first.
#
# Usage: scripts/release/generate-sbom.sh <out-dir>
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <out-dir>" >&2
  exit 2
fi

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
mkdir -p "$1"
out_dir=$(cd "$1" && pwd)
cd "$repo_root"

# Rust: cargo-cyclonedx writes <crate>.cdx.json next to each Cargo.toml. Each
# run also rewrites the files of the other members, so the copies it leaves in
# the source tree are removed below.
cargo metadata --no-deps --locked --format-version 1 \
  | python3 -c 'import json,sys; [print(p["name"], p["manifest_path"]) for p in json.load(sys.stdin)["packages"]]' \
  | while read -r name manifest; do
      cargo cyclonedx --manifest-path "$manifest" --format json --spec-version 1.5 --target all </dev/null
      mv "$(dirname "$manifest")/$name.cdx.json" "$out_dir/$name.cdx.json"
    done
find "$repo_root" \( -name target -o -name node_modules -o -name .git -o -path "$out_dir" \) -prune \
  -o -name '*.cdx.json' -exec rm -f {} +

# Frontend: cdxgen is pinned to a version. It reads apps/desktop/pnpm-lock.yaml.
(cd apps/desktop && npx --yes @cyclonedx/cdxgen@12.8.6 -t js --spec-version 1.5 -o "$out_dir/frontend.cdx.json" .)

for f in "$out_dir"/*.cdx.json; do
  python3 -c 'import json,sys; json.load(open(sys.argv[1], encoding="utf-8"))' "$f"
done
echo "SBOMs written to $out_dir:"
ls -1 "$out_dir"
