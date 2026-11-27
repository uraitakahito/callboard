#!/usr/bin/env bash
# image が、空の image の中で起き、/healthz に答え、外の Host と browser からの書き込みを断り、
# 入口の画面と CSS が CSP の決まりどおりに出て、castlist と pagetray に名で届き、一覧・タイムライン・相関図・プロフィール・費えが castlist の API を、
# メッセージの一覧・新着の知らせの中継・ページの一覧・file を上げる道が pagetray の API を通して動き、止める合図で（流れを開いたままでも）自分から終わり、
# 起こし直しても答えるかを見る。
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
# どの画面も <head> で読む、字の差し替えの script 4 つ（AI-50。この順。?v= は外して比べる）。
i18n_scripts='<script src="/static/vendor/i18next-27.2.9.min.js">
<script src="/static/locales/ja.js">
<script src="/static/locales/en.js">
<script src="/static/i18n.js">'
# 相関図: 同じ CSP が付き、inline の style が無く、script は callboard が配る物を src で読むだけ（どの画面も読む字の差し替えの 4 つ
# （i18next・訳 2 つ・i18n.js。AI-50）と、cytoscape と network.js）。
curl -fsS -D "$tmp/network-head.txt" -o "$tmp/network.html" "$url/graph/network"
tr -d '\r' <"$tmp/network-head.txt" | grep -qixF "content-security-policy: $csp"
if grep -qiE '<style|style=' "$tmp/network.html"; then
  echo "NG: 相関図に inline の style がある" >&2
  exit 1
fi
scripts=$(grep -oiE '<script[^>]*>' "$tmp/network.html" | sed -E 's/\?v=[0-9a-f]+"/"/' | LC_ALL=C sort)
want_scripts='<script src="/static/i18n.js">
<script src="/static/locales/en.js">
<script src="/static/locales/ja.js">
<script src="/static/network.js">
<script src="/static/vendor/cytoscape-3.35.0.min.js">
<script src="/static/vendor/i18next-27.2.9.min.js">'
if [ "$scripts" != "$want_scripts" ]; then
  echo "NG: 相関図の script が、callboard の配る 6 つでない: $scripts" >&2
  exit 1
fi
grep -qF 'data-src="/api/castlist/graph"' "$tmp/network.html"
# 相関図の script・字の差し替えの script と、取り込んだ cytoscape・i18next: JavaScript の型で、static/ の物と同じ字。
for path in static/network.js static/i18n.js static/vendor/cytoscape-3.35.0.min.js static/vendor/i18next-27.2.9.min.js; do
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
# 一覧とタイムライン: 同じ CSP が付き、inline の script と style が無く、本物の castlist（空の台帳）に聞いた答えを出す。
for path in actors/ graph/timeline; do
  curl -fsS -D "$tmp/page-head.txt" -o "$tmp/page.html" "$url/$path"
  tr -d '\r' <"$tmp/page-head.txt" | grep -qixF "content-security-policy: $csp"
  if grep -qiE '<script|<style|style=' "$tmp/page.html"; then
    echo "NG: $path に inline の script か style がある" >&2
    exit 1
  fi
  grep -qF '条件に合うアクターはいません' "$tmp/page.html"
done
test "$(code "$url/actors/?nope=1")" = 400
# 費え: 同じ CSP が付き、inline の script と style が無く、本物の castlist（2.1.0 からの ListUsage。空の台帳）に聞いた答えを出す。
curl -fsS -D "$tmp/page-head.txt" -o "$tmp/page.html" "$url/usage/"
tr -d '\r' <"$tmp/page-head.txt" | grep -qixF "content-security-policy: $csp"
if grep -qiE '<script|<style|style=' "$tmp/page.html"; then
  echo "NG: usage/ に inline の script か style がある" >&2
  exit 1
fi
grep -qF 'この期間に、数の記録はありません' "$tmp/page.html"
test "$(code "$url/usage/?nope=1")" = 400
# プロフィール: castlist に無いアクターは 404 の画面（castlist に聞いた答え）。
test "$(curl -s -o "$tmp/profile.html" -w '%{http_code}' "$url/actors/019a0000-0000-7000-8000-0000000000ff")" = 404
grep -qF 'そのアクターは無い' "$tmp/profile.html"
# メッセージの一覧: 同じ CSP が付き、inline の style が無く、script は callboard が配る 1 つを src で読むだけ。本物の pagetray（空の置き場）の
# API（ListMessages と、1.2.0 からの GetMessageFacets）に聞いた答えを出す。
curl -fsS -D "$tmp/messages-head.txt" -o "$tmp/messages.html" "$url/messages"
tr -d '\r' <"$tmp/messages-head.txt" | grep -qixF "content-security-policy: $csp"
if grep -qiE '<style|style=' "$tmp/messages.html"; then
  echo "NG: メッセージの一覧に inline の style がある" >&2
  exit 1
fi
scripts=$(grep -oiE '<script[^>]*>' "$tmp/messages.html" | sed -E 's/\?v=[0-9a-f]+"/"/')
if [ "$scripts" != "$i18n_scripts"$'\n''<script src="/static/messages.js">' ]; then
  echo "NG: メッセージの一覧の script が、字の差し替えの 4 つと、callboard の配る 1 つでない: $scripts" >&2
  exit 1
fi
grep -qF '合うメッセージは無い' "$tmp/messages.html"
grep -qF 'data-events="/api/pagetray/events"' "$tmp/messages.html"
curl -fsS -D "$tmp/js-head.txt" -o "$tmp/got.js" "$url/static/messages.js"
tr -d '\r' <"$tmp/js-head.txt" | grep -qi '^content-type: application/javascript'
cmp -s "$tmp/got.js" "$here/../static/messages.js" || {
  echo "NG: 配る static/messages.js が repo の物と違う" >&2
  exit 1
}
# pagetray が断る引数は、pagetray の訳を書いた 400 の画面。
test "$(curl -s -o "$tmp/messages.html" -w '%{http_code}' "$url/messages?at=zz")" = 400
grep -qF 'pagetray が引数を断りました' "$tmp/messages.html"
# ページの一覧: この画面だけの CSP（本文の枠と図に、pagetray の link の頭を足した物。設定を書いていないので、既定の
# http://127.0.0.1:8801）が付き、inline の style が無く、script は字の差し替えの 4 つと、callboard が配る 1 つ。本物の pagetray（空の置き場）の
# API（ListPages と、1.3.0 からの GetPageFacets）に聞いた答えを出す。
pages_csp="default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob: http://127.0.0.1:8801; connect-src 'self'; frame-src http://127.0.0.1:8801; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
curl -fsS -D "$tmp/pages-head.txt" -o "$tmp/pages.html" "$url/pages/"
tr -d '\r' <"$tmp/pages-head.txt" | grep -qixF "content-security-policy: $pages_csp"
if grep -qiE '<style|style=' "$tmp/pages.html"; then
  echo "NG: ページの一覧に inline の style がある" >&2
  exit 1
fi
scripts=$(grep -oiE '<script[^>]*>' "$tmp/pages.html" | sed -E 's/\?v=[0-9a-f]+"/"/')
if [ "$scripts" != "$i18n_scripts"$'\n''<script src="/static/pages.js">' ]; then
  echo "NG: ページの一覧の script が、字の差し替えの 4 つと、callboard の配る 1 つでない: $scripts" >&2
  exit 1
fi
grep -qF '合うページは無い' "$tmp/pages.html"
curl -fsS -o "$tmp/got.js" "$url/static/pages.js"
cmp -s "$tmp/got.js" "$here/../static/pages.js" || {
  echo "NG: 配る static/pages.js が repo の物と違う" >&2
  exit 1
}
test "$(code "$url/pages/?kind=a&kind=b")" = 400
# file を上げる道: この callboard の画面の form からの要求（Origin が自分）だけを通し、本物の pagetray に、役割 product-manager で置く
# （browser が書いた役割は読まない）。ほかの site・Origin の無い要求は 403 で、何も置かない。
printf 'hello' >"$tmp/upload.txt"
up_path='api/pagetray/projects/smoke/keys/K-1/files?name=a.txt&role=tpm'
test "$(code -X POST -H 'Origin: https://evil.example' -H 'Content-Type: text/plain' --data-binary "@$tmp/upload.txt" "$url/$up_path")" = 403
test "$(code -X POST -H 'Content-Type: text/plain' --data-binary "@$tmp/upload.txt" "$url/$up_path")" = 403
test "$(curl -s -o "$tmp/uploaded.json" -w '%{http_code}' -X POST -H "Origin: $url" -H 'Content-Type: text/plain' --data-binary "@$tmp/upload.txt" "$url/$up_path")" = 201
jq -e '.role == "product-manager" and .workspace == "" and .name == "a" and .n == "1" and .size == 5' "$tmp/uploaded.json" >/dev/null
# 種類の合わない中身は、pagetray が断った訳（415）が、そのまま返る。
test "$(code -X POST -H "Origin: $url" -H 'Content-Type: image/png' --data-binary "@$tmp/upload.txt" "$url/api/pagetray/projects/smoke/keys/K-1/files?name=b.png")" = 415
# Key の画面に、上げた file が 1 つだけ出る。
curl -fsS -o "$tmp/key.html" "$url/pages/?project=smoke&key=K-1"
test "$(grep -c 'data-file=' "$tmp/key.html")" = 1
grep -qF 'data-file="/smoke/K-1/files/a-1.txt"' "$tmp/key.html"
grep -qF 'data-upload="/api/pagetray/projects/smoke/keys/K-1/files"' "$tmp/key.html"
# 新着の知らせの中継: 本物の pagetray の流れ（SSE）につながる。流れは終わらないので、頭を見たら切る（curl は時間切れの 28 で終わる）。
curl -s -N -D "$tmp/events-head.txt" -o /dev/null --max-time 2 "$url/api/pagetray/events" || test $? = 28
tr -d '\r' <"$tmp/events-head.txt" | grep -qi '^HTTP/1.1 200'
tr -d '\r' <"$tmp/events-head.txt" | grep -qix 'content-type: text/event-stream'
# 止める合図（SIGTERM）で、自分から終わるか（PID 1 は受け口が無いと合図を無視する）。新着の知らせの流れを開いたままでも、
# 流れを閉じて終わる（閉じないと、browser が切るまで終われず、docker が 10 秒後に SIGKILL で落とす: 終わりの code が 137 になる）。
curl -s -N -o /dev/null "$url/api/pagetray/events" &
stream=$!
sleep 1
kill -0 "$stream"
docker stop "$name"
test "$(docker inspect -f '{{.State.ExitCode}}' "$name")" = 0
# 開いていた流れは、callboard が閉じる（流れを途中で壊さずに閉じれば、curl は 0 で終わる）。
wait "$stream"
docker start "$name"
up
# 起きた時の行に、版と聞く先が出る（grep -q に pipe でつながない: grep が先に終わると、書く側が SIGPIPE で落ちる）。
logs=$(mktemp)
docker logs "$name" >"$logs" 2>&1
grep -q "callboard .* listen=0.0.0.0:8080 castlist=http://$name-castlist:8080 pagetray=http://$name-pagetray:8080" "$logs"
grep -q 'signal SIGTERM' "$logs"
rm -f "$logs"
# 呼ぶ先を止めても、callboard は生きていると答え、入口は届かないと出す。
docker stop "$name-pagetray" >/dev/null
curl -fsS "$url/healthz" >/dev/null
curl -fsS -o "$tmp/index.html" "$url/"
test "$(grep -cF 'つながっています' "$tmp/index.html")" = 1
test "$(grep -cF '届きません' "$tmp/index.html")" = 1
# メッセージの一覧・新着の知らせの中継・ページの一覧・file を上げる道は 503（中継は、restJson1 の形の Unavailable）。
test "$(curl -s -o "$tmp/messages.html" -w '%{http_code}' "$url/messages")" = 503
grep -qF 'pagetray に聞けません' "$tmp/messages.html"
test "$(curl -s -o "$tmp/pages.html" -w '%{http_code}' "$url/pages/")" = 503
grep -qF 'pagetray に聞けません' "$tmp/pages.html"
test "$(code -X POST -H "Origin: $url" -H 'Content-Type: text/plain' --data-binary "@$tmp/upload.txt" "$url/$up_path")" = 503
test "$(curl -s -D "$tmp/events-head.txt" -o "$tmp/events.json" -w '%{http_code}' --max-time 10 "$url/api/pagetray/events")" = 503
tr -d '\r' <"$tmp/events-head.txt" | grep -qix 'x-amzn-errortype: unavailable'
echo "image は動く: ${image}"
