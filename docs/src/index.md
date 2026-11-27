# callboard

castlist（アクターの台帳）と pagetray（ページの置き場）の画面をまとめるサーバー。

- 画面は callboard が持ち、castlist と pagetray は API を通して使う。API の形は、それぞれの repo の Smithy の定義で決まり、callboard はその定義から作った client で呼ぶ。
- data は持たない（DB も volume も無い）。止めても、起こし直しても、失う物は無い。
- 画面は、サーバーが HTML を組んで返す。

v0.2.0 を出した（image は `ghcr.io/uraitakahito/callboard:v0.2.0`。変えたことは repo の `CHANGELOG.md`）。今あるのは、入口（`/`。castlist と pagetray が生きているかと版、それぞれの画面への link）と、castlist から移した 4 つの画面（アクターの一覧 `/actors/`・プロフィール `/actors/{id}`・タイムライン `/graph/timeline`・相関図 `/graph/network`）。pagetray の画面（ページの一覧・メッセージ）は、これから移す。移すまでの間、その画面への link は pagetray の画面を指す（[画面](ui.md)）。castlist は 1.1.0 からが要る。

## 守り

castlist・pagetray と同じ守りを、callboard の入口に持つ。

- host（callboard を動かす機械）の外からは届かない（`127.0.0.1` だけに出す）。`Host` が `127.0.0.1`・`localhost` でない要求と、`Origin` の付いた書く要求は 403（browser で開いたほかのページからの読み書きを断る物）。
- 画面の応答には Content-Security-Policy を付け、callboard 自身が配る script と style だけを許す（inline の script と style も断る。form は callboard 自身へだけ送れる）。script を使うのは相関図だけで、外の library（cytoscape。MIT）も CDN から読まず、npm の物と同じかを CI で確かめて callboard が配る。
- 1 つの問い合わせは 5 秒まで（過ぎれば 503）、本文は 2 MiB まで（超えれば 413）。
- castlist と pagetray には、browser から来た header（`Origin`・`Cookie` など）を渡さない。`Host: 127.0.0.1` で聞く。
- container を起こす・止める・版を上げるのは、DevOps Engineer 担当だけ（repo の設定の `infra.callboard` は `tpm`。[動かす](run.md)）。
