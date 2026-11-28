#!/usr/bin/env bash
# castlist と pagetray の定義（Smithy）から、呼ぶ側の crate（crates/castlist-client/・crates/pagetray-client/）を作り直す。
# callboard は自分の定義を持たない（画面のサーバーで、API は castlist と pagetray の物を呼ぶだけ）。
# 生成した物は repo に入れ、手で直さない。
#   ci/gen.sh           定義（model/castlist/・model/pagetray/）を下の tag から取り直し、2 つの crate を作り直して置き換える
#   ci/gen.sh --check   取った定義が下の digest と合うか、作り直した物と入れてある物が同じかを見る（CI が流す）。違えば exit 1。
#                       castlist と pagetray には取りに行かない（入れてある物を digest で見る）
# 要る物: curl・unzip・jq・shasum か sha256sum・rustfmt（rustup。rust-toolchain.toml の版）。Smithy の CLI は JRE を同梱しているので、Java は要らない。
# 生成器の版は model/smithy-build.json に書く。そこに名指した Maven の物は、公開から 2 日たっていなければ使わない
# （crate と pnpm の待ちに合わせた物。名指した物だけを見て、それが引く物は見ない）。
# 作るのは repo の中（model/build。git は無視）。字の並びは rust-toolchain.toml の版の rustfmt で整える（下の 4）。
# 作り方は pagetray の ci/gen.sh と同じ（違うのは、定義がみな外から取った物で、作るのが呼ぶ側の crate 2 つなこと。
# 自分の定義が無いので、壊す変わりを見る --diff は無い）。
# macOS の bash（3.2）でも動く書き方にする。
set -euo pipefail

SMITHY_VERSION=1.74.1
# release の digest（https://github.com/smithy-lang/smithy/releases/tag/1.74.1）。
# 版は model/smithy-build.json の trait の package（smithy-aws-traits など）と同じにする。上げる時は 2 か所と digest を一緒に替える。
sha_for() {
  case "$1" in
    darwin-aarch64) echo ebb5e931936aa503055b4620c4981319f2b060656482553ada0151235ff0f1ac ;;
    darwin-x86_64) echo 06b5fa6660fd3ab0281511af1d04f3128e10fa6220c552bc2b94ff18d660886d ;;
    linux-aarch64) echo c563b5387c66fe04b8c459178dfcc08316798bc107c03fd92474dd8a0c028850 ;;
    linux-x86_64) echo 6b50c0b9f2443400bb7115b9badb54b3609816ec5fb4f625954eadd2f6b2f52a ;;
    *) return 1 ;;
  esac
}
# 取る定義を固める所。ここだけに書く。上げる時は tag と digest を一緒に替え、ci/gen.sh を流して、取った定義と
# 2 つの crate を作り直し、同じ commit に入れる。
# pagetray の定義は ai.common（castlist の common.smithy）を使う。pagetray のその版が固めている castlist の版と、
# 下の castlist の版の common.smithy が合わない時は、smithy build が落ちる（その時は、castlist と pagetray の版を揃えて上げる）。
# 並びは「置く folder:file:digest」。
CASTLIST_TAG=v2.2.0
PAGETRAY_TAG=v2.1.0
PINS="castlist:common.smithy:6425614b79d58d7149f17396cebc3eb30e0e6c511516957d82004eb329fb66d0
castlist:castlist.smithy:f54ca9bec27404382b3f696d08cd5fecac5fff09056bd9824dd2ec2250c20539
pagetray:pagetray.smithy:f9e4d1db81d35847f73ff3e95a91b55a9899ff9f74d85cd8d5ca93f592faa1ec"
# 取る所（folder の名 → その repo の、tag の model/model/）。
raw_for() {
  case "$1" in
    castlist) echo "https://raw.githubusercontent.com/uraitakahito/castlist/$CASTLIST_TAG/model/model" ;;
    pagetray) echo "https://raw.githubusercontent.com/uraitakahito/pagetray/$PAGETRAY_TAG/model/model" ;;
    *) return 1 ;;
  esac
}
tag_for() {
  case "$1" in
    castlist) echo "$CASTLIST_TAG" ;;
    pagetray) echo "$PAGETRAY_TAG" ;;
    *) return 1 ;;
  esac
}
MIN_AGE_SECONDS=$((2 * 24 * 60 * 60))
# つなぎの一時の失敗（github.com や Maven Central への時間切れ）は、少し待って送り直す（castlist の AI-36 の台帳 #153 と同じ）。
# 送り直すのは時間切れ・5xx・つなぐ前の断り。404 などの書き違いは早く落とす（--retry-all-errors は使わない）。
CURL_RETRY="--retry 5 --retry-connrefused --retry-delay 5 --connect-timeout 20"

root=$(cd "$(dirname "$0")/.." && pwd)
# 作る crate: 生成器の plugin の名（同じ plugin を 2 度使う物は、smithy-build.json の `<plugin>::<名>` の名）と、置き場。
crates="rust-client-codegen:crates/castlist-client pagetray-client:crates/pagetray-client"
check=false
case "${1:-}" in
  --check) check=true ;;
  "") ;;
  *) echo "使い方: ci/gen.sh [--check]" >&2; exit 2 ;;
esac

ng() {
  echo "NG: $*" >&2
  exit 1
}
sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# 1. 取った定義（model/castlist/・model/pagetray/）。--check は入れてある物を digest で見るだけ。ほかは tag から取り直してから見る。
if ! $check; then
  for pin in $PINS; do
    dir=${pin%%:*}
    rest=${pin#*:}
    f=${rest%%:*}
    want=${rest#*:}
    mkdir -p "$work/$dir"
    curl -fsSL $CURL_RETRY -o "$work/$dir/$f" "$(raw_for "$dir")/$f"
    got=$(sha256 "$work/$dir/$f")
    [ "$got" = "$want" ] || ng "$dir の $(tag_for "$dir") の $f が、固めた digest と違う（${got}。$want のはず）。model/$dir/ は替えない"
  done
  for pin in $PINS; do
    dir=${pin%%:*}
    rest=${pin#*:}
    f=${rest%%:*}
    mkdir -p "$root/model/$dir"
    cp "$work/$dir/$f" "$root/model/$dir/$f"
  done
fi
for pin in $PINS; do
  dir=${pin%%:*}
  rest=${pin#*:}
  f=${rest%%:*}
  want=${rest#*:}
  [ -f "$root/model/$dir/$f" ] || ng "取った定義が無い: model/$dir/${f}（ci/gen.sh を流す）"
  got=$(sha256 "$root/model/$dir/$f")
  [ "$got" = "$want" ] || ng "model/$dir/$f が $dir の $(tag_for "$dir") の物と違う（${got}。$want のはず）"
done
# 固めた file のほかの物が、定義の folder に無いか（あれば、digest を見ないまま生成に入る）。
for dir in castlist pagetray; do
  for path in "$root/model/$dir"/*; do
    f=$(basename "$path")
    echo "$PINS" | grep -qx "$dir:$f:[0-9a-f]*" || ng "model/$dir/ に、取った定義のほかの file がある: $f"
  done
done

# 2. Smithy の CLI を取る（版と digest を確かめ、cache に置く）。
os=$(uname -s | tr '[:upper:]' '[:lower:]')
arch=$(uname -m)
[ "$arch" = arm64 ] && arch=aarch64
platform="$os-$arch"
want=$(sha_for "$platform") || ng "Smithy の CLI の無い機械: $platform"
cache="${XDG_CACHE_HOME:-$HOME/.cache}/callboard-smithy/$SMITHY_VERSION-$platform"
smithy="$cache/smithy-cli-$platform/bin/smithy"
# cache（手元の folder と、CI の actions/cache）には、取った zip を残す。毎回 zip の digest を見てから、zip から開き直す
# （開いた物を cache から戻してそのまま使うと、digest を見ないまま走らせることになるため）。
zip="$cache/cli.zip"
if [ ! -f "$zip" ] || [ "$(sha256 "$zip")" != "$want" ]; then
  rm -rf "$cache"
  mkdir -p "$cache"
  curl -fsSL $CURL_RETRY -o "$zip" "https://github.com/smithy-lang/smithy/releases/download/$SMITHY_VERSION/smithy-cli-$platform.zip"
  got=$(sha256 "$zip")
  [ "$got" = "$want" ] || ng "Smithy の CLI の digest が違う（${got}。$want のはず）"
fi
rm -rf "$cache/smithy-cli-$platform"
unzip -q -o "$zip" -d "$cache"
[ -x "$smithy" ] || ng "Smithy の CLI の zip に bin/smithy が無い"

# 3. 生成器（Maven の物）が、公開から 2 日たっているか。
# 依存は JSON として読む（.maven.dependencies）。どれも group:artifact:版 の形でなければ断る。
# 公開の時刻は増えないので、一度 2 日を越えたと見た物は、cache に印を置き、次からは Maven Central に聞かない
# （CI で何度も流すと、Maven Central が 429 で断ることがあった）。
now=$(date -u +%s)
deps=$(jq -r '.maven.dependencies // [] | .[]' "$root/model/smithy-build.json") || ng "model/smithy-build.json を JSON として読めない"
[ -n "$deps" ] || ng "model/smithy-build.json に Maven の依存が無い"
aged="$cache/maven-aged"
mkdir -p "$aged"
for dep in $deps; do
  echo "$dep" | grep -Eqx '[a-zA-Z0-9._-]+:[a-zA-Z0-9._-]+:[0-9][a-zA-Z0-9._-]*' || ng "Maven の依存の形が違う: $dep"
  [ -f "$aged/$dep" ] && continue
  group=${dep%%:*}
  rest=${dep#*:}
  artifact=${rest%%:*}
  version=${rest#*:}
  pom="https://repo1.maven.org/maven2/$(echo "$group" | tr . /)/$artifact/$version/$artifact-$version.pom"
  modified=$(curl -fsSI $CURL_RETRY "$pom" | tr -d '\r' | sed -n 's/^[Ll]ast-[Mm]odified: //p')
  [ -n "$modified" ] || ng "$dep の公開の時刻を読めない（${pom}）"
  if date -u -d "$modified" +%s >/dev/null 2>&1; then
    published=$(date -u -d "$modified" +%s)
  else
    published=$(LC_ALL=C date -u -j -f '%a, %d %b %Y %H:%M:%S GMT' "$modified" +%s)
  fi
  [ $((now - published)) -ge $MIN_AGE_SECONDS ] || ng "$dep は公開から 2 日たっていない（${modified}）"
  : >"$aged/$dep"
done

# 4. 作る（repo の中の model/build で。git は無視する）。
build="$root/model/build"
rm -rf "$build"
log="$build.log"
(cd "$root/model" && "$smithy" build --quiet >"$log" 2>&1) || {
  cat "$log" >&2
  rm -f "$log"
  ng "smithy build が落ちた"
}
rm -f "$log"
bad=0
for pair in $crates; do
  plugin=${pair%%:*}
  dir=${pair#*:}
  made="$build/smithy/source/$plugin"
  out="$root/$dir"
  [ -f "$made/Cargo.toml" ] || ng "$plugin の crate が作られていない"
  # 字の並びを整える。生成器も cargo fmt を流すが、作る folder が workspace の外の扱いになって落ちる（warning で続く）ので、
  # ここで rustfmt を直に流す。repo の中で打つので、rust-toolchain.toml の版の rustfmt が使われる。
  (cd "$root" && find "$made" -name '*.rs' -print0 | xargs -0 rustfmt --edition 2021) || ng "rustfmt が落ちた（${plugin}）"
  # 生成した crate には clippy を掛けない。server の crate が引くので、`cargo clippy` は生成した crate も workspace の member として見る。
  # 生成器が付ける断り（lib.rs の頭の allow）は、新しい Rust の lint に追いつかないので、clippy の lint をまとめて外す 1 行を、頭に足す。
  lib="$made/src/lib.rs"
  [ -f "$lib" ] || ng "$plugin の lib.rs が無い"
  { echo '#![allow(clippy::all)]'; cat "$lib"; } >"$lib.new" && mv "$lib.new" "$lib"
  # 呼ぶ側の crate は、既定で https の client（rustls と aws-lc）を引く。callboard が castlist と pagetray を呼ぶのは、
  # 同じ機械の中の http:// だけなので、既定の features から外す（TLS の crate を build せず、license の見張りにも入れない）。
  # HTTP の client は、使う側（crates/server）が設定で渡す。
  manifest="$made/Cargo.toml"
  grep -qx 'default = \["rt-tokio", "rustls", "default-https-client"\]' "$manifest" \
    || ng "$plugin の Cargo.toml の既定の features が、思った形でない（ci/gen.sh を見直す）"
  sed 's/^default = \["rt-tokio", "rustls", "default-https-client"\]$/default = ["rt-tokio"]/' "$manifest" >"$manifest.new" \
    && mv "$manifest.new" "$manifest"
  if $check; then
    if ! diff -r "$made" "$out" >"$work/diff.txt"; then
      head -50 "$work/diff.txt" >&2
      echo "NG: $dir/ が定義と合わない。ci/gen.sh を流して、作り直した物を commit する" >&2
      bad=1
    else
      echo "$dir/ は定義（model/）と合っている"
    fi
  else
    rm -rf "$out"
    mkdir -p "$(dirname "$out")"
    cp -R "$made" "$out"
    echo "作り直した: $dir/"
  fi
done
[ "$bad" = 0 ] || exit 1
