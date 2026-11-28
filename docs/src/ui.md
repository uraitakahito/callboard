# 画面

callboard が HTML を組んで返す画面（askama。`templates/`）。callboard を動かす機械の browser で `http://127.0.0.1:8803/` を開く。

- 字はみな HTML として逃がす（ラベル・名・出来事の `detail` など、送り手が書いた字も）。
- どの画面も、`<head>` で字の差し替えの script（取り込んだ i18next・訳 2 つ・`GET /static/i18n.js`。下の「画面の字の言語」）を読む。そのほかに script を使うのは相関図（`GET /static/network.js` と、取り込んだ cytoscape）・メッセージの一覧（`GET /static/messages.js`）・ページの一覧（`GET /static/pages.js`）だけで、callboard が配る file だけを読む。JavaScript を切っても、相関図のほかは日本語で読める。inline の script は無い。style は callboard が配る CSS（`GET /static/site.css`）だけで、inline の style も無い。どの file も binary に埋め込む。
- どの画面にも Content-Security-Policy を付ける: `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'`。browser は、castlist と pagetray の API に直には聞かない（`connect-src 'self'`）。ページの一覧だけは、pagetray が持つページの本文と file を browser が直に読むので、`img-src` と `frame-src` に pagetray の link の頭を足した物を付ける（下の「ページの一覧」）。
- 時刻は、日本時間（UTC+9）で表示する（castlist の画面は castlist が受け取った時刻、メッセージの一覧は送り手が書いた時刻、ページの一覧は pagetray が受け取った時刻）。ISO 8601 の字（UTC・ミリ秒まで）は `<time datetime>` と `title` に置く（時刻にマウスを重ねると表示される）。
- 明るい色と暗い色の両方を持つ（`prefers-color-scheme`）。
- どの画面の頭にも、画面の間の link（アクター・タイムライン・相関図・使用量・ページ・メッセージ）を出す。

## 移した画面と、まだ移していない画面

castlist と pagetray の画面を、1 つずつ callboard に移した。castlist の 4 つの画面（0.2.0 まで）と、pagetray の 2 つの画面（メッセージの一覧は 0.3.0、ページの一覧は 0.4.0）。castlist は 2.0.0 で、pagetray は 2.0.0 で、自分の画面を外した（表の「castlist・pagetray での道」は、前の道。今は 404）。ページの本文と file の中身は、画面でなく pagetray が持つ物なので、browser が pagetray から直に読む（URL の頭は設定の `CALLBOARD_PAGETRAY_LINK_URL`。[動かす](run.md)）。

| 画面 | 今ある所 | callboard の道 | castlist・pagetray での道（どちらも 2.0.0 で無くなった） |
|---|---|---|---|
| 入口 | callboard | `/` | — |
| アクターの一覧 | callboard（0.2.0 から） | `/actors/` | castlist の `/` |
| プロフィール | callboard（0.1.0 から） | `/actors/{id}` | castlist の `/actors/{id}` |
| タイムライン | callboard（0.2.0 から） | `/graph/timeline` | castlist の `/graph/timeline` |
| 相関図 | callboard（0.1.0 から） | `/graph/network` | castlist の `/graph/network` |
| 使用量 | callboard（0.5.0 から。移した物でなく、callboard で初めて作った画面） | `/usage/` | —（castlist は、2.1.0 で読む API だけを足した） |
| ページの一覧 | callboard（0.4.0 から） | `/pages/` | pagetray の `/`・`/<project>/`・`/<project>/<key>/` |
| メッセージ | callboard（0.3.0 から） | `/messages` | pagetray の `/messages` |

castlist にあった画面との違い（画面の中身。頭の帯の下）は 2 つだけ: 送り手の時刻の ISO の字をミリ秒までにそろえることと、誤りの画面の戻りの link（「一覧へ」でなく「入口へ」）。castlist が画面を外す前（1.1.0）に、同じ castlist を相手に、castlist の画面と callboard の画面の中身を比べて確かめた（CI の browser の試験と、動いている台帳の写し（アクター 114 人。2026-11-27）。比べる相手が無くなったので、その試験は外した）。

## `GET /` 入口

castlist と pagetray が生きているか（「つながっています」と版。聞けなければ「届きません」と訳）と、それぞれの画面への link。呼ぶ先が答えなくても 200 で返す。生きているかは、開くたびに聞く（1 秒まで待つ）。

## 期間の欄

一覧の稼働した期間（`active_from`・`active_to`）と、2 つの図（タイムライン・相関図）の登録した期間（`from`・`to`）と、使用量の期間（`from`・`to`）の欄は、browser の日時の欄（`<input type="datetime-local" step="any">`。script は使わない）。

- browser が送る `YYYY-MM-DDTHH:MM`（`:SS`・`:SS.sss` も）は日本時間（UTC+9）として、RFC 3339（`2026-11-22T11:00:00+09:00`）にしてから、今までと同じ検索をする。URL に RFC 3339 の値（`Z` や別のずれの付いた前の link）を書けば、そのまま効く。どちらでもない値は 400 の画面で、欄の見出しと直し方を書く（「「稼働した期間（から）」の日時を読めません。日時の欄で選ぶか、2026-11-22T13:00 の形で入れてください。」。API の訳「RFC 3339 の時刻」は画面に出さない）。
- 期間を付けて開いた画面では、欄にその期間を日本時間の字（`2026-11-22T11:00`。秒やミリ秒があれば `:07`・`:07.120` まで）で出す。form を送り直しても同じ時刻。`step="any"` は、秒やミリ秒の付いた値でも browser が送るのを断らないため。
- 欄の下に「期間（今の条件のまま、期間だけを替えます）」の link: 直近 1 時間・今日・昨日・直近 7 日。今の画面の URL のまま、期間の 2 つの引数だけを替える（ほかの条件は残し、次のページの `after` は落とす）。期間は callboard が今の時刻から決め、RFC 3339（`+09:00`。秒まで）で URL に置くので、開いた URL をそのまま証拠に貼れる。直近 1 時間・直近 7 日は今まで（今の秒を切り上げる）、今日・昨日は日本時間の 0 時から次の 0 時まで。
- 日本時間の字にするのは画面だけ。castlist の API には、UTC の字（`Z`）にして渡す。

## `GET /actors/` アクターの一覧

アクターを、最初の登場の新しい順に表示する。

| 列 | 中身 |
|---|---|
| 名前 | プロフィールへの link |
| 種類 | 人（Product Manager）にだけ「人」。AI には何も出さない（アクターはほとんど AI なので） |
| 状態 | 稼働中・停止（推しの幕引きで停止した者は「停止（推し）」）と、状態を替える出来事の最新（日本語の名） |
| 課題の Key・役割 | ラベル `tpm.key`・`tpm.role` |
| model · effort | 最新の model と effort |
| 最初の登場・最後の退場 | 受け取った時刻 |
| 時刻ずれ | アクター・出来事・model と effort のどれかに、送り手の時計がずれた行がある |
| 額・いちばん長い文脈 | 一覧の期間の、その席の額（本体と手伝いの計）と、いちばん長い文脈（本体の回）。数の無い席は「—」（下の「額の列」） |

絞り込みは、castlist の監査の検索 `ListActors`（`GET /api/actors`）と同じ条件を、画面の引数の名（snake_case。`activeFrom` は `active_from`、`launchedBy` は `launched_by`、`maxResults` は `limit`、`nextToken` は `after`）で URL に置く。引数は callboard が確かめ（`crates/server/src/ui/list.rs`・`ui/params.rs`）、確かめた物を castlist の API に渡す。form には `label`（何度でも）・`active`（状態: すべて・稼働中・停止）・`kind`（種類: すべて・人。人を探す道として残す。AI だけに絞る選び肢は無いが、URL の `kind=ai` は今と同じく AI だけに絞る）・`model`・`effort`・`active_from`・`active_to`（稼働した期間（から）・（まで）。上の「期間の欄」）・`skewed`（時刻ずれあり）の欄がある。ほかの引数（`binding`・`at`・`launched_by`・`sort`・`order`・`limit`）も URL に書けば効き、form を送り直しても残る。

- 絞り込みは、すべての条件に合うアクターを表示する（AND）。枠の見出しは「絞り込み」だけで、この訳は画面に書かない。
- form は空の欄も送るので、空の値は付けなかったのと同じにする。
- `order` が無ければ `desc`（新しい順）、`limit` が無ければ 100。
- 条件が無ければ、`active_from` を今から 7 日前にして表示し、そう書く（「この 7 日に稼働したアクター」。castlist の API は条件の無い検索を断るので）。登場の記録の無いアクターは、この既定では表示されない。ラベルなどで絞り込むと表示される。
- 次のページは「次のページ」の link（`rel="next"`）。条件はそのままで、検索の次の頁の印を `after` に付ける。
- 引数の誤り（知らない名・1 つだけの名の重なり・読めない値）は、引数の名を書いた 400 の画面（期間の欄の読めない値だけは、[期間の欄](#期間の欄)の字）。castlist には聞かない。
- 行の「状態」の横の出来事の名と、「時刻ずれ」の印は、castlist 1.1.0 で足した欄（`presence`・`skewed`）から出す。
- castlist に聞けない時は 503 の画面で、訳を書く。

### 額の列

一覧の行の「額」と「いちばん長い文脈」は、castlist の `ListUsage`（castlist 2.1.0 から）を 1 度（頁はみな引く）聞いて、callboard が席ごとに足した物。一覧の問い（`ListActors`）と一緒に聞く。

- **期間**: 一覧の期間（`active_from`・`active_to`。条件が無い時の既定は、今から 7 日前から）を、[使用量](#get-usage-使用量)と同じく時の境に揃えた物。終わりが無ければ今まで、始めが無ければ、揃えた終わりの 7 日前から。ラベル（`label`）も同じ物で絞る。表の下に、揃えた期間を書く。
- **中身**: 額は本体と手伝い（subagent）の回の計。いちばん長い文脈は本体の回だけ（同じ時の行が 2 つ以上あれば、大きい方）。数の行の無い席は「—」。
- 揃えた期間が 31 日を越える時は、castlist に数を聞かずに列を「—」にし、表の下に 1 行の注を書く。終わりが始めより後でない期間も、数は聞かない。
- 数を聞けない時（castlist が 2.1.0 より古い 404・503・時間切れ）と、記録が多すぎて全部を読めなかった時は、列を「—」にし、表の下に訳を書く。**一覧そのものは、今までどおり出す**（503 にしない）。
- **一覧を遅らせない**: 一覧は人がいちばん開く画面なので、数は 1 秒だけ待つ（使用量の画面とアクターの頁は、castlist の口の期限のまま）。過ぎたら列を「—」にし、「時間内に読めませんでした」と書く。
- 表の下の注は「額を数えた期間」と、揃えた期間の日時（一覧に下の端が無い時も、額は揃えた終わりの 7 日前からの分）。
- 並べ替え（`sort`）は castlist の一覧の作りに依るので、額では並べない。

## `GET /actors/{id}` プロフィール

castlist の API の 2 つの答えから組む: アクター 1 つ（`GetActor`）と、起動した・引き継いだ線を辿った先（`GetLineage`。上は人の起点まで、下は 1 段）。

| 節 | 中身 |
|---|---|
| 頭 | 名前・種類・id・発行元（issuer）・登録した時刻 |
| 今の状態 | 状態（稼働中・停止・停止（推し）と、最新の出来事の名）・最新の model と effort（指定・実際 と受け取った時刻）・最初の登場・最後の退場・時刻ずれの印 |
| ラベル | ラベルの全部（鍵の順） |
| 外部の id | 受け取った順の全部（履歴）。キーの物に「キー」、種類ごとの最新に「今」 |
| 起動した人 | 起動した（`launched`）・引き継がせた（`continued`）線を上り、人の起点までの鎖（起点から順）。両方あれば起動した方を辿る |
| 起動したアクター | このアクターが `launched` で起動したアクター |
| 引き継ぎ | `continued` で、このアクターに引き継がせたアクター（引き継ぎ元）と、このアクターから引き継いだアクター（引き継ぎ先） |
| model と effort の履歴 | 受け取った時刻・model・effort・指定 / 実際（`requested` / `observed`）。実際の値のうち、その前に最後に指定した値と model か effort が違う物に「食い違い」（castlist の監査の検索の `mismatch=true` と同じ決め方） |
| 使用量（直近 31 日） | そのアクターの額（本体と手伝いの計と、それぞれ）・回の数（手伝いの回の数）・いちばん長い文脈（本体）と、文脈の長さの段々の線（下の「節「使用量」」） |
| 出来事 | 受け取った時刻・出来事の日本語の名（字に重ねると id）・理由（`detail.reason`）・きっかけ（`detail.source`）・ほかの `detail`（鍵の順の JSON の字）・送り手の時刻・時刻ずれの印 |

- 種類の印は、人（Product Manager）にだけ「人」。AI には何も出さない（アクターはほとんど AI なので）。
- 無い id と、id の形（小文字の UUID の字）でない物は 404 の画面。形でない物は、castlist に聞かない。
- castlist に聞けない時は 503 の画面で、訳（つながりません・時間内に答えがありません・`<状態>` が返りました・答えを読めません）を書く。聞く先の URL と、castlist の誤りの字は出さない（記録に残る。[動かす](run.md) の「確かめる」）。
- 無い物は「ありません。」（起動した人が無い人の起点は「ありません（起点）。」）と書く。
- 下に「表示した時刻」（castlist がアクターを読み始めた時刻）を書く。
- 2 つの答えは別々の読みなので、間に記録が足されると、片方にだけ出ることがある。名を引けないアクターは、id のまま出す。

### 節「使用量」

castlist の `ListUsage` を、アクターの 2 つの問いと一緒に、3 つ目として聞く（`actorId` で、そのアクターの行だけ。期間は、今を時の境に切り上げた時刻までの 31 日）。

- **3 つ目が落ちても、頁は出す**: 数の問いが落ちた時（castlist が 2.1.0 より古い 404・503・時間切れ）は、この節だけを「数を読めません（訳）。」にする。アクターの 2 つの問いのどちらかが落ちた時は、今までどおり頁ごと 503 か 404。
- 行が無ければ「この 31 日に、数の記録はありません。」。
- **文脈の長さの段々の線**: 横は時刻（日本時間。目盛りはタイムラインと同じ決め方）、縦は文脈の長さ。1 時間（UTC の時。行の区間の終わりの時刻 `toAt` で決める）ごとの、本体の回のいちばん長い文脈を、その時の初めから終わりまでの横の線にし、続く時は縦の線でつなぐ。記録の無い時は線を切る。20 万に横の点線（縦の目盛りは、いつも 20 万を含める）。横の幅は、数の記録の在る時の内だけ（その外の圧縮の印は描かない）。時にマウスを重ねると、時と値。横の範囲は、記録の在る初めの時から終わりの時まで（圧縮の印がその外なら、印まで延ばす）。本体の回が無ければ、線の図は出さない。
- **圧縮の印**: このアクターの出来事のうち、期間の内の要約（`compacted`）の受け取った時刻に、縦の点線。マウスを重ねると、出来事の名と時刻と、`detail` に `preTokens`・`postTokens` が在れば「（18 万 → 3 万）」の形で前と後の文脈の長さ。出来事は、頁がもう読んでいる `GetActor` の答えから取る。
- 図の作りは、使用量の画面の図と同じ（下の「図」）。

画面の中身（頭の帯の下）の、castlist の画面との違いは 1 つ: 送り手の時刻の ISO の字を、ミリ秒までにそろえる（castlist の画面は、送られた字のまま出す。同じ時刻）。頭の帯（題・画面の間の link・tab の印）は、callboard の物。

「今の状態」の出来事の名と時刻ずれの印は、辿った先の自分の行の欄（`presence`・`skewed`。castlist 1.1.0 から）から読む（一覧の行と同じ値）。

## `GET /graph/timeline` タイムライン

アクターが稼働していた期間を、横を時刻にした帯で表示する。callboard が、castlist の `GetTimeline`（`GET /api/timeline`。castlist 1.1.0 から）の答えから SVG を組んでページに埋め込む（script は使わない）。

- 絞りは castlist の `GetTimeline` と同じ引数（`label`・`from`・`to`。どれも 1 つだけ。`from`・`to` は「期間の欄」）。絞った組に触れる線と、その線の向こうの端のアクター（一歩だけ）も出す（人の根が切れないように）。form は空の欄も送るので、空の値は付けなかったのと同じにする。条件が無ければ、全部のアクターを出す。
- 引数の誤りは、引数の名を書いた 400 の画面（期間の欄の読めない値だけは、[期間の欄](#期間の欄)の字）。条件に合うアクターがいなければ、SVG を出さずにそう書く（「条件に合うアクターはいません。」）。
- 横は castlist が受け取った時刻（`received_at`）を日本時間で。`from`・`to`（登録した期間）を付ければ、横もその期間にする（期間の外は切る）。無ければ、稼働した期間・印・線の時刻の初めから終わりまで（稼働中のアクターがいれば今まで。1 分より短ければ 1 分に延ばす）。

| 物 | 描き方 |
|---|---|
| 行 | 1 つのアクターに 1 行。人を上に、ほかは最初の登場の受け取った時刻の順（登場の無いアクターは登録した時刻）。左に名（プロフィールへの link）と、人には「人」 |
| 帯 | 稼働にする出来事（`entered`・`reentered`・`forked`）から、停止にする出来事（`cleared`・`exited`・`closed`）まで（どれも seq の順。稼働中の登場・停止中の退場は何もしない）。停止の後にまた来れば、帯は別になる。稼働中のままなら今（か表示する期間の右端）まで描き、右端を尖らせる（「まだ稼働中」）。期間の左端で切った帯は、左端を尖らせる |
| 推しの帯 | 推しの幕引き（`closed` で `detail.inferred` が JSON の true）で終わる帯は、最後の便り（その行の稼働・停止の出来事・印・model と effort の替わり目の最新。終わりの出来事そのものは数えない）で切り、そこから終わりまでを点線の縁と薄い塗りで描く（class `inferred`）。終わりは roster が気づいた時刻（受け取った時刻）で、止まった本当の時刻ではない。マウスを重ねると、終わりの時刻の後に訳を「（推し: 居場所が消えた）」（`detail.reason` が `workspace_gone`）・「（推し: 居場所に別の session が入った）」（`superseded`）・「（推し: roster が落ちたと推した）」（ほか）の形で表示する。推しの幕引きが表示する期間の右端より後なら、右端を尖らせ「続く（表示する期間の後に推しの幕引き: …）」と表示する。帯の字は、推しの切れで切っても 1 度だけ（続く切れを合わせた幅に収まる時に、初めの切れに）書く。どの帯が推しで終わったかは、一覧などの「停止（推し）」と同じ決め方（castlist の `state.inferred`）。凡例に「点線の帯」 |
| 帯の切れ | model と effort の替わり目（`settings` の行を seq の順に。送り手の時刻 `sent_at` は見ない）で切る。同じ値が続けば切らない。登場の前に指定した物は、帯の初めから効く。切れに `opus-5-5 · high` の形の字を、幅に収まる時だけ書く（はみ出さない。字の幅は字の数から見積もる: ASCII は字の大きさの 0.62、ほかは 1 字分）。矢印の線が切れを通る所は避け（線から 3 px 空ける）、線の横の空いた所に書く。空いた所に収まらなければ書かない。マウスを重ねると（SVG の `<title>`）、名前・値・指定 / 実際（実際の値が、その前に指定した値と違えば「指定と違う」）・初めと終わりの時刻を表示する |
| 切れの色 | model の名で決める class（`m-opus`・`m-sonnet`・`m-haiku`。ほかの model はみな `m-other`。記録が無ければ `m-none`）。凡例に、出てくる model を並べる |
| 実線の矢印 | 起動した（`launched`）。起動したアクターの行の、線を受け取った時刻から、起動されたアクターの最初の帯の始まりへ |
| 点線の矢印 | 引き継いだ（`continued`）。引き継ぎ元の帯の終わり（引き継ぎ先の始まりより前の、最後の物）から、引き継ぎ先の最初の帯の始まりへ |
| 印 | 帯を縦に切る短い線。要約（`compacted`）・名乗り（`reported_in`）・model 不明（`model_unreadable`）。マウスを重ねると、出来事の名と時刻を表示する |
| 目盛り | 表示する期間の長さで、10 秒〜1 年の切りのよい間（日本時間で揃える）を選ぶ。目盛りの間は 100 px より狭くしない。字は `時:分`（秒の間なら `時:分:秒`）で、初めと日が替わった目盛りに `月-日` を前に付ける。1 日より長い間は `月-日` |
| 今 | 表示する期間の中なら、今（表示した時刻。castlist が読み始めた時刻）に点線の縦の線 |

並べ方（`crates/server/src/ui/timeline_layout.rs`。castlist の `timeline.rs` をそのまま移した、外に触らない純な計算で、単体の試験を持つ）は、行ごとの稼働・停止の出来事と替わり目と印の時刻（ミリ秒）、線、期間を受けて、帯の切れ・矢印・印・目盛りの置き場を返す。同じ入力からは、いつも同じ図。並べる計算と HTML を組むのは `tokio::task::spawn_blocking` の糸で行う。色は `site.css` の class で決める（明るい色と暗い色）。矢印は帯の下に、帯の字は印の上に（縁取りを付けて）描く。

- castlist に聞けない時は 503 の画面で、訳を書く。castlist が 1.1.0 より古いと、「castlist に聞けません（404 が返りました）」になる。
- 答えの大きさ: castlist は頁を分けずに、組のアクターの出来事の全部を返す。測った値（release の build。手元の機械）: 動いている台帳の写し（アクター 108 人。2026-11-27）で、castlist の答えは 257 KB。アクター 500 人の見本（1 人に出来事 4〜5 つと model と effort 2 行・線 499 本）で、castlist の答えは 869 KB・17 ms、callboard の画面は 504 KB・26 ms。callboard が castlist を待つ 4 秒には遠い。もっと広い組は、ラベルか期間で絞る。

## `GET /usage/` 使用量

席（アクター = Claude Code の session）が使ったトークンの数と額を、全体・役割ごと・課題ごとの表と、2 つの図（文脈の長さの段ごとの額・日ごとか時ごとの額）で表示する。castlist の `ListUsage`（`GET /api/usage`。castlist 2.1.0 から）の行を、期間の内でみな読み、callboard がまとめる（castlist は集計しない）。script は使わない。

- **行**: castlist の 1 行は、アクター 1 人の、時計の 1 時間（UTC の時）の内の、同じ種類（model・手伝い（subagent）の種類・速さ・service tier）の回のまとめ。同じアクター・同じ時・同じ種類の行は 2 つ以上在りうるので、数は足し、いちばん長い文脈は大きい方を取り、文脈の長さの段は番号ごとに足す。
- **引数**: `label`（`鍵:値`。何度でも。重ねれば、どれも持つアクターの行）・`from`・`to`（期間。「期間の欄」と同じ欄と、期間をすぐ選ぶ link）。form は空の欄も送るので、空の値は付けなかったのと同じにする。
- **ラベルの欄**（t-nd3w）: ラベルを覚えていなくても選べるように、form は「役割」（`tpm.role`）と「課題」（`tpm.subject`）を選ぶ一覧（`<select>`）にし、ほかのラベルは「ほかのラベル」の欄（`<input list>`）に打つ。一覧の値も欄の値も、引数の名は `label`（値は `鍵:値`。一覧の「すべて」は空の値）なので、URL と castlist への渡し方は欄だけの時と同じ。
  - **候補**: 表示と同じ期間（揃えた後）に稼働した席を、castlist の `ListActors`（`activeFrom`・`activeTo`・`sort=entered`・`order=desc`・`maxResults=1000`。ラベルの絞りは付けない）で、`ListUsage` と並べて聞き、席のラベルから作る。絞っても候補は減らない。1 頁だけ読むので、期間に稼働した席が 1,000 を越えると、古く登場した席のラベルは候補に出ない（log に 1 行書く）。一覧の option は「software-engineer（72 席）」、欄の候補（`<datalist>`）は値と「tpm.key:t-nd3w（1 席）」（字にも値を書く。Firefox は候補に値の代わりに字を出すため）。並びは席の多い順、同じなら字の順。`tpm.launch`（起こした回の id）は候補にしない。作る計算は `crates/server/src/ui/usage_labels.rs`（外に触らない純な計算で、単体の試験を持つ）。
  - **今の値**: URL の `label` のうち、初めの `tpm.role:…` を役割の一覧で、初めの `tpm.subject:…` を課題の一覧で選んでおき、残りは「ほかのラベル」の欄に URL の順に出す（その後に空の欄を 1 つ）。一覧で選んだ値が候補に無ければ（期間の外の席など）、席の数を付けずに一覧に足す（送り直しても条件が消えない）。
  - **待つ長さ**: 候補は 1 秒（`CANDIDATE_TIMEOUT`。一覧の額の列と同じ）で切る。候補は飾りで、使用量の数は `ListUsage` だけで決まるので、候補のために画面を待たせない。候補を引けない時（castlist が `ListActors` に答えない・断った・1 秒を過ぎた）も、使用量の画面は出す（一覧は今の値だけ、欄は候補無し。log に 1 行）。
  - **課題の一覧は `tpm.subject` だけ**: 「課題ごと」の表は `tpm.subject` が無ければ `tpm.key` で分けるが、課題の一覧は `tpm.subject` だけから作る（絞りはラベルのどれも持つ席なので、「`tpm.subject` か `tpm.key`」をまとめて選ぶ形は作れない）。`tpm.key` だけの席（表の「devops」など）は、「ほかのラベル」の欄で `tpm.key:…` を選ぶ。
  - 候補の数は、稼働した席の数で、使用量の行の無い席も入る。そのラベルを選ぶと、表が空になることがある。アクターの一覧（`/actors/`）の「ラベル」の欄は、手で打つ欄のまま。
- **期間**: 時の境に揃えてから castlist に送る（始めは時の初めに切り下げ、終わりは時の境に切り上げる。日本時間と UTC は、時の境が同じ）。行が期間に入るかは、行の終わりの時刻で決まるので、時の途中で切ると、端の 1 時間の行が欠けたり余ったりするため。**送った期間（揃えた後）を、日本時間で画面に書く**（「集めた期間」）。form の欄には、入れたとおりの値を出す。
  - 終わりが無ければ今まで、始めが無ければ、揃えた終わりの 7 日前から（どちらも無ければ、直近 7 日。そう書く）。
  - 揃えた後で 31 日を越える期間（castlist の上限）と、終わりが始めより後でない期間は、400 の画面。
- **本体と手伝い**: 本体の回（castlist の行に `agent` が無い）と、手伝い（subagent）の回（`agent` が在る）を分ける。文脈の長さの数（中央・上から 1 割・いちばん長い文脈・20 万以上の回）は、本体の回だけで出す（手伝いは自分の短い文脈で動くので、混ぜると中央が下がる）。

| 節 | 中身 |
|---|---|
| 全体 | 席（アクター）の数・回の数・額（本体と手伝いの計）・手伝いの回の数と額（全体の額に対する比） |
| 役割ごと | アクターのラベル `tpm.role`（無ければ「（無し）」）ごとに: 席の数・回の数（本体）・額（本体）・全体の額に対する比・文脈の長さの中央・上から 1 割・いちばん長い文脈・20 万以上の回の比・20 万以上の回の額の比・手伝いの額。本体の額の大きい順 |
| 課題ごと | ラベル `tpm.subject`（無ければ `tpm.key`。どちらも無ければ「（無し）」）ごとに: 席の数・回の数と額（本体と手伝いの計）・いちばん長い文脈（本体）。額の大きい順 |

- **中央と上から 1 割**: castlist の行は、回ごとの文脈の長さでなく、2.5 万刻みの段（段 n は n×25,000 以上 (n+1)×25,000 未満。40 は 100 万以上）ごとの回の数を持つ。本体の回の段を足し合わせ、回を文脈の短い順に並べた時の、真ん中（下から 5 割。順位は切り上げ）と、上から 1 割（下から 9 割）の回が入る段を取り、**その段の下の端**を「22.5 万〜」の形で出す（近い値。幅は 2.5 万）。
- **20 万以上の回**: 段 8（20 万以上）から上の段の、回の数と額の比（本体の回の段の計に対して）。ちょうど 20 万の回も入る。
- **数の字**: 額は 100 万分の 1 ドルの整数で足し、出す時にドルの小数 2 桁（四捨五入）。比は % の小数 1 桁（元が 0 なら「—」）。トークンは「万」の単位（`22.5 万`・`97 万`）。本体の回が無い行の、文脈の長さの欄は「—」。
- 表の下に注を書く: 額は、使った量を API の値段に直した目安で、払う額ではないこと・役割と課題を分けるラベル・文脈の長さは本体の回だけで出すこと・中央と上から 1 割は段から出した近い値なこと・稼働中の席の数は、長くて 1 時間ほど遅れて出ること（送り手が、時ごとにまとめて送るため）。
- 行が 1 つも無ければ、表を出さずにそう書く（「この期間に、数の記録はありません。」）。
- **行が多い時**: 1 度に 1,000 行ずつ、50 頁（5 万行）まで読む。まだ続きが在れば、誤りにせず、読めた分の数を出して「記録が多すぎて、全部を読めませんでした」と期間を狭めるように書く。全部の頁を読み終えるまでの時間は 4 秒（`upstream::TIMEOUT`）。初めの頁が時間内に来なければ 503 の画面（「時間内に答えがありません」）。2 頁目より後で時間を過ぎたら、頁の上限の時と同じく、読めた分の数を出して「時間内に、全部を読めませんでした」と書く。2 頁目より後で castlist が頁の印を断った時も、人には直せないので 503 の画面（「次の頁を断られました」）。
- 引数の誤り（知らない名・`from` と `to` の重なり・`label` の形・読めない日時・31 日を越える期間・始めだけを先の時刻にした）は、訳を書いた 400 の画面。castlist には聞かない。
- castlist に聞けない時は 503 の画面で、訳を書く。castlist が 2.1.0 より古いと、「castlist に聞けません（404 が返りました）」になる（ほかの画面は動く）。

まとめる計算（`crates/server/src/ui/usage_summary.rs`）は、外に触らない純な計算で、単体の試験を持つ。まとめる計算と HTML を組むのは、タイムラインと同じく `tokio::task::spawn_blocking` の糸で行う。

### 図

「全体」の下、表の上に 2 つ。行が 1 つも無ければ、表と同じく出さない。

| 図 | 中身 |
|---|---|
| 文脈の長さの段ごとの額（本体の回） | 横に段（0〜40 の 41 本の枠）、縦にその段の額の棒。本体の回だけ（手伝いは混ぜない）。0 の段の棒は描かない。横の目盛りは段 4 つごと（「0・10 万・20 万・…・90 万・100 万〜」。段の下の端）。20 万（段 8 の下の端）に縦の点線と字「20 万」。棒にマウスを重ねると「22.5 万〜25 万: $0.50（2 回）」の形で段と額と回の数。本体の回が無い時は、図を出さずに「この期間に、本体の回の記録はありません。」 |
| 日ごとの額・時ごとの額（日本時間） | 横に日（日本時間の 0 時で区切る。初めの区切りは、集めた期間の初めから）、縦に額。本体の棒の上に手伝いの棒を積む（2 色）。集めた期間（揃えた後）が 1 日（24 時間）までなら、時ごとにする（「今日」「昨日」「直近 1 時間」は時ごと、「直近 7 日」は日ごと）。行の日と時は、区間の終わりの時刻（`toAt`）で決める。棒にマウスを重ねると「11-22: 本体 $1.50（本体と手伝いの計 $1.75）」の形。横の目盛りの字は、字の間が 48 px より狭くならないように間引く。手伝いの棒は、本体の棒の上の端から積む（本体の棒が 1 px に満たず 1 px で描いた時も、覆わない）。図の下に、額の在る区切りごとの額（本体・手伝い・計）の字の表を畳んで置く（読み上げのため。棒の `<title>` は読み上げに出ない） |

- **作り**: サーバーで座標を出し、template が `<svg>` の中に `rect`・`line`・`path`・`text` を並べる（タイムラインと同じ。script は使わない）。座標の計算（縦の目盛り・横の枠・棒の高さ・日と時の区切り・段々の線の path）は `crates/server/src/ui/usage_chart.rs`（外に触らない純な計算で、単体の試験を持つ）。図の SVG は `templates/macros.html` の `chart` で、使用量の画面とプロフィールが同じ物を使う。
- **大きさ**: 幅 720 px・高さ 220 px の決まった大きさ（左に 64 px の縦の目盛りの字、下に 28 px の横の目盛りの字）。狭い画面では、図ごと横に送る。
- **縦の目盛り**: 0 から、1・2・5 × 10 の冪の刻みで 4 つほど（刻みはいちばん大きい値の 4 分の 1 以上のいちばん小さい物。額は 1 セントより、文脈の長さは 1 万より細かくしない）。いちばん上は、いちばん大きい値を刻みに切り上げた物。字は、刻みに合わせて `$2`・`$0.5`・`$0.05`、文脈の長さは「10 万」。0 でない小さい値の棒も、1 px の高さにする（見えるように）。
- **字と色**: 字は SVG の `text`。図に `role="img"` と、図の中身と計を書いた `aria-label`。マウスを重ねた時の字は SVG の `<title>`。色は `site.css` の変数と class（本体 `--chart-main`・手伝い `--chart-helper`。タイムラインの `--m-opus`・`--m-sonnet` を借りるので、明るい色と暗い色の両方で替わる。点線は `--warn`）。CSP は替えない（`style=""` を使わない）。

## `GET /graph/network` 相関図

誰が誰を起動し、誰が誰から引き継いだかを、点と線の相関図で表示する。script を使う唯一の画面。

- 絞りは castlist の `GetGraph`（`GET /api/graph`）と同じ引数（`label`・`from`・`to`。どれも 1 つだけ。`from`・`to` は「期間の欄」）。form は空の欄も送るので、空の値は付けなかったのと同じにする。
- callboard は引数を確かめるだけで（誤りは、引数の名を書いた 400 の画面）、点と線は描かない。同じ絞りの中継の道（下の `GET /api/castlist/graph`）の URL を、絵の場所の `data-src` に置く（期間は UTC・ミリ秒までの字に直す。castlist の API は時差の付いた時刻を受けないため）。
- `static/network.js` が、その URL から点と線を引き、cytoscape で canvas に描く。字は DOM の `textContent` でだけ置く。
- JavaScript を切った browser には、同じ条件のタイムラインを勧める字を表示する（`<noscript>`）。

| 物 | 描き方 |
|---|---|
| 点 | 1 つのアクター。名前を下に書く。AI は丸、人は角を丸めた四角で縁を緑に |
| 点の色 | 最新の model の系統（`opus`・`sonnet`・`haiku`・ほか・記録なし）。色は `site.css` の `--m-opus` などを読むので、明るい色と暗い色の両方で替わる |
| 薄い点 | 停止したアクター（推しの幕引きで停止した者も同じ） |
| 実線の矢印 | 起動した（`launched`） |
| 点線の矢印 | 引き継いだ（`continued`。/clear の交代・fork の分身） |

- 並べ方は cytoscape の `cose`。初めの置き場は答えの順に円に置くので、同じ答えからは同じ図。点はドラッグで動かせる。
- 点を押すと、そのアクターのプロフィールを表示する。名前にマウスを重ねると、名前・種類・model · effort・状態を表示する。
- 「表示する点を選ぶ」の model（短い名）と effort は、ページの中だけで、合わない点と、その点に付く線を隠す（聞き直さない）。
- 絵の下に、アクターと線の数と、castlist が答えた時刻（日本時間）を書く。条件に合うアクターがいなければ、そう書く。castlist に聞けなければ「相関図を表示できません: 503 castlist に聞けません（…）」と書く。

### `GET /api/castlist/graph` 図の点と線（中継）

castlist の `GetGraph`（`GET /api/graph`）の答えの、状態・型・本文を返す道（断った答えの本文には `code`・`values` を足す）。相関図の script が使う（browser は castlist に直には聞かない）。

- 渡すのは、問い合わせの引数の字だけ（castlist が確かめる）。browser の header（`Cookie`・`Referer` など）は渡さず、`Host: 127.0.0.1` で聞く。
- 返すのは、castlist の答えの状態・本文の型・誤りの型の名（`X-Amzn-Errortype`）・本文。castlist の誤り（400 など）は、状態・型・本文を返す（断った答えの本文には `code`・`values` を足す。下の「画面の字の言語」の「誤りの JSON」）。
- castlist に聞けない時（つながらない・答えを読み終えるまで 4 秒）は、503 `Unavailable`（`{"message":"castlist に聞けません（…）","code":"errors.unreachable","values":{…}}`。下の「画面の字の言語」）。castlist が断った答えには、`code`（型の名の鍵）と `values`（`detail` に castlist の訳）を足す。
- 読むだけ（`GET`・`HEAD`）。castlist のほかの道は中継しない。

CSP を緩めずに cytoscape を使うための、2 つの決まり（`static/network.js`）:

- cytoscape は、初めに `<style>` を head に差し込もうとする（CSP の `style-src 'self'` が断り、違反になる）。同じ id（`__________cytoscape_stylesheet`）の印を先に置くと差し込まないので、そうする。差し込むはずだった決まり（容れ物を `position: relative` に）は `site.css` に書く。
- cytoscape は、絵の画面の中の位置を覚えておき、押した所の点を探す。覚え直すのは window の scroll と大きさの替わりの知らせが来た時だけなので、指が入った時と押す前に覚え直させる（`cy.resize()`）。

### 取り込んだ外の JavaScript

- [cytoscape](https://js.cytoscape.org/) 3.35.0（MIT。The Cytoscape Consortium）。相関図が使う。npm の tarball の `package/dist/cytoscape.min.js` を、そのまま `static/vendor/cytoscape-3.35.0.min.js` に置く。license の写しは `static/vendor/cytoscape-LICENSE`。
- [i18next](https://www.i18next.com/) 27.2.9（MIT）。画面の字を言語ごとに差し替える（AI-50）。npm の tarball の `package/dist/umd/i18next.min.js` を、そのまま `static/vendor/i18next-27.2.9.min.js` に置く。license の写しは `static/vendor/i18next-LICENSE`。
- どちらも callboard が binary に埋め込んで配る（CDN からは読まない）。出どころの記録は `static/vendor/SOURCES.md`（package ごとに 1 節）。`ci/check-vendor.sh`（CI の job `test`）が、節ごとに、npm の registry の物と、commit した file とを突き合わせる（package ごとに書き替えた写しを当てること・2 つ目の節の壊れた記録を当てること・表を書き損じた節を当てることも、落ちた訳まで CI で見る）。表を書き損じた節・同じ package の節が 2 つ・どの節にも名指されない `static/vendor/` の file も落とす。

版の上げ方（`<名>` は cytoscape か i18next）:

1. 新しい版が npm の `latest` で、公開から 2 日たっていることを見る（`curl -s https://registry.npmjs.org/<名> | jq '."dist-tags", .time["<版>"]'`）。
2. 空の folder で tarball を落とし、SOURCES.md の節の `path` の file を `static/vendor/<名>-<版>.min.js` に、`package/LICENSE` を `static/vendor/<名>-LICENSE` に写す。古い版の file は消す。
3. `static/vendor/SOURCES.md` のその節の version・tarball・integrity・file・sha256 を書き替える。
4. 版の入った path を替える: `crates/server/src/ui.rs` の `<名>` の `…_JS`・`…_PATH`、試験（cytoscape は `crates/server/tests/network.rs`、i18next は `crates/server/tests/i18n.rs`）、`ci/smoke-image.sh`。
5. `ci/check-vendor.sh`・`cargo test --locked`・`cd e2e && pnpm test`（console の誤りと CSP の違反が 0 か）を流す。

## 出来事の名

id は英語のまま（castlist の API）。名は画面だけで、字は訳の file（`static/locales/ja.json`・`en.json`）の `events` の 1 か所に置く（id と鍵の組は `crates/server/src/ui/profile.rs` の `EVENT_NAMES`）。知らない id は、id のまま出す。

| id | 名 |
|---|---|
| `entered` | 登場 |
| `reentered` | 再登場 |
| `forked` | 分身 |
| `compacted` | 要約 |
| `cleared` | 交代 |
| `exited` | 退場 |
| `closed` | 幕引き |
| `reported_in` | 名乗り |
| `model_unreadable` | model 不明 |
| `retracted` | 取り消し |

## 画面の字の言語

画面の字は、日本語と英語で出す（AI-34・AI-50）。字は `static/locales/ja.json`・`en.json`（[i18next](https://www.i18next.com/) の JSON v4）の 1 か所に置き、画面は鍵で引く。

- **サーバーは日本語の字だけを組む。** 英語の字は組まない（英語の字は、ブラウザーが同じ鍵と値で組む）。 template は字を書く要素に鍵を付け（`<a href="/actors/" data-i18n="nav.actors">{{ "nav.actors"|t }}</a>`）、filter `t`（`crates/server/src/ui/i18n.rs`）が `ja.json` から日本語の字を引く。JavaScript を切っても日本語で読め、日本語の人には開いた時の字の空きが無い。属性は `data-i18n-attr="aria-label:<鍵>;title:<鍵>"`、差し込む値は `data-i18n-options='{"n":3}'`。
- **英語はブラウザーで差し替える。** どの画面も `<head>` で、取り込んだ i18next・`/static/locales/ja.js`・`/static/locales/en.js`・`/static/i18n.js` の順に、`defer` を付けずに読む。訳の中身を同期の script で読むので、i18next は同期で起き、本文の script は起きた時から `window.callboardT(鍵, 値)` で字を引ける。
- **言語の選び**: cookie `callboard-lang`（`ja` か `en` のほかの値は無い物とみなす）→ ブラウザーの言語の並びの初めの 1 つ（`en-US` は `en` に寄せる）→ 日本語。`ja`・`en` のほかは日本語。頭の帯の右の「日本語 / English」で選び直せ、選んだ物は cookie（1 年・`SameSite=Lax`）に残る。切り替えは JavaScript が出す（JavaScript が無い時は出さない）。選んだ言語の訳が届かなければ日本語のまま。i18next が届かない時も日本語のままで、`callboardT` は日本語の字を返す（本文の script が止まらないように、`i18n.js` が i18next を起こす前に出す仮の物。`{{名}}` に値を入れるだけ）。
- **英語の時**: `<head>` の内に `<html>` へ `i18n-wait` を付けて本文を隠し、読み終えたら `[data-i18n]` と `[data-i18n-attr]` を差し替え、`lang="en"` にして外す。差し替えは `textContent` と属性でだけする（`innerHTML` は使わない）。途中で止まっても、`site.css` の守り（CSS だけで効く）が 1.5 秒で本文を日本語のまま見せる。
- **JavaScript の決まり**: 新着で server の HTML を読み直す script は、比べと差し替えの前に `window.callboardLocalize(読み直した木)` で訳す。JavaScript は、起きた時に DOM の字を読まない（本文の終わりの script は差し替えより先に動くので、日本語を取る）。字は `callboardT` で引く。
- **鍵を付ける要素の決まり**: `data-i18n` は、子の要素を持たない要素にだけ付ける（英語の時は字を丸ごと替えるので、子の link・強め・数の要素は消える）。字の中に link や時刻がある文は、その前と後で鍵を分け、それぞれを `<span>` に入れる。
- **Rust が組む字**: Rust（`crates/server/src/`）は、画面に出す字を日本語で返さず、鍵と値（`ui/i18n.rs` の `Text`）で返す。`Text` を `{{ x }}` で書くと日本語の字、`{{ x.attrs()|safe }}` でその要素の `data-i18n`（と `data-i18n-options`）、属性の字は `x.attr("title")`。英語は、ブラウザーが同じ鍵と値で組む。時・日のように言語で替わる単位は、値に入れずに鍵を分ける（`usage.bySlot.heading.hour`・`.day`）。鍵の無い字（数・時刻・記録の値）は `Text::plain` で、鍵を付けない。
- **入れ子の字と名の並び**: ほかの字の中に入れる字（「castlist に聞けません（つながりません）」の「つながりません」）は、鍵と値のまま `data-i18n-options` に `{"why":{"$t":"upstream.unreachable"}}`（値があれば `{"$t":"<鍵>","values":{…}}`。入れ子の中の入れ子も同じ）で書く（`Text::with_text`）。名の並びは `{"$join":["ai","human"],"sep":"common.listSep"}`（`Text::with_list`）。サーバーは日本語で組み（区切りは「・」）、`i18n.js` は i18next に渡す前に、値の中の `$t`・`$join` を今の言語で先に組む（`callboardT(鍵, 値)` も同じ）。`$t` は値の側にだけ置き、訳の file には `$t(…)` を書かない。
- **万の数**: 値は言語で替わらない数の部分だけ（`common.tokens` の `man`（万の数）と `k`（千の数。同じ桁まで））で、形は訳の file に置く（日本語「{{man}} 万」・英語「{{k}}k」）。段の幅などは、この字を入れ子で入れる。
- **誤りの JSON**: callboard が返す誤りの答え（守りの層・file を上げる道・中継の道）は、`message`（日本語の字。今までのまま）に、`code`（その字の鍵）と `values`（差し込む値）を足す。ブラウザーは `callboardT(code, values)` で選んだ言語の字を組む（今はページの一覧の、上げられなかった訳）。castlist・pagetray が断った答えを運ぶ時は、`code` を型の名（`X-Amzn-Errortype`。`#` の前と `:` の後は落として比べる）の鍵（`errors.kind.unsupportedMediaType` など）にし、呼ぶ先の訳（`message`、無ければ `error`）を `values` の `detail` に入れる（日本語の字は `{{detail}}` で呼ぶ先の訳のまま。英語は型ごとの決まった字に、呼ぶ先の訳を「（…）」で添える）。`code` は callboard が付けた物だけを信じ、呼ぶ先の本文に `code` があれば `upstreamCode` に移す。訳の無い本文には足さない。
- **区切りの記号**: 字の間の区切りの記号（「 ・ 」「 · 」と、全角の括弧「（…）」）は、題（`<title>`）のほかは、鍵にせず記号のまま template と字に残す（英語の時も同じ記号。題だけは、鍵の字に「 ・ callboard」「 · callboard」まで入れる）。
- **訳の file の決まり**: 差し込みは `{{名}}` だけ（サーバーとブラウザーで字が食い違わないように。`$t(…)`・`{{n, number}}` は使わない）。数で言い分ける英語は `<鍵>_one`・`<鍵>_other`（日本語は `<鍵>_other` だけ。値の名は `count`）。`data-i18n-options` で数になるのは、数の型で渡した値だけ（字の `007` は字のまま）。`ja.json` と `en.json` は同じ鍵を持つ。画面に書いた鍵と、template と Rust の source に書いた鍵は、みな `ja.json` にある。どれも試験（`crates/server/tests/i18n.rs`・`e2e/tests/i18n.spec.ts`）が見る。画面の字の決まり（`ci/check-wording.sh`）は `ja.json` も見る。

### 訳語

英語の画面の字は、次の訳語にそろえる（`static/locales/en.json`）。

| 日本語 | 英語 |
|---|---|
| アクター | Actor（並びは Actors） |
| ラベル | Label |
| 稼働中・停止・停止（推し） | Running・Stopped・Stopped (inferred) |
| 人 | Human |
| タイムライン・相関図 | Timeline・Network |
| 使用量・額 | Usage・Cost |
| 時刻ずれ | Clock skew |
| 絞り込み | Filters |
| ページの置き場・メッセージの記録 | Page store・Message log |
| 返事待ち・レビュー待ち | Awaiting reply・Awaiting review |
| 届いた | Delivered |
| 課題・役割 | Issue・Role |
| 文脈・要約 | Context・Compaction |

### 言語を足す・字を足す

- **字を足す**: `ja.json` と `en.json` に同じ鍵を足し、template には `data-i18n` と `{{ "<鍵>"|t }}`、Rust には `Text::new("<鍵>")`、JavaScript には `callboardT("<鍵>")` で書く。鍵が片方にしか無い・画面に書いた鍵が無い・差し込みの形が違う、は試験が落とす。
- **言語を足す**: 今の作りでは、言語を名指す所が次のようにあり、どれにも 1 つずつ足す（サーバーが書く字は日本語のまま替えない。言語の並び 1 つから回す形にするのは、まだ）。
  1. `static/locales/<言語>.json`（`ja.json` と同じ鍵）。
  2. Rust: `crates/server/src/ui/i18n.rs` に `<言語>` の JSON と、配る script（`<言語>_JS`）と `?v=` の付いた path（`<言語>_JS_HREF`）を足し、`crates/server/src/ui.rs` に配る関数と route（`/static/locales/<言語>.js`）を足す。
  3. `templates/base.html`: `<head>` の訳の script と、言語の切り替えの釦。
  4. `static/i18n.js`: `LANGS` に足し、`pick()`（今はブラウザーの言語が `en` かだけを見る）をその言語も選ぶ形にし、`i18next.init` の `resources` に足す。
  5. 試験と smoke: `crates/server/tests/common/mod.rs` の `without_i18n_scripts`（head の script の並び）、`crates/server/tests/i18n.rs`（鍵のそろいと配る道。今は `ja`・`en` だけを比べる）、`ci/smoke-image.sh` の `i18n_scripts`、`e2e/tests/i18n.spec.ts`。

## 配る file

| 道 | 中身 |
|---|---|
| `GET /static/site.css` | 画面の CSS（`text/css`） |
| `GET /static/network.js` | 相関図の script（`application/javascript`） |
| `GET /static/vendor/cytoscape-3.35.0.min.js` | 取り込んだ cytoscape |
| `GET /static/vendor/i18next-27.2.9.min.js` | 取り込んだ i18next（画面の字の言語） |
| `GET /static/locales/ja.js`・`GET /static/locales/en.js` | 日本語と英語の字（`static/locales/{ja,en}.json` を script に包んだ物。`application/javascript`） |
| `GET /static/i18n.js` | 画面の字を差し替える script |
| `GET /static/icon.svg` | browser の tab に出す印（`image/svg+xml`） |

どれも binary に埋め込む。画面は CSS と script を `?v=<中身の写しの頭>` を付けて読み、取り込んだ物（cytoscape・i18next）は版が path に入るので、中身が替われば browser は読み直す。長く持たせてよい（`Cache-Control: public, max-age=31536000, immutable`）。印だけは 1 日。

## `GET /messages` メッセージの一覧

AI の担当どうしのメッセージ（roster が配る）の記録を、送った時刻の新しい順に表示する。pagetray の `/messages` を移した物で、記録は pagetray の API（`ListMessages` と、絞り込みの候補と件数の `GetMessageFacets`。pagetray 1.2.0 から）で読む。callboard は見せるだけ（状態は、pagetray が残した字を、画面の字にして出す）。

- **操作の帯**: 「メッセージの記録」（条件の無い一覧へ）・「ページ」（ページの一覧へ）・「まだ届いていない n 件」（押すと、ほかの絞り込みを外して、まだ届いていない物だけに絞る。0 件なら出さない。tab の題にも出す）・「絞り込みを外す」（絞っている時だけ）・新着の印。
- **絞り込みの列**: 状態・受け手・送り手・Key・project の候補と件数。件数は、絞りに関わらず置き場の全部。押すと今の条件に重ね、選んである候補をもう 1 度押すと外す。条件は URL に出る（`project`・`key`・`to`・`from`・`state`）。受け手・送り手・Key の候補は 100 件まで（今絞っている値は、その外でも出す）。
- **行**: 送った時刻・Key・見出し・状態（まだ届いていない・届いた・渡せない・期限切れ・ほかの道具で運んだ）・「送り手 → 受け手」・届いた時刻・渡した時（`gate` は「検査が止めた後」、`wake` は「起こした」、`prompt` は「人の入力の時」、`max-deliveries` は「渡し直しの上限を超えた」、`ttl` は「期限で消えた」、`closed` は「席を閉じた」。ほかの名は書かれた字のまま）・渡し直しの数・本文。本文は HTML として出さない。200 字か 5 行を超える本文は、頭の 120 字だけを見せ、開くと全部。
- **続き**: 1 度に 200 件。続きがあれば「続きを出す」の link（`at` に、pagetray の続きの印を付ける）。
- **引数**: 空の値・知らない状態（`state`）・知らない引数は、書かないのと同じ。同じ名を 2 度書いた物は 400 の画面。値の形が違う物（受け手の名が 200 字を超える・Key の形が違う・読めない続きの印）は、pagetray が断り、その訳を書いた 400 の画面。
- pagetray に聞けない時は 503 の画面で、訳を書く。pagetray が 1.2.0 より古いと、「pagetray に聞けません（404 が返りました）」になる。

### 新着の知らせ

script（`/static/messages.js`）が、`GET /api/pagetray/events` を `EventSource` で聞く。この道は、pagetray の新着の知らせ（`GET /api/events`。SSE）を、名も中身もそのまま流す（browser は pagetray に直には聞かない）。

- メッセージの記録が置かれる・書き足されるたびに（`event: message-record`）、script が今の URL を callboard から読み直し、一覧の列と絞り込みの列・まだ届いていない数・tab の題を、押さずに差し替える。開いていた本文は開いたまま。下へ送って見ている時は、見ている行を同じ所に置き、頭に入った数を新着の印（「● 新着 n 件（上へ）」）に出す。続き（`at` のある URL）を見ている時は、行は替えず、「● 新しい記録（読み直す）」を出す。ほかの知らせ（ページを置いた知らせなど）では読み直さない。
- 流れには時間切れを付けない（つなぎっぱなし。pagetray が 15 秒ごとに送る注釈の行も、そのまま流す）。browser 1 つにつき、pagetray へのつなぎが 1 本。browser が閉じれば、pagetray へのつなぎも切れる。
- pagetray を起こし直すと流れが終わり、browser がつなぎ直す。つなぎ直した時に pagetray が止まっていれば（callboard が 503 を返す）、script が 3 秒ごとに試す。つながるたびに（初めての時も）、今の URL を読み直す（つながる前・切れていた間の記録を入れる）。callboard を起こし直した時も同じ。
- pagetray の答えの頭は 4 秒まで待つ。届かない・知らせの流れでない答え（200 でない・型が `text/event-stream` でない）は 503 `Unavailable`。
- script が動かなくても、一覧は開き、絞り込みも続きも link で動く（新着は、読み直すと出る）。

pagetray の画面との違いは 3 つ: 時刻を callboard が日本時間で組むこと（pagetray の画面は、UTC の字を出して、script がその機械の時刻に書き替える）・値の形が違う引数と読めない続きの印を 400 の画面にすること（pagetray の画面は、条件が無い物として出す）・頭の帯。外す前（pagetray 1.3.0 まで）は、CI の browser の試験が、同じ pagetray を相手に、pagetray の画面と中身を比べていた（動いている置き場の 109 画面でも、違いは 0）。比べる相手が無くなったので、その試験は外した（1.0.0）。

## `GET /pages/` ページの一覧

担当が置いたページ（依頼書・提案・計画・成果・確認・レビュー）を、置いた時刻の新しい順に表示する。pagetray の `/` を移した物で、記録は pagetray の API（`ListPages`・`GetPageFacets`・`ListReviews`。Key の画面では `ListFiles`・`GetFileRules` も。`GetPageFacets`・`ListReviews`・`GetFileRules` は pagetray 1.3.0 から）で読む。`/pages`（後ろに `/` の無い道）は、`/pages/` へ送る。

- **3 つの列**: 絞り込みの列・一覧の列・本文の枠。幅が狭い時（900px 以下）は、縦に並ぶ。
- **操作の帯**: 「ページの置き場」（条件の無い一覧へ）・「メッセージ」（メッセージの一覧へ）・探す form（題と本文。今の絞り込みを保つ）・並び（新しい順・古い順）・待ちの数の釦（返事待ち・レビュー待ち・上げた。押すと、その待ちだけに絞る。0 件なら出さない。返事待ちの数は tab の題にも出す）・「絞り込みを外す」・新着の印。
- **絞り込みの列**: 返事（返事待ち・答えた・レビュー待ち・上げた）・判定・送り手・担当者（役割）・workspace（その ID で置かれた Key を添える）・種類・Key・project の候補と件数。件数は、絞りに関わらず置き場の全部。条件は URL に出る（`project`・`key`・`kind`・`role`・`workspace`・`q`・`reply`・`on`・`verdict`・`actor`・`sort=created_at`）。
- **行**: 置いた時刻・Key・種類と番号・レビューの判定・返事の印（返事待ち・答えた・レビュー待ち・レビュー済み・上げた・決まった）・題・役割と workspace。行の下に、見た物（レビューのページ）・見る物（ほかのページ）の link、このページを見たレビューの link と判定、送り手（castlist の名。押すと callboard のプロフィール。castlist の答えの無い送り手は「未確認」）と付随する id。
- **本文の枠**: 行を押すと、そのページが本文の枠（`<iframe sandbox="allow-scripts">`）に 1 枚だけ出る（URL に `open=<ページの path>`）。枠の中身は、browser が pagetray から直に読む（`<pagetray の link の頭>/<project>/<key>/<kind>-<n>.html`）。ページの path の形でない `open` は、書かないのと同じ。
- **続き**: 1 度に 200 枚。続きがあれば「続きを出す」の link（`at` に、pagetray の続きの印を付ける）。
- **引数**: 空の値（workspace だけは、空も条件）・知らない返事の状態と判定と並び・知らない引数は、書かないのと同じ。同じ名を 2 度書いた物は 400 の画面。値の形が違う物（project・Key・種類の形・アクターの id・読めない続きの印）は、pagetray が断り、その訳を書いた 400 の画面。
- pagetray に聞けない時は 503 の画面で、訳を書く。pagetray が 1.3.0 より古いと、「pagetray に聞けません（404 が返りました）」になる。
- **この画面の CSP**: `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob: <pagetray の link の頭>; connect-src 'self'; frame-src <pagetray の link の頭>; frame-ancestors 'none'; base-uri 'none'; form-action 'self'`。足したのは、本文の枠（`frame-src`）・file の枠の図（`img-src`）・上げる前の図の見本（`blob:`）だけ。

### 新着・返事の状態・未読

script（`/static/pages.js`）が、メッセージの一覧と同じ道（`GET /api/pagetray/events`）を聞く。

- ページが置かれると（名の無い知らせ）、一覧の列と絞り込みの列を押さずに差し替える（本文の枠は読み込み直さない）。今の絞り込みで一覧に入ったページの数を、tab の題の頭（`(n)`）に出す。下へ送って見ている時は、見ている行を同じ所に置き、新着の印（「● 新着 n 件（上へ）」）を出す。
- 返事の状態・レビューの印が書かれると（`reply`・`review`）、行の印と待ちの数を新しくする（新着には数えない）。
- **未読の印**（題の頭の ●）: この browser で初めて開いた時より後に置かれ、まだ本文の枠に出していないページ。browser が覚える（`localStorage` の `callboard.pages.since`・`callboard.pages.seen`。5,000 枚まで）。pagetray の画面で覚えた印は、origin が違うので引き継がない。
- つなぎ直しは、メッセージの一覧と同じ（つながるたびに読み直し、つなげなかった時は 3 秒ごとに試す）。

### Key の画面（file の枠と、上げる枠）

project と Key を 1 つずつ選んだ時（`?project=…&key=…`）だけ、一覧の列の下に 2 つの枠を出す（project が `api` の時と、形の違う名には出さない）。

- **file の枠**: その Key の file（新しい順に 200 まで）。名（押すと、pagetray の file を別の tab で開く）・図なら小さな見本・大きさ・役割・置いた時刻・ページから参照する時の相対の path（`files/<語>-<番号>.<拡張子>`）・送り手。
- **上げる枠**: file を選ぶ（か、落とす）と、拡張子と大きさを見て（受ける種類と上限は、pagetray の `GetFileRules` の答え）、見本を出す。「秘密が写っていないか見た」に印を付けると、送れる。script が要る。
- 送る先は、callboard の `POST /api/pagetray/projects/{project}/keys/{key}/files?name=<語>.<拡張子>`（本文は file の中身。`Content-Type` は、その拡張子の種類）。callboard が、pagetray の `CreateFile` に、**役割 `product-manager`** で渡す。browser からは、名・中身の種類・中身だけを受ける（役割・workspace・番号・時刻・付随する id を書いても、渡さない）。答えは、pagetray の答え（置いた記録の 201 か、断った訳。種類が合わなければ 415）の状態・型・本文を返す（断った答えの本文には `code`・`values` を足す）。pagetray に聞けなければ 503（pagetray の答えを 4 秒まで待つ。時間切れの時は、置かれたかもしれないので、画面は file の枠を読み直し、確かめてから送り直すように書く）。
- **この道の守り**: callboard の、ただ 1 つの書く道。この callboard の画面の form からの要求（`Origin` が `http://<Host>` とそのまま同じで、`Sec-Fetch-Site` が無いか `same-origin`）だけを通す。ほかの site・本文の枠の中のページ（pagetray の origin か、sandbox の `null`）・`Origin` の無い要求は 403。本文は 16 MiB まで（pagetray が file 1 つに受ける大きさ。ほかの道は 2 MiB）。project と Key の形が違う・名が無い要求は 400。
- ほかの所（`roster file put` など）から、この Key に file が置かれると（`file` の知らせ）、file の枠を押さずに読み直す。

pagetray の画面との違いは 5 つ: 一覧の道が `/pages/` の 1 つなこと（pagetray の `/<project>/`・`/<project>/<key>/` の道は無い）・時刻を callboard が日本時間で組むこと・値の形が違う引数と読めない続きの印を 400 の画面にすること・送り手の名の link が callboard のプロフィールなこと・未読の印を callboard の origin で覚え直すこと。外す前（pagetray 1.3.0 まで）は、CI の browser の試験が、同じ pagetray を相手に、pagetray の画面と中身を比べていた（動いている置き場の 330 画面でも、違いは 0）。比べる相手が無くなったので、その試験は外した（1.0.0）。
