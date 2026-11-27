# callboard

castlist（アクターの台帳）と pagetray（ページの置き場）の画面をまとめるサーバー。castlist と pagetray は、Smithy の定義から作った client で API を通して使う。data は持たない。

v0.5.0 を出した（image は `ghcr.io/uraitakahito/callboard:v0.5.0`）。今あるのは、入口と、castlist から移した 4 つの画面（アクターの一覧・プロフィール・タイムライン・相関図）と、pagetray から移した 2 つの画面（メッセージの一覧・ページの一覧）と、費えの画面。変えたことは [CHANGELOG.md](CHANGELOG.md)。

- 本: https://uraitakahito.github.io/callboard/ （release の後に出る。原文は [docs/src/](docs/src/)。概要 [docs/src/index.md](docs/src/index.md)・動かす [docs/src/run.md](docs/src/run.md)・画面 [docs/src/ui.md](docs/src/ui.md)・release を出す [docs/src/release.md](docs/src/release.md)・開発 [docs/src/development.md](docs/src/development.md)）
- license: MIT（[LICENSE](LICENSE)）。相関図の画面が使う cytoscape も MIT（[static/vendor/cytoscape-LICENSE](static/vendor/cytoscape-LICENSE)。出どころは [static/vendor/SOURCES.md](static/vendor/SOURCES.md)）
