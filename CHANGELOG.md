# 変えたこと

版ごとに、画面と、呼ぶ先（castlist・pagetray）の要る版と、動かし方（設定・image）が変わったかを書く。release の workflow は、tag の版の節（`## <版>`）が無いと何も出さない（`ci/check-release-version.sh`）。

## 0.4.0

pagetray のページの一覧を移した版。castlist と pagetray の画面が、みな callboard で開く（pagetray の画面も、まだ残っている）。

- **呼ぶ先の要る版: pagetray 1.3.0 から**（0.3.0 は 1.2.0 から）。ページの一覧が、1.3.0 で足した操作（`GetPageFacets`・`ListReviews`・`GetFileRules`）を使う。pagetray を先に 1.3.0 に上げてから、callboard を上げる。古い pagetray に向けると、ページの一覧は「pagetray に聞けません（404 が返りました）」になる（ほかの画面は動く）。castlist は 1.1.0 からのまま。
- 画面: ページの一覧（`/pages/`。pagetray の `/` を移した物）。絞り込みの列・待ちの数の釦・探す form・並び・続きの link・返事と判定の印・見た物とレビューの link・送り手・本文の枠・Key の画面の file の枠と上げる枠は、pagetray の画面と同じ（同じ pagetray を相手に比べる試験が通る）。違いは 5 つ:
  - 一覧の道は `/pages/` の 1 つ（pagetray の `/<project>/`・`/<project>/<key>/` の道は無い。`?project=…&key=…` で開く）。
  - 時刻を、callboard が日本時間（UTC+9）で組む。
  - 値の形が違う引数と読めない続きの印は、400 の画面で pagetray の訳を書く（pagetray の画面は、条件が無い物・合う物が無い物として出す）。
  - 送り手の名の link は、callboard のプロフィール（`/actors/<id>`）。
  - 未読の印は、callboard の origin で覚え直す（pagetray の画面で覚えた印は引き継がない）。
- **ページの本文と file の中身は、browser が pagetray から直に読む**（本文の枠の `<iframe>`・file の link と図）。URL の頭は `CALLBOARD_PAGETRAY_LINK_URL`。ページの一覧の答えだけ、CSP に pagetray の link の頭を足す（`frame-src`・`img-src`。`img-src` に、上げる前の図の見本の `blob:`）。ほかの画面の CSP は前のまま。
- **file を上げる道**: `POST /api/pagetray/projects/{project}/keys/{key}/files?name=…`。callboard の初めての書く道。Key の画面の form が送り、callboard が pagetray の `CreateFile` に、役割 `product-manager` で渡す（browser からは名・中身の種類・中身だけを受け、役割・workspace・番号・時刻・送り手は渡さない）。この道だけ、守りを替えた: この callboard の画面の form からの要求（`Origin` が `http://<Host>` と同じで、`Sec-Fetch-Site` が無いか `same-origin`）だけを通し、ほかは `Origin` が無くても 403。本文は 16 MiB まで（ほかの道は 2 MiB のまま）。
- 画面の間の link: 頭の帯と入口の「ページ」・メッセージの一覧の「ページ」が、callboard の `/pages/` を指す。pagetray の画面を指す link は、無くなった。
- メッセージの一覧と、新着の知らせの中継は変わらない（ページの一覧も、同じ中継の道を聞く）。
- 設定は変わらない（4 つ）。`CALLBOARD_PAGETRAY_LINK_URL` は、browser が pagetray のページと file を開く URL の頭として、引き続き使う。

## 0.3.0

pagetray のメッセージの一覧を移した版。pagetray の 2 つの画面のうち 1 つが、callboard で開く。

- **呼ぶ先の要る版: pagetray 1.2.0 から**（0.2.0 までは 1.1.1 から）。メッセージの一覧が、1.2.0 で足した操作（`GetMessageFacets`。絞り込みの候補と件数）を使う。pagetray を先に 1.2.0 に上げてから、callboard を上げる。古い pagetray に向けると、メッセージの一覧は「pagetray に聞けません（404 が返りました）」になる（ほかの画面は動く）。castlist は 1.1.0 からのまま（2.0.0 でも動く）。
- **callboard から pagetray に、container の名で届くこと**が要る（今までは、入口の画面が「届きません」と出すだけだった）。pagetray の container の名は `pagetray.box`（pagetray 1.2.0 の docs から）で、聞く先は `CALLBOARD_PAGETRAY_URL=http://pagetray.box:8080`（[動かす](https://uraitakahito.github.io/callboard/run.html)）。
- 画面: メッセージの一覧（`/messages`。pagetray の `/messages` を移した物）。行・絞り込みの列・「まだ届いていない n 件」・続きの link・長い本文の畳みは、pagetray の画面と同じ（同じ pagetray を相手に、題・数・絞り込みの列・一覧の列を比べる試験が通る）。違いは 3 つ: 時刻を、callboard が日本時間（UTC+9）で組む（pagetray の画面は、script がその機械の時刻に書き替える）。値の形が違う引数と読めない続きの印は、400 の画面で pagetray の訳を書く（pagetray の画面は、条件が無い物として出す）。頭の帯は callboard の物（画面の間の link）。
- 新着の知らせ: pagetray の `GET /api/events`（SSE）を、`GET /api/pagetray/events` でそのまま流す（browser は pagetray に直に聞かない。CSP の `connect-src 'self'`）。メッセージの一覧の script（`/static/messages.js`）が聞き、記録が置かれる・書き足されると、一覧の列と絞り込みの列を、押さずに差し替える。流れには時間切れを付けない。pagetray が閉じれば流れも終わり、browser がつなぎ直す（つなぎ直した時に pagetray が止まっていれば、3 秒ごとに試す）。止める合図（SIGTERM）で、流れを閉じてから終わる。
- 画面の間の link: 頭の帯と入口の「メッセージ」が、callboard の `/messages` を指す。「ページ」（ページの一覧）だけ、まだ pagetray を指す。
- 設定は変わらない（4 つ）。image の中身も同じ（binary 1 つ）。

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
