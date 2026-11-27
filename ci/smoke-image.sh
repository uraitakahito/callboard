#!/usr/bin/env bash
# image が、空の image の中で起き、/healthz に答え、外の Host と browser からの書き込みを断り、
# 入口の画面と CSS が CSP の決まりどおりに出て、castlist と pagetray に名で届き、相関図とプロフィールが castlist の API を
# 通して出て、止める合図で自分から終わり、起こし直しても答えるかを見る。
# 呼ぶ先は、出した版の castlist と pagetray の image（ghcr.io の物を digest で取る）を、同じ network に起こす。
# 使い方: ci/smoke-image.sh <image>
# container の名は smoke-callboard で始める（callboard・castlist・pagetray の設定の守りの match（--name callboard など）に当てないため）。
# 手元で流す時は、名と port を SMOKE_NAME・SMOKE_PORT で替えられる（既定は smoke-callboard と 18813。
# 動いている pagetray・castlist・callboard の port（8801・8802・8803）とは別の port にする）。
set -euo pipefail

usage='使い方: ci/smoke-image.sh <image>'
image=${1:?$usage}
if [ $# -ne 1 ]; then
  echo "$usage" >&2
  exit 2
fi
name=${SMOKE_NAME:-smoke-callboard}
port=${SMOKE_PORT:-18813}
url=http://127.0.0.1:$port
tmp=${RUNNER_TEMP:-/tmp}
here=$(cd "$(dirname "$0")" && pwd)
# 呼ぶ先（出した版の image と版。ci/upstream.env）。版は、定義を取った tag（ci/gen.sh）と同じにする（下で突き合わせる）。
# shellcheck source=ci/upstream.env
. "$here/upstream.env"
castlist=$CASTLIST_IMAGE
castlist_version=$CASTLIST_VERSION
pagetray=$PAGETRAY_IMAGE
pagetray_version=$PAGETRAY_VERSION
# 定義を取った版と、ここで起こす image の版がずれていないか（片方だけ上げると、古い image を相手に通ってしまう）。
for pair in "CASTLIST_TAG:$castlist_version" "PAGETRAY_TAG:$pagetray_version"; do
  tag=$(sed -n "s/^${pair%%:*}=v\(.*\)\$/\1/p" "$here/gen.sh")
  if [ "$tag" != "${pair#*:}" ]; then
    echo "NG: ci/gen.sh の ${pair%%:*}（v${tag}）と、ci/upstream.env の版（${pair#*:}）が違う" >&2
    exit 1
  fi
done

up() {
  for _ in $(seq 1 40); do
    if curl -fsS "$url/healthz"; then return; fi
    sleep 0.5
  done
  return 1
}
# 後片付け: container と network を消す（落ちた時は、消す前に log を出す）。
cleanup() {
  for c in "$name" "$name-castlist" "$name-pagetray"; do
    if docker inspect "$c" >/dev/null 2>&1; then docker logs "$c" || true; fi
    docker rm -f -v "$c" >/dev/null 2>&1 || true
  done
  docker network rm "$name-net" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# 同じ network の container は、名で引ける。呼ぶ先は host に出さない（callboard だけが聞く）。
docker network create "$name-net" >/dev/null
docker run -d --name "$name-castlist" --network "$name-net" "$castlist"
docker run -d --name "$name-pagetray" --network "$name-net" "$pagetray"
docker run -d --name "$name" --network "$name-net" -p "127.0.0.1:$port:8080" \
  -e "CALLBOARD_CASTLIST_URL=http://$name-castlist:8080" \
  -e "CALLBOARD_PAGETRAY_URL=http://$name-pagetray:8080" \
  "$image"
up | tee "$tmp/healthz.json"
jq -e '.status == "ok" and (.version | length > 0) and (.now | length > 0)' "$tmp/healthz.json" >/dev/null
code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }
test "$(code -H 'Host: evil.example' "$url/healthz")" = 403
test "$(code -X POST -H 'Origin: https://evil.example' --data '{}' "$url/")" = 403
# 断る答えは、castlist・pagetray の誤りと同じ restJson1 の形（header に型の名・本文は JSON の message）。
for header in 'Host: evil.example' 'Origin: https://evil.example'; do
  curl -s -D "$tmp/forbidden-head.txt" -o "$tmp/forbidden.json" -X POST -H "$header" --data '{}' "$url/"
  tr -d '\r' <"$tmp/forbidden-head.txt" | grep -qix 'x-amzn-errortype: forbidden'
  jq -e '.message | length > 0' "$tmp/forbidden.json" >/dev/null
done
# 入口: CSP が付き（crates/server/src/lib.rs の CSP と同じ字）、inline の script と style が無く、callboard の CSS を読む。
csp="default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
# 呼ぶ先が起きるまで、少し待つ（入口は、呼ぶ先が答えなくても 200 で「届きません」と出す）。
for _ in $(seq 1 40); do
  curl -fsS -D "$tmp/head.txt" -o "$tmp/index.html" "$url/"
  if ! grep -qF '届きません' "$tmp/index.html"; then break; fi
  sleep 0.5
done
tr -d '\r' <"$tmp/head.txt" | grep -qixF "content-security-policy: $csp"
if grep -qiE '<script|<style|style=' "$tmp/index.html"; then
  echo "NG: 入口に inline の script か style がある" >&2
  exit 1
fi
grep -qF '<link rel="stylesheet" href="/static/site.css' "$tmp/index.html"
# castlist と pagetray に、container の名で届き、Host: 127.0.0.1 で聞けている（どちらも、ほかの Host を断る）。
if grep -qF '届きません' "$tmp/index.html"; then
  echo "NG: 入口が、呼ぶ先に届かないと出した" >&2
  cat "$tmp/index.html" >&2
  exit 1
fi
test "$(grep -cF 'つながっています' "$tmp/index.html")" = 2
grep -qF "<code>$castlist_version</code>" "$tmp/index.html"
grep -qF "<code>$pagetray_version</code>" "$tmp/index.html"
curl -fsS -D "$tmp/css-head.txt" -o "$tmp/site.css" "$url/static/site.css"
tr -d '\r' <"$tmp/css-head.txt" | grep -qi '^content-type: text/css'
tr -d '\r' <"$tmp/css-head.txt" | grep -qix 'x-content-type-options: nosniff'
test -s "$tmp/site.css"
test "$(code "$url/no/such")" = 404
# 相関図: 同じ CSP が付き、inline の style が無く、script は callboard が配る 2 つ（cytoscape と network.js）を src で読むだけ。
curl -fsS -D "$tmp/network-head.txt" -o "$tmp/network.html" "$url/graph/network"
tr -d '\r' <"$tmp/network-head.txt" | grep -qixF "content-security-policy: $csp"
if grep -qiE '<style|style=' "$tmp/network.html"; then
  echo "NG: 相関図に inline の style がある" >&2
  exit 1
fi
scripts=$(grep -oiE '<script[^>]*>' "$tmp/network.html" | sed -E 's/\?v=[0-9a-f]+"/"/' | LC_ALL=C sort)
want_scripts='<script src="/static/network.js">
<script src="/static/vendor/cytoscape-3.35.0.min.js">'
if [ "$scripts" != "$want_scripts" ]; then
  echo "NG: 相関図の script が、callboard の配る 2 つでない: $scripts" >&2
  exit 1
fi
grep -qF 'data-src="/api/castlist/graph"' "$tmp/network.html"
# 相関図の script と cytoscape: JavaScript の型で、cytoscape は static/vendor/ の物と同じ字。
for path in static/network.js static/vendor/cytoscape-3.35.0.min.js; do
  curl -fsS -D "$tmp/js-head.txt" -o "$tmp/got.js" "$url/$path"
  tr -d '\r' <"$tmp/js-head.txt" | grep -qi '^content-type: application/javascript'
  tr -d '\r' <"$tmp/js-head.txt" | grep -qix 'x-content-type-options: nosniff'
  cmp -s "$tmp/got.js" "$here/../$path" || {
    echo "NG: 配る $path が repo の物と違う" >&2
    exit 1
  }
done
curl -fsS -D "$tmp/icon-head.txt" -o /dev/null "$url/static/icon.svg"
tr -d '\r' <"$tmp/icon-head.txt" | grep -qi '^content-type: image/svg+xml'
# 中継の道: 本物の castlist の図の API（GetGraph）の答えが、そのまま返る（空の台帳なので、点も線も無い）。
curl -fsS -D "$tmp/graph-head.txt" -o "$tmp/graph.json" "$url/api/castlist/graph"
tr -d '\r' <"$tmp/graph-head.txt" | grep -qix 'content-type: application/json'
jq -e '.nodes == [] and .edges == [] and (.asOf | length > 0)' "$tmp/graph.json" >/dev/null
# castlist が断る引数は、castlist の誤りの形のまま返る。
test "$(curl -s -D "$tmp/graph-head.txt" -o "$tmp/graph.json" -w '%{http_code}' "$url/api/castlist/graph?label=x")" = 400
tr -d '\r' <"$tmp/graph-head.txt" | grep -qi '^x-amzn-errortype: '
jq -e '.message | length > 0' "$tmp/graph.json" >/dev/null
# プロフィール: castlist に無いアクターは 404 の画面（castlist に聞いた答え）。
test "$(curl -s -o "$tmp/profile.html" -w '%{http_code}' "$url/actors/019a0000-0000-7000-8000-0000000000ff")" = 404
grep -qF 'そのアクターは無い' "$tmp/profile.html"
# 呼ぶ先を止めても、callboard は生きていると答え、入口は届かないと出す。
docker stop "$name-pagetray" >/dev/null
curl -fsS "$url/healthz" >/dev/null
curl -fsS -o "$tmp/index.html" "$url/"
test "$(grep -cF 'つながっています' "$tmp/index.html")" = 1
test "$(grep -cF '届きません' "$tmp/index.html")" = 1
# 止める合図（SIGTERM）で、自分から終わるか（PID 1 は受け口が無いと合図を無視する）。
docker stop "$name"
test "$(docker inspect -f '{{.State.ExitCode}}' "$name")" = 0
docker start "$name"
up
# 起きた時の行に、版と聞く先が出る（grep -q に pipe でつながない: grep が先に終わると、書く側が SIGPIPE で落ちる）。
logs=$(mktemp)
docker logs "$name" >"$logs" 2>&1
grep -q "callboard .* listen=0.0.0.0:8080 castlist=http://$name-castlist:8080 pagetray=http://$name-pagetray:8080" "$logs"
grep -q 'signal SIGTERM' "$logs"
rm -f "$logs"
echo "image は動く: ${image}"
