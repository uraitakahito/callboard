# callboard

castlist（アクターの台帳）と pagetray（ページの置き場）の画面をまとめるサーバー。castlist と pagetray は、Smithy の定義から作った client で API を通して使う。data は持たない。

v1.6.0 を出した（image は `ghcr.io/uraitakahito/callboard:v1.6.0`。使用量の画面で、額の棒の幅（10 分・1 時間・1 日）を選べるようにし、棒の下に、区切りごと × 役割の升を足した版。画面は日本語と英語で出す（1.1.0 から）。castlist と pagetray の画面は、1.0.0 でみな移し終えた）。今あるのは、入口と、castlist から移した 4 つの画面（アクターの一覧・プロフィール・タイムライン・相関図）と、pagetray から移した 2 つの画面（メッセージの一覧・ページの一覧）と、メッセージの記録を課題ごとに見る 2 つの画面（課題の一覧・課題の筋）と、使用量の画面。変えたことは [CHANGELOG.md](CHANGELOG.md)。

- 本: https://uraitakahito.github.io/callboard/ （release の後に出る。原文は [docs/src/](docs/src/)。概要 [docs/src/index.md](docs/src/index.md)・動かす [docs/src/run.md](docs/src/run.md)・画面 [docs/src/ui.md](docs/src/ui.md)・release を出す [docs/src/release.md](docs/src/release.md)・開発 [docs/src/development.md](docs/src/development.md)）
- license: MIT（[LICENSE](LICENSE)）。取り込んだ cytoscape（相関図）と i18next（画面の字の多言語化）も MIT（[static/vendor/cytoscape-LICENSE](static/vendor/cytoscape-LICENSE)・[static/vendor/i18next-LICENSE](static/vendor/i18next-LICENSE)。出どころは [static/vendor/SOURCES.md](static/vendor/SOURCES.md)）
