#!/usr/bin/env bash
# 出した版の castlist の image（ci/upstream.env）から、server の binary（image の中の /castlist-server）を取り出す。
# browser の試験（e2e/）が、この binary を一時の DB で起こして、callboard の相手にする（CI の job e2e）。
# image は arm64 の Linux の静的な binary なので、同じ種類の機械（CI の ubuntu-24.04-arm）でだけ動く。
# ほかの機械（macOS など）では、castlist の同じ版の tag を build した binary を使う（docs/src/development.md）。
# 使い方: ci/castlist-binary.sh <置く path>
set -euo pipefail

out=${1:?使い方: ci/castlist-binary.sh <置く path>}
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=ci/upstream.env
. "$here/upstream.env"

id=$(docker create "$CASTLIST_IMAGE")
trap 'docker rm -f "$id" >/dev/null' EXIT
docker cp "$id:/castlist-server" "$out"
chmod +x "$out"
echo "取り出した: $out（castlist $CASTLIST_VERSION）"
