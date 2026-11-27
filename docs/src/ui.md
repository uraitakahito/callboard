# 画面

callboard が HTML を組んで返す画面（askama。`templates/`）。callboard を動かす機械の browser で `http://127.0.0.1:8803/` を開く。

- 字はみな HTML として逃がす（ラベル・名・出来事の `detail` など、送り手が書いた字も）。
- script を使うのは相関図だけで、callboard が配る file（`GET /static/network.js` と、取り込んだ cytoscape）だけを読む。inline の script は無い。style は callboard が配る CSS（`GET /static/site.css`）だけで、inline の style も無い。どの file も binary に埋め込む。
- どの画面にも Content-Security-Policy を付ける: `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'`。browser は、castlist と pagetray に直には聞かない（`connect-src 'self'`）。
- 時刻は、castlist が受け取った時刻を日本時間（UTC+9）で表示する。ISO 8601 の字（UTC・ミリ秒まで）は `<time datetime>` と `title` に置く（時刻にマウスを重ねると表示される）。
- 明るい色と暗い色の両方を持つ（`prefers-color-scheme`）。
- どの画面の頭にも、画面の間の link（アクター・タイムライン・相関図・ページ・メッセージ）を出す。

## 移した画面と、まだ移していない画面

castlist と pagetray の画面を、1 つずつ callboard に移している。castlist の 4 つの画面は、みな移した（0.2.0）。まだ移していない pagetray の画面への link は、pagetray の画面を指す（link の頭は設定の `CALLBOARD_PAGETRAY_LINK_URL`。[動かす](run.md)）。

| 画面 | 今ある所 | callboard の道 | castlist・pagetray での道 |
|---|---|---|---|
| 入口 | callboard | `/` | — |
| アクターの一覧 | callboard（0.2.0 から） | `/actors/` | castlist の `/` |
| プロフィール | callboard（0.1.0 から） | `/actors/{id}` | castlist の `/actors/{id}` |
| タイムライン | callboard（0.2.0 から） | `/graph/timeline` | castlist の `/graph/timeline` |
| 相関図 | callboard（0.1.0 から） | `/graph/network` | castlist の `/graph/network` |
| ページの一覧 | pagetray | これから（`/pages/`） | pagetray の `/` |
| メッセージ | pagetray | これから（`/messages`） | pagetray の `/messages` |

castlist の画面との違い（画面の中身。頭の帯の下）は 2 つだけ: 送り手の時刻の ISO の字をミリ秒までにそろえることと、誤りの画面の戻りの link（「一覧へ」でなく「入口へ」）。CI の browser の試験（`e2e/tests/compare.spec.ts`・`profile.spec.ts`）が、同じ castlist を相手に、castlist の画面と比べている。

## `GET /` 入口

castlist と pagetray が生きているか（「つながっています」と版。聞けなければ「届きません」と訳）と、それぞれの画面への link。呼ぶ先が答えなくても 200 で返す。生きているかは、開くたびに聞く（1 秒まで待つ）。

## 期間の欄

一覧の稼働した期間（`active_from`・`active_to`）と、2 つの図（タイムライン・相関図）の登録した期間（`from`・`to`）の欄は、browser の日時の欄（`<input type="datetime-local" step="any">`。script は使わない）。

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

絞り込みは、castlist の監査の検索 `ListActors`（`GET /api/actors`）と同じ条件を、画面の引数の名（snake_case。`activeFrom` は `active_from`、`launchedBy` は `launched_by`、`maxResults` は `limit`、`nextToken` は `after`）で URL に置く。引数は callboard が確かめ（`crates/server/src/ui/list.rs`・`ui/params.rs`）、確かめた物を castlist の API に渡す。form には `label`（何度でも）・`active`（状態: すべて・稼働中・停止）・`kind`（種類: すべて・人。人を探す道として残す。AI だけに絞る選び肢は無いが、URL の `kind=ai` は今と同じく AI だけに絞る）・`model`・`effort`・`active_from`・`active_to`（稼働した期間（から）・（まで）。上の「期間の欄」）・`skewed`（時刻ずれあり）の欄がある。ほかの引数（`binding`・`at`・`launched_by`・`sort`・`order`・`limit`）も URL に書けば効き、form を送り直しても残る。

- 絞り込みは、すべての条件に合うアクターを表示する（AND）。枠の見出しは「絞り込み」だけで、この訳は画面に書かない。
- form は空の欄も送るので、空の値は付けなかったのと同じにする。
- `order` が無ければ `desc`（新しい順）、`limit` が無ければ 100。
- 条件が無ければ、`active_from` を今から 7 日前にして表示し、そう書く（「この 7 日に稼働したアクター」。castlist の API は条件の無い検索を断るので）。登場の記録の無いアクターは、この既定では表示されない。ラベルなどで絞り込むと表示される。
- 次のページは「次のページ」の link（`rel="next"`）。条件はそのままで、検索の次の頁の印を `after` に付ける。
- 引数の誤り（知らない名・1 つだけの名の重なり・読めない値）は、引数の名を書いた 400 の画面（期間の欄の読めない値だけは、[期間の欄](#期間の欄)の字）。castlist には聞かない。
- 行の「状態」の横の出来事の名と、「時刻ずれ」の印は、castlist 1.1.0 で足した欄（`presence`・`skewed`）から出す。
- castlist に聞けない時は 503 の画面で、訳を書く。

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
| 出来事 | 受け取った時刻・出来事の日本語の名（字に重ねると id）・理由（`detail.reason`）・きっかけ（`detail.source`）・ほかの `detail`（鍵の順の JSON の字）・送り手の時刻・時刻ずれの印 |

- 種類の印は、人（Product Manager）にだけ「人」。AI には何も出さない（アクターはほとんど AI なので）。
- 無い id と、id の形（小文字の UUID の字）でない物は 404 の画面。形でない物は、castlist に聞かない。
- castlist に聞けない時は 503 の画面で、訳（つながりません・時間内に答えがありません・`<状態>` が返りました・答えを読めません）を書く。聞く先の URL と、castlist の誤りの字は出さない（記録に残る。[動かす](run.md) の「確かめる」）。
- 無い物は「ありません。」（起動した人が無い人の起点は「ありません（起点）。」）と書く。
- 下に「表示した時刻」（castlist がアクターを読み始めた時刻）を書く。
- 2 つの答えは別々の読みなので、間に記録が足されると、片方にだけ出ることがある。名を引けないアクターは、id のまま出す。

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
- 答えの大きさ: castlist は頁を分けずに、組のアクターの出来事の全部を返す（アクター 108 人で 257 KB、castlist が答えるまで 20 ms ほどだった。2026-11-27 の動いている台帳の写しで測った）。広い組は、ラベルか期間で絞る。

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

castlist の `GetGraph`（`GET /api/graph`）の答えを、そのまま返す道。相関図の script が使う（browser は castlist に直には聞かない）。

- 渡すのは、問い合わせの引数の字だけ（castlist が確かめる）。browser の header（`Cookie`・`Referer` など）は渡さず、`Host: 127.0.0.1` で聞く。
- 返すのは、castlist の答えの状態・本文の型・誤りの型の名（`X-Amzn-Errortype`）・本文。castlist の誤り（400 など）も、そのまま返る。
- castlist に聞けない時（つながらない・答えを読み終えるまで 4 秒）は、503 `Unavailable`（`{"message":"castlist に聞けません（…）"}`）。
- 読むだけ（`GET`・`HEAD`）。castlist のほかの道は中継しない。

CSP を緩めずに cytoscape を使うための、2 つの決まり（`static/network.js`）:

- cytoscape は、初めに `<style>` を head に差し込もうとする（CSP の `style-src 'self'` が断り、違反になる）。同じ id（`__________cytoscape_stylesheet`）の印を先に置くと差し込まないので、そうする。差し込むはずだった決まり（容れ物を `position: relative` に）は `site.css` に書く。
- cytoscape は、絵の画面の中の位置を覚えておき、押した所の点を探す。覚え直すのは window の scroll と大きさの替わりの知らせが来た時だけなので、指が入った時と押す前に覚え直させる（`cy.resize()`）。

### 取り込んだ cytoscape

- [cytoscape](https://js.cytoscape.org/) 3.35.0（MIT。The Cytoscape Consortium）。npm の tarball の `package/dist/cytoscape.min.js` を、そのまま `static/vendor/cytoscape-3.35.0.min.js` に置き、callboard が binary に埋め込んで配る（CDN からは読まない）。license の写しは `static/vendor/cytoscape-LICENSE`。
- 出どころの記録は `static/vendor/SOURCES.md`。`ci/check-vendor.sh`（CI の job `test`）が、npm の registry の物と、commit した file とを突き合わせる（書き替えた写しを当てることも CI で見る）。

版の上げ方:

1. 新しい版が npm の `latest` で、公開から 2 日たっていることを見る（`curl -s https://registry.npmjs.org/cytoscape | jq '."dist-tags", .time["<版>"]'`）。
2. 空の folder で tarball を落とし、`package/dist/cytoscape.min.js` を `static/vendor/cytoscape-<版>.min.js` に、`package/LICENSE` を `static/vendor/cytoscape-LICENSE` に写す。古い版の file は消す。
3. `static/vendor/SOURCES.md` の version・tarball・integrity・file・sha256 を書き替える。
4. 版の入った path を替える: `crates/server/src/ui.rs` の `CYTOSCAPE_JS`・`CYTOSCAPE_PATH`、`crates/server/tests/network.rs`、`ci/smoke-image.sh`、CI の「見張りが、書き替えた写しを当てるか」の file の名。
5. `ci/check-vendor.sh`・`cargo test --locked`・`cd e2e && pnpm test`（console の誤りと CSP の違反が 0 か）を流す。

## 出来事の名

id は英語のまま（castlist の API）。日本語の名は画面だけで、`crates/server/src/ui/profile.rs` の `EVENT_NAMES` の 1 か所に置く。

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

## 配る file

| 道 | 中身 |
|---|---|
| `GET /static/site.css` | 画面の CSS（`text/css`） |
| `GET /static/network.js` | 相関図の script（`application/javascript`） |
| `GET /static/vendor/cytoscape-3.35.0.min.js` | 取り込んだ cytoscape |
| `GET /static/icon.svg` | browser の tab に出す印（`image/svg+xml`） |

どれも binary に埋め込む。画面は CSS と script を `?v=<中身の写しの頭>` を付けて読み、cytoscape は版が path に入るので、中身が替われば browser は読み直す。長く持たせてよい（`Cache-Control: public, max-age=31536000, immutable`）。印だけは 1 日。
