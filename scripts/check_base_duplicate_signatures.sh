#!/usr/bin/env bash
# check_base_duplicate_signatures.sh
#
# Detect duplicate same-name/same-signature function definitions in bundled
# Julia Base. Existing intentional duplicates must be classified in
# docs/vm/BASE_DUPLICATE_SIGNATURE_ALLOWLIST.tsv.

set -euo pipefail

cd "$(dirname "$0")/.."

python3 scripts/check_base_duplicate_signatures.py
