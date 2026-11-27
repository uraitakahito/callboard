#!/usr/bin/env bash
# 取り込んだ外の JavaScript（static/vendor/）が、npm に出ている物と同じかを見る。1 つでも違えば exit 1。
# static/vendor/SOURCES.md の `## <名>` の節ごとに（package ごとに 1 節）、次を見る:
#   1. 節の表の記録（package・version・tarball・integrity・path・file・sha256・license_path・license_file）を読む
#   2. npm の registry の記録の tarball と integrity が、記録と同じ
#   3. tarball を落とし、その sha512 が integrity と同じ
#   4. tarball の中の path の file の sha256 が、記録の sha256 と同じ
#   5. commit した file の sha256 も同じ。license の file も、tarball の中の物と同じ
# macOS の bash（3.2）でも動く書き方にする。curl と jq と openssl が要る。
# 使い方: ci/check-vendor.sh [<package> <commit した file の代わりの写し>]
#   引数は、見張りが違いを本当に当てるかを見る時に、書き替えた写しを渡すため（その package の file の代わりに見る）。
#   記録の file は、環境変数 CHECK_VENDOR_SOURCES で替えられる（壊した記録を当てるかを見る時のため。既定は static/vendor/SOURCES.md）。
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
sources=${CHECK_VENDOR_SOURCES:-$root/static/vendor/SOURCES.md}
override_package=${1:-}
override_file=${2:-}
if [ -n "$override_package" ] && [ -z "$override_file" ]; then
  echo "使い方: ci/check-vendor.sh [<package> <写し>]" >&2
  exit 2
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0
ng() {
  echo "NG: $*" >&2
  fail=1
}

[ -f "$sources" ] || { echo "NG: 記録が無い: $sources" >&2; exit 1; }

# 節ごとに、表の行だけを別の file に分ける（`## ` の見出しから次の見出しまで）。
mkdir "$work/sections"
awk -v dir="$work/sections" '
  /^## / { n++; file = sprintf("%s/%03d.md", dir, n); next }
  n > 0 && /^\| / { print > file }
' "$sources"
sections=$(find "$work/sections" -type f | LC_ALL=C sort)
[ -n "$sections" ] || { echo "NG: SOURCES.md に節（## <名>）が無い" >&2; exit 1; }

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# 節の表の `| 名 | \`値\` |` の行から、値を読む。節に 1 つだけあること。
field() {
  local section=$1 name=$2 values count
  values=$(sed -n "s/^| $name | \`\([^\`]*\)\` |\$/\1/p" "$section")
  count=$(printf '%s' "$values" | grep -c '' || true)
  [ "$count" -eq 1 ] || { echo "NG: SOURCES.md の節（$(basename "$section")）に $name が ${count} 個（1 つのはず）" >&2; exit 1; }
  printf '%s\n' "$values"
}

# 節 1 つ（package 1 つ）を見る。
check_section() {
  local section=$1
  local package version tarball integrity path file want license_path license_file committed x got got_tarball got_integrity got_sha512
  package=$(field "$section" package)
  version=$(field "$section" version)
  tarball=$(field "$section" tarball)
  integrity=$(field "$section" integrity)
  path=$(field "$section" path)
  file=$(field "$section" file)
  want=$(field "$section" sha256)
  license_path=$(field "$section" license_path)
  license_file=$(field "$section" license_file)
  committed=$root/$file
  if [ "$package" = "$override_package" ]; then committed=$override_file; fi
  x="$work/$package"
  mkdir -p "$x"

  # --- 2. registry の記録 ---
  curl -fsSL -o "$x/meta.json" "https://registry.npmjs.org/$package/$version"
  got_tarball=$(jq -r '.dist.tarball' "$x/meta.json")
  got_integrity=$(jq -r '.dist.integrity' "$x/meta.json")
  [ "$got_tarball" = "$tarball" ] || ng "$package: registry の tarball が ${got_tarball}（記録は ${tarball}）"
  [ "$got_integrity" = "$integrity" ] || ng "$package: registry の integrity が ${got_integrity}（記録は ${integrity}）"

  # --- 3. tarball ---
  curl -fsSL -o "$x/package.tgz" "$tarball"
  got_sha512="sha512-$(openssl dgst -sha512 -binary "$x/package.tgz" | openssl base64 -A)"
  [ "$got_sha512" = "$integrity" ] || ng "$package: tarball の sha512 が ${got_sha512}（記録は ${integrity}）"

  # --- 4. tarball の中の file ---
  mkdir "$x/x"
  tar -xzf "$x/package.tgz" -C "$x/x" "$path" "$license_path"
  got=$(sha256 "$x/x/$path")
  [ "$got" = "$want" ] || ng "$package: tarball の $path の sha256 が ${got}（記録は ${want}）"

  # --- 5. commit した file ---
  if [ -f "$committed" ]; then
    got=$(sha256 "$committed")
    [ "$got" = "$want" ] || ng "$package: $committed の sha256 が ${got}（記録は ${want}）"
  else
    ng "$package: commit した file が無い: $committed"
  fi
  if [ -f "$root/$license_file" ]; then
    cmp -s "$x/x/$license_path" "$root/$license_file" || ng "$package: $license_file が tarball の $license_path と違う"
  else
    ng "$package: license の file が無い: $root/$license_file"
  fi

  echo "見た: $package ${version}（$path → ${file}、sha256 ${want}）"
}

seen_override=0
for section in $sections; do
  check_section "$section"
  if [ -n "$override_package" ] && [ "$(field "$section" package)" = "$override_package" ]; then seen_override=1; fi
done
if [ -n "$override_package" ] && [ "$seen_override" -eq 0 ]; then
  ng "SOURCES.md に package $override_package の節が無い"
fi

if [ "$fail" -ne 0 ]; then
  echo "取り込んだ file の確かめに落ちた" >&2
  exit 1
fi
echo "取り込んだ file は、みな npm の物と同じ"
