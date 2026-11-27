# 画面

callboard が HTML を組んで返す画面（askama。`templates/`）。callboard を動かす機械の browser で `http://127.0.0.1:8803/` を開く。

- 字はみな HTML として逃がす（ラベル・名・出来事の `detail` など、送り手が書いた字も）。
- script を使うのは相関図だけで、callboard が配る file（`GET /static/network.js` と、取り込んだ cytoscape）だけを読む。inline の script は無い。style は callboard が配る CSS（`GET /static/site.css`）だけで、inline の style も無い。どの file も binary に埋め込む。
- どの画面にも Content-Security-Policy を付ける: `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'`。browser は、castlist と pagetray に直には聞かない（`connect-src 'self'`）。
- 時刻は、castlist が受け取った時刻を日本時間（UTC+9）で表示する。ISO 8601 の字（UTC・ミリ秒まで）は `<time datetime>` と `title` に置く（時刻にマウスを重ねると表示される）。
- 明るい色と暗い色の両方を持つ（`prefers-color-scheme`）。
- どの画面の頭にも、画面の間の link（アクター・タイムライン・相関図・ページ・メッセージ）を出す。

## 移した画面と、まだ移していない画面

castlist と pagetray の画面を、1 つずつ callboard に移している。まだ移していない画面への link は、castlist・pagetray の画面を指す（link の頭は設定の `CALLBOARD_CASTLIST_LINK_URL`・`CALLBOARD_PAGETRAY_LINK_URL`。[動かす](run.md)）。

| 画面 | 今ある所 | callboard の道 |
|---|---|---|
| 入口 | callboard | `/` |
| プロフィール | callboard | `/actors/{id}` |
| 相関図 | callboard | `/graph/network` |
| アクターの一覧 | castlist（`/`） | これから（`/actors/`） |
| タイムライン | castlist（`/graph/timeline`） | これから（`/graph/timeline`） |
| ページの一覧 | pagetray（`/`） | これから（`/pages/`） |
| メッセージ | pagetray（`/messages`） | これから（`/messages`） |

## `GET /` 入口

castlist と pagetray が生きているか（「つながっています」と版。聞けなければ「届きません」と訳）と、それぞれの画面への link。呼ぶ先が答えなくても 200 で返す。生きているかは、開くたびに聞く（1 秒まで待つ）。

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

## `GET /graph/network` 相関図

誰が誰を起動し、誰が誰から引き継いだかを、点と線の相関図で表示する。script を使う唯一の画面。

- 絞りは castlist の `GetGraph`（`GET /api/graph`）と同じ引数（`label`・`from`・`to`。どれも 1 つだけ）。form は空の欄も送るので、空の値は付けなかったのと同じにする。
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

### 期間の欄

登録した期間（`from`・`to`）の欄は、browser の日時の欄（`<input type="datetime-local" step="any">`）。

- browser が送る `YYYY-MM-DDTHH:MM`（`:SS`・`:SS.sss` も）は日本時間（UTC+9）として、RFC 3339 にしてから読む。URL に RFC 3339 の値を書けば、そのまま効く。どちらでもない値は 400 の画面で、欄の見出しと直し方を書く。
- 期間を付けて開いた画面では、欄にその期間を日本時間の字で出す。form を送り直しても同じ時刻。
- 欄の下に「期間（今の条件のまま、期間だけを替えます）」の link: 直近 1 時間・今日・昨日・直近 7 日。

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
