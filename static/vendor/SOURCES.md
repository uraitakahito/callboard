# 取り込んだ外の file

画面が使う外の JavaScript。CDN から読まず、callboard が binary に埋め込んで自分で配る。package ごとに `## <名>` の節を 1 つ置く。`ci/check-vendor.sh` が、節ごとに、表の値と、npm の registry の物と、commit した file とを突き合わせる（CI の job `test`）。

## cytoscape

相関図（`/graph/network`）が使う（castlist の画面が配っていた物と同じ file）。

| 項目 | 値 |
|---|---|
| package | `cytoscape` |
| version | `3.35.0` |
| tarball | `https://registry.npmjs.org/cytoscape/-/cytoscape-3.35.0.tgz` |
| integrity | `sha512-sMvBIXiGMnhkCaWz2R06FwF/27MNlS+Tqvi17foO9UhnUg3XGZccKjxsvEqQFAQYMSSZmN5SxYzUYlQ0DOJJ3g==` |
| path | `package/dist/cytoscape.min.js` |
| file | `static/vendor/cytoscape-3.35.0.min.js` |
| sha256 | `71fa9ebec21a7ca2c40cff6aacc30d1f6464e363662ad4b2c197e1adc770cd94` |
| license | MIT（The Cytoscape Consortium） |
| license_path | `package/LICENSE` |
| license_file | `static/vendor/cytoscape-LICENSE` |

- 公開: 2026-11-10（npm の `latest`）。
- 版を上げる時は、[画面](../../docs/src/ui.md) の「取り込んだ外の JavaScript」の「版の上げ方」。

## i18next

画面の字を、言語ごとに差し替える（AI-50）。UMD 版（`window.i18next`）を、1.1.0 から、どの画面も読む。

| 項目 | 値 |
|---|---|
| package | `i18next` |
| version | `27.2.9` |
| tarball | `https://registry.npmjs.org/i18next/-/i18next-27.2.9.tgz` |
| integrity | `sha512-GOWWofCGYPSPEzUKJnRLJSoHrSUDa/NzYWqAOJJDZUaM7faLPd9VABlbHDZ+jv8uadu/fq1wsHFfZP6+0tFPkg==` |
| path | `package/dist/umd/i18next.min.js` |
| file | `static/vendor/i18next-27.2.9.min.js` |
| sha256 | `73d7e49a3b51b53c8cda433f0a030d1724a2e3b8b7dcb392d660dbfaf465c549` |
| license | MIT（i18next） |
| license_path | `package/LICENSE` |
| license_file | `static/vendor/i18next-LICENSE` |

- 公開: 2026-11-25（取り込んだ 2026-11-28 の `latest` は 2026-11-27 の 27.2.10 で、公開から 2 日たっていなかったので、1 つ前の版）。
- 版を上げる時は、[画面](../../docs/src/ui.md) の「取り込んだ外の JavaScript」の「版の上げ方」。
