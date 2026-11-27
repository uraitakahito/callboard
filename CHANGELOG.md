# 変えたこと

版ごとに、画面と、呼ぶ先（castlist・pagetray）の要る版と、動かし方（設定・image）が変わったかを書く。release の workflow は、tag の版の節（`## <版>`）が無いと何も出さない（`ci/check-release-version.sh`）。

## 0.2.0

castlist の残りの 2 つの画面（アクターの一覧・タイムライン）を移した版。castlist の 4 つの画面が、みな callboard で開く。

- **呼ぶ先の要る版: castlist 1.1.0 から**（0.1.0 は 1.0.0 から）。一覧とプロフィールが、1.1.0 で足した行の欄（`presence`・`skewed`）を、タイムラインが 1.1.0 で足した操作（`GetTimeline`）を使う。castlist を先に 1.1.0 に上げてから、callboard を上げる。古い castlist に向けると、タイムラインは「castlist に聞けません（404 が返りました）」になり、一覧とプロフィールの時刻ずれの印と、状態の横の出来事の名が出ない。pagetray は 1.1.1 からのまま。
- 画面: アクターの一覧（`/actors/`。castlist の `/` を移した物。絞り込み・期間の欄・次のページ）とタイムライン（`/graph/timeline`。並べる計算ごと移した）。画面の中身（頭の帯の下）の、castlist の画面との違いは、送り手の時刻の ISO の字をミリ秒までにそろえることと、誤りの画面の戻りの link が「入口へ」なことだけ（比べたのは、一覧・タイムライン・プロフィールの中身と、引数の誤りの訳）。
- 画面の間の link: 頭の帯の「アクター」「タイムライン」と、相関図・プロフィールからの行き来が、callboard の中で閉じる。pagetray の 2 つの画面（ページの一覧・メッセージ）だけ、まだ pagetray を指す。
- プロフィール: 「今の状態」の出来事の名と時刻ずれの印を、辿った先の自分の行の欄から読むようにした（0.1.0 は、アクター 1 つの答えから callboard が出していた。同じ値）。
- 設定: `CALLBOARD_CASTLIST_LINK_URL` を無くした（castlist の画面を指す link が無くなったため）。書いてあっても読まない。ほかの 4 つは変わらない。
- 中継の道（`/api/castlist/graph`）は変わらない。

## 0.1.0

初めの版。castlist と pagetray の画面をまとめるサーバーの土台と、castlist から移した 2 つの画面。

- 画面: 入口（`/`。castlist と pagetray が生きているかと版、それぞれの画面への link）・プロフィール（`/actors/{id}`）・相関図（`/graph/network`）。プロフィールと相関図は、castlist の画面を移した物（[画面](https://uraitakahito.github.io/callboard/ui.html)）。画面の中身（頭の帯の下）の、castlist の画面との違いは、送り手の時刻の ISO の字をミリ秒までにそろえることだけ（比べたのは、プロフィールの中身）。頭の帯は替えた: 画面の間の link（アクター・タイムライン・相関図・ページ・メッセージ）を足し、題の終わりを「callboard」にし、browser の tab の印を足した。
- まだ移していない画面（アクターの一覧・タイムライン・ページの一覧・メッセージ）への link は、castlist・pagetray の画面を指す。
- 呼ぶ先の要る版: castlist 1.0.0 から・pagetray 1.1.1 から（定義を取った版。castlist と pagetray は変えていない）。castlist は、API の `GetHealth`・`GetActor`・`GetLineage`・`GetGraph` を呼ぶ。pagetray は `GetHealth` だけ。
- 守り: castlist・pagetray と同じ（`Host` が `127.0.0.1`・`localhost` だけ・`Origin` の付いた書く要求を断る・時間切れ 5 秒・本文 2 MiB・CSP）。
- 動かし方: image は `ghcr.io/uraitakahito/callboard:v0.1.0`（arm64）。data も volume も持たない。設定は環境変数 5 つで、聞く先の 2 つ（`CALLBOARD_CASTLIST_URL`・`CALLBOARD_PAGETRAY_URL`）は必ず書く（[動かす](https://uraitakahito.github.io/callboard/run.html)）。
