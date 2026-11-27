# 変えたこと

版ごとに、画面と、呼ぶ先（castlist・pagetray）の要る版と、動かし方（設定・image）が変わったかを書く。release の workflow は、tag の版の節（`## <版>`）が無いと何も出さない（`ci/check-release-version.sh`）。

## 0.1.0

初めの版。castlist と pagetray の画面をまとめるサーバーの土台と、castlist から移した 2 つの画面。

- 画面: 入口（`/`。castlist と pagetray が生きているかと版、それぞれの画面への link）・プロフィール（`/actors/{id}`）・相関図（`/graph/network`）。プロフィールと相関図は、castlist の画面を移した物（[画面](https://uraitakahito.github.io/callboard/ui.html)）。画面の中身（頭の帯の下）の、castlist の画面との違いは、送り手の時刻の ISO の字をミリ秒までにそろえることだけ（比べたのは、プロフィールの中身）。頭の帯は替えた: 画面の間の link（アクター・タイムライン・相関図・ページ・メッセージ）を足し、題の終わりを「callboard」にし、browser の tab の印を足した。
- まだ移していない画面（アクターの一覧・タイムライン・ページの一覧・メッセージ）への link は、castlist・pagetray の画面を指す。
- 呼ぶ先の要る版: castlist 1.0.0 から・pagetray 1.1.1 から（定義を取った版。castlist と pagetray は変えていない）。castlist は、API の `GetHealth`・`GetActor`・`GetLineage`・`GetGraph` を呼ぶ。pagetray は `GetHealth` だけ。
- 守り: castlist・pagetray と同じ（`Host` が `127.0.0.1`・`localhost` だけ・`Origin` の付いた書く要求を断る・時間切れ 5 秒・本文 2 MiB・CSP）。
- 動かし方: image は `ghcr.io/uraitakahito/callboard:v0.1.0`（arm64）。data も volume も持たない。設定は環境変数 5 つで、聞く先の 2 つ（`CALLBOARD_CASTLIST_URL`・`CALLBOARD_PAGETRAY_URL`）は必ず書く（[動かす](https://uraitakahito.github.io/callboard/run.html)）。
