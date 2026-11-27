#!/usr/bin/env bash
# 公開する字（README・本の docs/src・画面の templates）に、読む人の機械や、ある人の手元を前提にした言い方が無いかを見る。
# callboard はだれの機械でも動かせる物として公開するので、「この Mac」のように 1 台を指す字や、ある人の home の path を書かない。
# 画面の字は、castlist の画面で Product Manager が決めた字にそろえる（castlist の ci/check-wording.sh と同じ決まり）:
# 鍵と値の組（labels）は「ラベル」（前の呼び方の「札」は書かない）、アクターの状態は「稼働中・停止」、図は「タイムライン・相関図」、
# 送り手の時計のずれは「時刻ずれ」、絞りの枠は「絞り込み」。
# 見る形: 語の Mac（macOS は当てない）・/Users/<名>・札・上の前の字
# 1 つでも当たれば、file と行を出して exit 1。
# 使い方: ci/check-wording.sh [path…]（既定は README.md・docs/src・templates）
set -euo pipefail

if [ "$#" -eq 0 ]; then
  root=$(cd "$(dirname "$0")/.." && pwd)
  set -- "$root/README.md" "$root/docs/src" "$root/templates"
fi

files=$(find "$@" -type f | LC_ALL=C sort)
if [ -z "$files" ]; then
  echo "NG: 見る file が無い: $*" >&2
  exit 1
fi

# shellcheck disable=SC2086
if grep -nE '(^|[^A-Za-z])Mac([^A-Za-z]|$)|/Users/[A-Za-z]|札' $files >&2; then
  echo "NG: 公開する字に、1 台の機械やある人の手元を指す言い方か、前の呼び方「札」がある（上の行）。「callboard を動かす機械」「host」のように、どの機械でも通じる言い方にし、鍵と値の組は「ラベル」と書く" >&2
  exit 1
fi
# shellcheck disable=SC2086
if grep -nE '居るか|居た間|>いる<|>いない<|時間の帯|関係の網|相関図 ?[A-C]|時計のずれた行|絞る（|当たるアクターはいない' $files >&2; then
  echo "NG: 公開する字に、前の画面の字がある（上の行）。状態は「稼働中・停止」「稼働した期間」、図は「タイムライン・相関図」（まとめは「図 B・C」）、ずれは「時刻ずれ」、絞りは「絞り込み」「条件に合うアクターはいません」と書く" >&2
  exit 1
fi
echo "ok: 公開する字に、1 台の機械やある人の手元を指す言い方と「札」と、前の画面の字は無い"
