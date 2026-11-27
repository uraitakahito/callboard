# 開発

## 道具

Rust は `rustup` で入れる（Homebrew の `rust` は `rust-toolchain.toml` を読まない）。版は `rust-toolchain.toml` が決め、この repo の中で `cargo` を打つと、その版が入って使われる。

画面の browser の試験（`e2e/`）だけは Node を使う。版は `e2e/.node-version`、pnpm の版は `e2e/package.json` の `packageManager`、Playwright の版は `e2e/pnpm-lock.yaml` が決める（公開から 2 日たっていない版は入れない。`e2e/pnpm-workspace.yaml`）。初めに `cd e2e && pnpm install --frozen-lockfile && pnpm exec playwright install chromium`。設定は `e2e/playwright.config.ts`。Node と Playwright は image にも binary にも入らない。

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked -p castlist_client -p pagetray_client          # 定義の例から作った、生成した crate の中の試験（protocol の試験）
ci/check-wording.sh                                                # 公開する字と画面の字が、決めた字か
ci/check-vendor.sh                                                 # 取り込んだ cytoscape が npm の物と同じか（static/vendor/SOURCES.md）
(cd e2e && CALLBOARD_E2E_CASTLIST=<castlist-server の binary> pnpm test)   # 画面を、本物の castlist を相手に、本物の browser で見る（下の「browser の試験の相手」）
CALLBOARD_LISTEN=127.0.0.1:18813 CALLBOARD_CASTLIST_URL=http://127.0.0.1:8802 CALLBOARD_PAGETRAY_URL=http://127.0.0.1:8801 cargo run --bin callboard-server   # 手元で起こす（動いている callboard の 8803 とは別の port。castlist と pagetray は読むだけ）
mdbook build docs                                                  # この本を作る
ci/gen.sh                                                          # castlist と pagetray の定義を tag から取り直し、呼ぶ側の crate を作り直す
ci/gen.sh --check                                                  # 取った定義と、作り直した物が、入れてある物と同じか（CI の job model）
```

手元で起こすのは、試しの callboard だけにする。動いている callboard の container を起こす・止めるのは、DevOps Engineer 担当だけ（[動かす](run.md)）。

### browser の試験の相手

`e2e/` の試験は、出した版の castlist の server を一時の DB で起こし、castlist の API で見本の記録を置いてから、callboard の画面を開く。castlist の画面（古い画面）と callboard の画面の中身を比べる試験（新旧の比べ）も、この castlist を相手にする。

- CI（job `e2e`）は、`ci/castlist-binary.sh` が、出した版の image（`ci/upstream.env`）から binary を取り出す（arm64 の Linux の binary なので、同じ種類の機械で流す）。
- ほかの機械では、castlist の同じ版の tag を build した binary を、`CALLBOARD_E2E_CASTLIST` に渡す。castlist の repo を変えないように、tag を別の folder に取り出して build する: `git -C <castlist の repo> archive v1.0.0 | tar -x -C <空の folder>` の後、その folder で `cargo build --locked -p castlist-next`（v1.0.0 の tag では、image に入る server の package の名が `castlist-next`。binary は `target/debug/castlist-next`）。
- pagetray は起こさない（今ある画面は castlist だけを呼ぶ）。

## 置き場所

| 所 | 中身 |
|---|---|
| `crates/server/` | server（package `callboard`・binary `callboard-server`）。`src/config.rs` 設定・`src/guard.rs` 入口の守り・`src/upstream/` castlist と pagetray を呼ぶ所・`src/ui.rs` と `src/ui/` 画面 |
| `crates/castlist-client/`・`crates/pagetray-client/` | castlist と pagetray の定義から作った、呼ぶ側の crate。**手で直さない** |
| `model/` | castlist と pagetray の release の tag から取った定義（`model/castlist/`・`model/pagetray/`）と、生成器の設定（`smithy-build.json`） |
| `templates/`・`static/` | 画面の template（askama）と、配る CSS・script・取り込んだ cytoscape（`static/vendor/`）。binary に埋め込む |
| `e2e/` | 画面の browser の試験（Playwright） |
| `ci/` | 生成（`gen.sh`）と、出す image に入ってよい物だけが入っているか・動くかを見る script |
| `docs/` | この本（mdBook。日本語だけ） |

## 定義と生成

callboard は自分の API の定義を持たない。castlist と pagetray の定義（Smithy）を、それぞれの repo の release の tag から取り、呼ぶ側の crate を作る。

- 取る版は `ci/gen.sh` の頭（`CASTLIST_TAG`・`PAGETRAY_TAG` と、file ごとの digest）に固めてある。`ci/gen.sh` は、取った file の digest が合わなければ何も替えない。
- 生成した crate は workspace の member だが、既定の命令（`default-members`）の外で clippy は掛けない（server の crate が引くので `cargo clippy` も見るが、`ci/gen.sh` が `lib.rs` の頭に `#![allow(clippy::all)]` を足す）。中の protocol の試験は `cargo test --locked -p castlist_client -p pagetray_client` で流す。fmt は生成器が `rust-toolchain.toml` の rustfmt で整えた物を、`cargo fmt --all --check` が見る。
- 生成器の版は `model/smithy-build.json`（Maven の物）と `ci/gen.sh`（Smithy の CLI と digest）に書く。CLI と trait の package の版は揃える。`smithy-build.json` に名指した Maven の物は、公開から 2 日たっていなければ `ci/gen.sh` が断る。
- pagetray の定義は、castlist の定義の共通の形（`ai.common`）を使う。pagetray のその版が固めている castlist の版と、callboard が固めた castlist の版が合わないと、生成が落ちる。その時は、2 つの版を揃えて上げる。

### castlist・pagetray の版を上げる時

1. `ci/gen.sh` の tag と digest を替える（digest は、その tag の `model/model/` の file の SHA-256）。`ci/upstream.env` の、呼ぶ先の image の digest と版も替える（`ci/smoke-image.sh` が、2 つの版が同じかを見る）。
2. `ci/gen.sh` を流し、取った定義と作り直した crate を、同じ commit に入れる。
3. 動いている castlist・pagetray が、その版より古くないことを見る（新しい操作を呼ぶと、古い server は 404 を返す）。

## 試験を先に書く

どの PR も、先に試験だけの commit を作り、落ちることを見てから、通る実装を足す（castlist・pagetray と同じ決まり）。

1. 試験だけの commit（`test: … を先に書く（まだ落ちる）`）。型と関数の形は先に置き、build が通らないだけの落ち方にしない（「期待と違う」で落ちるようにする）。
2. 通るまでの最小の実装を、次の commit にする。そのあと形を整える（試験は替えない）。
3. PR の本文に「先に書いた試験」の節を置き、落ちた写しと通った写しを載せる。

試験の層:

| 層 | 中身 | 在り処 |
|---|---|---|
| 受け入れ | 本物の binary を空いた port で起こし、HTTP で叩く（起きる・設定の誤りで起きない・止める合図で終わる） | `crates/server/tests/server.rs` |
| 道筋 | castlist・pagetray の代わりに、決めた答えを返す偽の server を process の中に立て、Router を直に呼ぶ。偽の server が受けた要求（path・header）も見る | `crates/server/tests/` の `guard.rs`（守りの層）・`health.rs`・`home.rs`（入口）・`profile.rs`（プロフィール）・`network.rs`（相関図と中継の道）（道具は `crates/server/tests/common/`） |
| browser | 出した版の castlist と、同じ commit の callboard を空いた port で起こし、castlist の API で見本を置き、Chromium で開く。相関図の点と線の数・押した先・絞り・色・console の誤りと CSP の違反が 0、プロフィールの中身、castlist の画面との新旧の比べを見る | `e2e/`（Playwright。CI の job `e2e`。Node の依存の弱点は job `advisories` の `pnpm audit`） |
| 単体 | 外に触らない純な関数 | 各 file の `#[cfg(test)]` |
| image | 空の image の中で起き、出した版の castlist と pagetray の image に名で届き、相関図と中継の道が本物の castlist を通して出て、断るべき物を断り、止める合図で終わる | `ci/smoke-image.sh <image>`（CI の job `image`） |

## 待たせない書き方

並列に来る問い合わせを、互いに待たせない。

- handler はみな `async`。tokio の糸を塞ぐ呼び出し（`std::thread::sleep`・`std::fs` の読み書き・同期の `TcpStream::connect` など）は、`clippy.toml` の `disallowed-methods` で断る（`cargo clippy` が誤りにする）。
- 鍵を握ったままの `await`（`clippy::await_holding_lock`）も誤りにする。
- 試験の道具のうち同期で書く物（`crates/server/tests/server.rs`）は、その file の頭で `disallowed_methods` の断りを外している。`crates/server/src/` では外さない。
- 呼ぶ先（castlist・pagetray）を待つ長さ（`upstream::TIMEOUT`）は、callboard 自身の時間切れ（`REQUEST_TIMEOUT`）より短くする。
