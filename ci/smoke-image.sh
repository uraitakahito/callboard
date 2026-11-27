#!/usr/bin/env bash
# image が、空の image の中で起き、/healthz に答え、外の Host と browser からの書き込みを断り、
# 入口の画面と CSS が CSP の決まりどおりに出て、castlist と pagetray に名で届き、止める合図で自分から終わり、
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
# 呼ぶ先（出した版。release の本文の digest で固める）。版は、定義を取った tag（ci/gen.sh）と同じにする（下で突き合わせる）。
castlist=ghcr.io/uraitakahito/castlist@sha256:ae9ebbe8371f4f3abd5d4d8121c15aa43ce57d06e0466e61f9e8006c2a20b05d
castlist_version=1.0.0
pagetray=ghcr.io/uraitakahito/pagetray@sha256:1561e7322d050674f42718db2de2f7048d083571d1dfa3dfef936e6959a5cba7
pagetray_version=1.1.1
# 定義を取った版と、ここで起こす image の版がずれていないか（片方だけ上げると、古い image を相手に通ってしまう）。
for pair in "CASTLIST_TAG:$castlist_version" "PAGETRAY_TAG:$pagetray_version"; do
  tag=$(sed -n "s/^${pair%%:*}=v\(.*\)\$/\1/p" "$here/gen.sh")
  if [ "$tag" != "${pair#*:}" ]; then
    echo "NG: ci/gen.sh の ${pair%%:*}（v${tag}）と、ci/smoke-image.sh の版（${pair#*:}）が違う" >&2
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
