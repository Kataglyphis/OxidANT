#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd -- "${script_dir}/../.." && pwd)"
cd "${root}"

# Read the CI guide list so a local check uses the same pages as docs.yml.
guides="$(sed -n '/^      guides: |$/,/^      link-allow:/p' .github/workflows/docs.yml | sed '1d;$d;s/^        //')"
[ -n "${guides}" ] || { printf 'No docs.yml guides found\n' >&2; exit 1; }

DOCS_BUILDER=rustdoc DOCS_GUIDES="${guides}" \
  bash third_party/ANTfrastructure/linux/scripts/docs-site.sh build target/doc
python3 third_party/ANTfrastructure/linux/scripts/verify_site_links.py \
  target/doc --allow docs/site-links.allow
