# 動かす

Apple Container（`container` CLI）で、release の image を動かす。host（callboard を動かす機械）の `127.0.0.1` だけに出し、LAN にもインターネットにも開かない。

> [!WARNING]
> callboard の container を起こす・止める・消す・版を上げるのは、roster では **DevOps Engineer 担当だけ**がする（repo の `.claude/roles/config.jsonc` の `infra.callboard` は `tpm`で、`--name callboard`・`container start callboard`・`container stop callboard` などに当たる命令は、ほかの担当には断られる）。ほかの担当は `roster ask devops` で頼む。

| 物 | 値 |
|---|---|
| image | `ghcr.io/uraitakahito/callboard:v<version>`（release に digest がある。arm64） |
| container | `callboard`（`127.0.0.1:8803` → 中の 8080。メモリは Apple Container の下限の 200 MiB、CPU は 1） |
| URL | host の browser から `http://127.0.0.1:8803/`（入口） |
| data | 無い（DB も volume も持たない） |
| 記録 | `container logs callboard`（stdout だけ。起きた時の行・止める合図・呼ぶ先に聞けなかった時の行・守りが断った要求の行。問い合わせごとの行は出さない） |

## 設定

環境変数で渡す。起きる時に 1 度読み、形が違えば、訳を出して起きない。

| 名 | 中身 | 既定 |
|---|---|---|
| `CALLBOARD_LISTEN` | 待ち受ける所 | `0.0.0.0:8080`（image の既定のままでよい） |
| `CALLBOARD_CASTLIST_URL` | castlist に聞く先 | 無い（必ず書く） |
| `CALLBOARD_PAGETRAY_URL` | pagetray に聞く先 | 無い（必ず書く） |
| `CALLBOARD_PAGETRAY_LINK_URL` | browser が pagetray の画面（まだ移していない物）と、置いたページを開く URL の頭 | `http://127.0.0.1:8801` |

- どれも `http://<host>:<port>` の形だけを受ける（path・`https://` は受けない）。
- 0.1.0 にあった `CALLBOARD_CASTLIST_LINK_URL`（castlist の画面を開く URL の頭）は、0.2.0 で無くした（castlist の画面をみな移したので、指す先が無い）。書いてあっても読まない。
- **聞く先と link の頭は別**。callboard は container の中から聞くので、聞く先は同じ network の container の名で書く（container の中の `127.0.0.1` は container 自身で、host の port には届かない）。link は人が host の browser で開くので、`127.0.0.1` の port で書く。
- castlist と pagetray へは、つなぐ先に関わらず `Host: 127.0.0.1` で送る（どちらも、`Host` が `127.0.0.1`・`localhost` でない要求を断る）。

## 起こす

```sh
container image pull ghcr.io/uraitakahito/callboard:v<version>
container run -d --name callboard -p 127.0.0.1:8803:8080 -m 200M -c 1 -e CALLBOARD_CASTLIST_URL=http://castlist.box:8080 -e CALLBOARD_PAGETRAY_URL=http://<pagetray に届く名>:8080 ghcr.io/uraitakahito/callboard:v<version>
```

- castlist は、container の名 `castlist.box` で届く（既定の network では、Apple Container の DNS が domain `box` の名を答える）。
- pagetray の container の名は `pagetray` で、domain の付かない名。Apple Container の DNS は、domain（`box`）の付いた名だけを答えるので、**この名では届かない**（0.1.0 を起こした時に確かめた: `Name does not resolve`）。届くようにするまで、入口の画面は pagetray を「届きません」と出す（callboard は起き、castlist の画面はみな使える。pagetray の画面は、まだ pagetray が出している）。
- **castlist は 1.1.0 から**（0.2.0 の一覧とタイムラインが、1.1.0 で足した欄と操作を使う）。古い castlist に向けると、タイムラインは「castlist に聞けません（404 が返りました）」の画面になり、一覧とプロフィールの「時刻ずれ」の印と、状態の横の出来事の名が出ない。

> [!WARNING]
> `-p` には必ず `127.0.0.1:` を書く。`-p 8803:8080` と書くと LAN に開く。

## 止める・起こし直す・版を上げる

```sh
container stop callboard
container start callboard
```

版を上げる時は、止めて消し、新しい版で「起こす」の `container run` を打つ。data を持たないので、写しも移行も無い。戻す時は、前の版で同じことをする。

```sh
container stop callboard
container delete callboard
```

- host の機械を起こし直すと止まったままになる。`container system start` の後に `container start callboard` を打ち、「確かめる」をする。castlist と pagetray より先に起こしてもよい（呼ぶ先が答えなくても起き、入口の画面に「届きません」と出す）。

## 確かめる

1. `container list` で `callboard` が `running`。
2. `curl -s http://127.0.0.1:8803/healthz` が `{"status":"ok","version":"<version>","now":"…"}` を返す（`version` が起こした版）。これは callboard 自身が生きているかで、castlist と pagetray には聞かない。
3. `lsof -nP -iTCP:8803 -sTCP:LISTEN` が `127.0.0.1:8803` だけを出す（`*:` で始まる行があれば LAN に開いている）。
4. browser で `http://127.0.0.1:8803/` を開き、入口の画面で、castlist と pagetray のどちらも「つながっています」と版が出る。「届きません」と出たら、その横の訳（つながりません・時間内に答えがありません・`<状態>` が返りました・答えを読めません）を見て、`container logs callboard` の `呼ぶ先に聞けない` の行（`upstream=` どの呼ぶ先か・`api=` 聞く先・`error=` 詳しい訳。名が引けないのか、port が違うのかは、ここで分かる）を読む。
5. 画面が 403 になる時は、`container logs callboard` の `refused` の行（`reason="host"` は `Host` が `127.0.0.1`・`localhost` でない、`reason="origin"` は `Origin` の付いた書く要求）を読む。
6. `container logs callboard` の頭の行が `callboard <version> listen=0.0.0.0:8080 castlist=<聞く先> pagetray=<聞く先>` で終わる。
