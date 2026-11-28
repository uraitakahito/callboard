# callboard

castlist（アクターの台帳）と pagetray（ページの置き場）の画面をまとめるサーバー。

- 画面は callboard が持ち、castlist と pagetray は API を通して使う。API の形は、それぞれの repo の Smithy の定義で決まり、callboard はその定義から作った client で呼ぶ。
- data は持たない（DB も volume も無い）。止めても、起こし直しても、失う物は無い。
- 画面は、サーバーが HTML を組んで返す。

v1.3.0 を出した（image は `ghcr.io/uraitakahito/callboard:v1.3.0`。使用量の画面に、model と役割ごとの行列（行は役割と手伝いの種類、列は model）を足した版。画面は日本語と英語で出す（1.1.0 から）。castlist と pagetray の画面は、1.0.0 でみな移し終えた。変えたことは repo の `CHANGELOG.md`）。今あるのは、入口（`/`。castlist と pagetray が生きているかと版、それぞれの画面への link）と、castlist から移した 4 つの画面（アクターの一覧 `/actors/`・プロフィール `/actors/{id}`・タイムライン `/graph/timeline`・相関図 `/graph/network`）と、pagetray から移した 2 つの画面（メッセージの一覧 `/messages`・ページの一覧 `/pages/`）と、callboard で初めて作った使用量の画面（`/usage/`）。castlist は 1.1.0 から（使用量は 2.1.0 から）、pagetray は 1.3.0 からが要る。

## 守り

castlist・pagetray と同じ守りを、callboard の入口に持つ。

- host（callboard を動かす機械）の外からは届かない（`127.0.0.1` だけに出す）。`Host` が `127.0.0.1`・`localhost` でない要求と、`Origin` の付いた書く要求は 403（browser で開いたほかのページからの読み書きを断る物）。
- 画面の応答には Content-Security-Policy を付け、callboard 自身が配る script と style だけを許す（inline の script と style も断る。form は callboard 自身へだけ送れる）。画面の字は日本語と英語で出す（サーバーが日本語を書き、英語はブラウザーの i18next が差し替える。JavaScript を切っても、相関図のほかは日本語で読める）。外の library（相関図の cytoscape と、字の i18next。どちらも MIT）も CDN から読まず、npm の物と同じかを CI で確かめて callboard が配る。
- 1 つの問い合わせは 5 秒まで（過ぎれば 503）、本文は 2 MiB まで（超えれば 413）。
- castlist と pagetray には、browser から来た header（`Origin`・`Cookie` など）を渡さない。`Host: 127.0.0.1` で聞く。
- container を起こす・止める・版を上げるのは、DevOps Engineer 担当だけ（repo の設定の `infra.callboard` は `tpm`。[動かす](run.md)）。
