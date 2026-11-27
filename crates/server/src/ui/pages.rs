//! ページの一覧（`GET /pages/`）と、file を上げる道（`POST /api/pagetray/projects/{project}/keys/{key}/files`）。
//! pagetray の画面（pagetray の `/`）から移した物。
//!
//! - ページは pagetray が持つ。callboard は、pagetray の API（ListPages・GetPageFacets・ListReviews と、Key の画面では
//!   ListFiles・GetFileRules）で読んで出す。ページの本文と file の中身は、browser が pagetray から直に読む
//!   （本文の枠の `<iframe>` と図の `src` は、pagetray の link の頭で組む。この画面の CSP だけが、それを許す）。
//! - 絞り込み・並べ替え・検索は link と form で動き、条件は URL に出る。引数の名と値は pagetray の画面と同じ
//!   （`project`・`key`・`kind`・`role`・`workspace`・`q`・`reply`・`on`・`verdict`・`actor`・`sort=created_at`・`at`（続きの印。
//!   ListPages の `nextToken`）・`open`（本文の枠に出すページの path））。空の値・知らない値は、書かないのと同じ
//!   （workspace だけは、空も条件）。値の形は pagetray が確かめる（断られたら、pagetray の訳を書いた 400 の画面）。
//! - 待ちの数と絞り込みの候補は、絞りに関わらず置き場の全部（GetPageFacets の答えのまま）。
//! - Key の画面（project と Key を 1 つずつ選んだ時）だけ、file の枠と上げる枠を出す。
//! - 字はみな HTML として逃がす。script（`static/pages.js`）が無くても、一覧は動く（上げる枠だけは script が要る）。
use std::sync::Arc;

use askama::Template;
use axum::body::Body;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::i18n::filters;
use super::messages::EVENTS_PATH;
use super::params::{self, PageError, Pairs, bad_request, strict};
use super::time::{Time, short};
use super::{CSS_HREF, Nav, PAGES_JS_HREF, error_page};
use crate::App;
use crate::errors::reply;
use crate::upstream::Failure;
use crate::upstream::pagetray::{FacetCount, File, Page, PageSearch, ReviewOf, Sender};

/// 一覧の道。
pub const PATH: &str = "/pages/";

/// 画面の form が file を上げる道（`{project}`・`{key}` は、Key の画面の project と Key）。
pub const UPLOAD_ROUTE: &str = "/api/pagetray/projects/{project}/keys/{key}/files";

/// 1 度に出す数（pagetray の画面と同じ）。
const PAGE_SIZE: i32 = 200;

/// レビューの判定の字（pagetray の定義の `Verdict`）と、画面に出す字。
const VERDICTS: [(&str, &str); 4] = [
    ("approve", "よい"),
    ("concerns", "気になる所あり"),
    ("reject", "直すべき"),
    ("not-assessed", "見られなかった"),
];

/// この画面の答えに付ける Content-Security-Policy。ほかの画面の物（[`crate::CSP`]）に、pagetray の link の頭を足す:
/// 本文の枠（`frame-src`）と、file の枠の図（`img-src`）。`blob:` は、上げる前の図の見本。
pub fn csp(pagetray: &str) -> HeaderValue {
    let csp = format!(
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob: {pagetray}; connect-src 'self'; frame-src {pagetray}; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
    );
    // link の頭は、設定を読む時に `http://<host>:<port>` の形を見てある。
    HeaderValue::from_str(&csp).unwrap_or_else(|_| HeaderValue::from_static(crate::CSP))
}

/// 画面の引数（確かめた値だけ）。
#[derive(Clone, Default)]
struct View {
    project: Option<String>,
    key: Option<String>,
    kind: Option<String>,
    role: Option<String>,
    workspace: Option<String>,
    q: Option<String>,
    /// 返事の状態（`waiting`・`answered`）。
    reply: Option<String>,
    /// 返事の状態の、待ちの相手（`review`・`tpm`。Product Manager は書かない）。
    on: Option<String>,
    /// レビューの判定。
    verdict: Option<String>,
    /// 送り手（castlist のアクターの id）。
    actor: Option<String>,
    /// `created_at`（古い順）か、無い（新しい順）。
    sort: Option<String>,
    /// 並びの続きの印（前の頁の答えの `nextToken`）。
    at: Option<String>,
    /// 本文の枠に出すページの path。
    open: Option<String>,
}

impl View {
    /// 引数の名と、その値の置き場（URL に出す順）。
    fn fields(&mut self) -> [(&'static str, &mut Option<String>); 13] {
        [
            ("project", &mut self.project),
            ("key", &mut self.key),
            ("kind", &mut self.kind),
            ("role", &mut self.role),
            ("workspace", &mut self.workspace),
            ("q", &mut self.q),
            ("reply", &mut self.reply),
            ("on", &mut self.on),
            ("verdict", &mut self.verdict),
            ("actor", &mut self.actor),
            ("sort", &mut self.sort),
            ("at", &mut self.at),
            ("open", &mut self.open),
        ]
    }

    /// 名と値の並びから読む。同じ名を 2 度書いた物は 400（pagetray の画面と同じ）。知らない名は読まない。
    /// 空の値・知らない値は、書かないのと同じにする（workspace だけは、空も条件）。
    fn read(pairs: Pairs) -> Result<Self, PageError> {
        let mut view = Self::default();
        let mut seen = Vec::new();
        for (name, value) in pairs {
            let Some((_, field)) = view.fields().into_iter().find(|(n, _)| *n == name) else {
                continue;
            };
            if seen.contains(&name) {
                return Err(bad_request(format!(
                    "引数を読めません（{name} を 2 度書いています）"
                )));
            }
            *field = Some(value).filter(|v| name == "workspace" || !v.trim().is_empty());
            seen.push(name);
        }
        view.reply = view
            .reply
            .filter(|r| matches!(r.as_str(), "waiting" | "answered"));
        // 待ちの相手は、返事の状態と組の時だけ。Product Manager は書かない。
        view.on = view
            .on
            .filter(|on| view.reply.is_some() && matches!(on.as_str(), "review" | "tpm"));
        view.verdict = view.verdict.filter(|v| verdict_label(v).is_some());
        view.sort = view.sort.filter(|s| s == "created_at");
        view.open = view.open.filter(|path| page_parts(path).is_some());
        Ok(view)
    }

    fn oldest_first(&self) -> bool {
        self.sort.is_some()
    }

    /// この条件の一覧の URL。空の条件は出さない。
    fn href(&self) -> String {
        let mut view = self.clone();
        let query: Vec<String> = view
            .fields()
            .into_iter()
            .filter_map(|(name, value)| Some(format!("{name}={}", strict(value.as_deref()?))))
            .collect();
        if query.is_empty() {
            PATH.to_owned()
        } else {
            format!("{PATH}?{}", query.join("&"))
        }
    }

    /// 条件を 1 つ変えた一覧の URL。並びの続きの印は、条件が変わると意味が無いので外す。
    fn with(&self, change: impl FnOnce(&mut View)) -> String {
        let mut next = self.clone();
        next.at = None;
        change(&mut next);
        next.href()
    }

    /// 本文の枠に `path` のページを出した、今の一覧の URL（並びの続きの印も保つ）。
    fn opening(&self, path: &str) -> String {
        let mut next = self.clone();
        next.open = Some(path.to_owned());
        next.href()
    }
}

/// URL の 1 段になる字（project・Key）: 英数字と `.`・`_`・`-` で 255 字まで。`.` と `..` は除く。
fn is_segment(s: &str) -> bool {
    (1..=255).contains(&s.len())
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// ページの path（`/<project>/<key>/<kind>-<n>.html`）を、project・Key・種類・番号の字に分ける。形が違えば None
/// （pagetray の `parse_page_path` と同じ決まり。番号は、頭に 0 の無い数）。
fn page_parts(path: &str) -> Option<(&str, &str, &str, &str)> {
    let mut parts = path.strip_prefix('/')?.split('/');
    let (project, key, file) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || !is_segment(project) || !is_segment(key) {
        return None;
    }
    let (kind, n) = file.strip_suffix(".html")?.rsplit_once('-')?;
    let name = (1..=32).contains(&kind.len())
        && kind.as_bytes()[0].is_ascii_lowercase()
        && kind
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    let number = !n.is_empty()
        && n.len() <= 18
        && !n.starts_with('0')
        && n.bytes().all(|b| b.is_ascii_digit());
    (name && number).then_some((project, key, kind, n))
}

/// レビューの判定の名（'static にする）と、画面に出す字。知らない字なら None。
fn verdict_label(verdict: &str) -> Option<(&'static str, &'static str)> {
    VERDICTS.into_iter().find(|(name, _)| *name == verdict)
}

/// 返事の状態と待ちの相手の、画面に出す字。
fn reply_label(answered: bool, on: &str) -> &'static str {
    match (answered, on) {
        (false, "review") => "レビュー待ち",
        (true, "review") => "レビュー済み",
        (false, "tpm") => "上げた",
        (true, "tpm") => "決まった",
        (false, _) => "返事待ち",
        (true, _) => "答えた",
    }
}

/// 絞り込みの列の、候補 1 つ。
struct Choice {
    label: String,
    /// 候補に添える字（workspace の Key）。無ければ空。
    note: String,
    count: i64,
    href: String,
    active: bool,
}

/// 絞り込みの列の、見出し 1 つと、その候補。
struct FilterGroup {
    title: &'static str,
    choices: Vec<Choice>,
}

/// 操作の帯の、待ちの数の釦 1 つ（押すと、その待ちだけに絞る）。数が 0 なら隠す。
struct Counter {
    id: &'static str,
    label: &'static str,
    title: &'static str,
    count: i64,
    href: String,
    active: bool,
}

/// 行の下に出す link 1 つ。置き場のページなら一覧の中の本文の枠に、ほかの URL なら別の tab で開く。
struct LinkView {
    href: String,
    label: String,
    external: bool,
}

/// 見られたページの行に出す、そのページを見たレビュー 1 つ。
struct ReviewView {
    link: LinkView,
    verdict: &'static str,
    verdict_label: &'static str,
}

/// 行に出す送り手: castlist の名（callboard のプロフィールへの link）と、その下に小さく付随する id。
struct SenderView {
    /// castlist の名。castlist の答えが無ければ「未確認」。
    name: String,
    /// アクターのプロフィール（`/actors/<id>`）。castlist の答えが無ければ無い。
    href: Option<String>,
    /// 付随する id。
    reference: String,
}

fn sender_view(sender: Option<Sender>) -> Option<SenderView> {
    let sender = sender?;
    let (name, href) = match sender.actor {
        Some(actor) => (actor.name, Some(format!("/actors/{}", strict(&actor.id)))),
        None => ("未確認".to_owned(), None),
    };
    Some(SenderView {
        name,
        href,
        reference: sender.r#ref,
    })
}

/// 一覧の列の、ページ 1 枚分。
struct Row {
    path: String,
    href: String,
    active: bool,
    key: String,
    /// `<kind>-<n>`。
    name: String,
    title: String,
    role: String,
    workspace: String,
    created: Time,
    /// 返事の状態の名（`waiting`・`answered`）。返事を求めていないページは空。
    reply: &'static str,
    /// 待ちの相手（`review`・`tpm`）。Product Manager と、返事を求めていないページは空。
    on: &'static str,
    reply_label: &'static str,
    /// レビューの判定の名と字。判定の無いページは空。
    verdict: &'static str,
    verdict_label: &'static str,
    /// 見た物（レビューのページ）・見る物（ほかのページ）の見出しと link。
    targets_label: &'static str,
    targets: Vec<LinkView>,
    /// このページを見たレビュー（古い順）。
    reviews: Vec<ReviewView>,
    sender: Option<SenderView>,
}

/// file の枠の、file 1 つ分。
struct FileItem {
    path: String,
    /// `<語>-<n>.<拡張子>`。
    name: String,
    /// 同じ Key のページから参照する時の、相対の path（`files/<語>-<n>.<拡張子>`）。
    reference: String,
    ext: String,
    image: bool,
    size: String,
    role: String,
    created: Time,
    sender: Option<SenderView>,
}

/// Key の画面だけに出す物（file の枠と上げる枠）。
struct KeyView {
    project: String,
    key: String,
    files: Vec<FileItem>,
    /// 上げられる拡張子と Content-Type の組の並び（JSON）。script が送る前に見る。
    types: String,
    /// `accept` に書く拡張子（`.png,.jpg,…`）。
    accept: String,
    max: i64,
    /// 上げる枠の script が送る道（名は script が `name` の引数で足す）。
    upload: String,
}

#[derive(Template)]
#[template(path = "pages.html")]
struct PagesPage<'a> {
    css: &'static str,
    nav: &'a Nav,
    script: &'static str,
    events: &'static str,
    /// 本文の枠・file の link と図の URL の頭（pagetray の link の頭）。
    store: &'a str,
    q: String,
    hidden: Vec<(&'static str, String)>,
    sort_href: String,
    sort_label: &'static str,
    reload_href: String,
    reset_href: String,
    filtered: bool,
    /// Product Manager の返事待ちのページの数（絞り込みに関わらず、置き場の全部）。tab の題に出す。
    waiting: i64,
    counters: Vec<Counter>,
    filters: Vec<FilterGroup>,
    rows: Vec<Row>,
    more_href: Option<String>,
    /// 本文の枠に出すページの path。
    viewer: Option<String>,
    key_view: Option<KeyView>,
}

/// `GET /pages/`。
pub async fn pages(
    State(app): State<Arc<App>>,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Response {
    match page(&app, q).await {
        Ok(page) => match page.render() {
            Ok(html) => (
                [(header::CONTENT_SECURITY_POLICY, app.pages_csp.clone())],
                axum::response::Html(html),
            )
                .into_response(),
            Err(e) => {
                tracing::error!("画面を表示できません: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "画面を表示できません").into_response()
            }
        },
        Err(PageError(status, message)) => error_page(&app.nav, status, &message),
    }
}

/// `GET /pages`: 一覧（`/pages/`）へ送る。引数は、そのまま運ぶ。
pub async fn redirect(axum::extract::RawQuery(query): axum::extract::RawQuery) -> Response {
    let to = match query {
        Some(query) => format!("{PATH}?{query}"),
        None => PATH.to_owned(),
    };
    axum::response::Redirect::permanent(&to).into_response()
}

/// `POST /api/pagetray/projects/{project}/keys/{key}/files?name=…`: 画面の form が上げた file を、pagetray に置く。
/// この道に来るのは、この callboard の画面の form からの要求だけ（守りの層 [`crate::guard`] が見る）。
pub async fn upload(
    State(app): State<Arc<App>>,
    Path((project, key)): Path<(String, String)>,
    q: Result<Query<Pairs>, QueryRejection>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let invalid = |message: &str| reply(StatusCode::BAD_REQUEST, "ValidationException", message);
    if !is_segment(&project) || !is_segment(&key) || project == "api" {
        return invalid("project と Key の形が違う");
    }
    // browser から受けるのは、名だけ（役割・workspace・番号・時刻・付随する id は読まない）。
    let name = q.ok().and_then(|Query(pairs)| {
        pairs
            .into_iter()
            .find(|(name, _)| name == "name")
            .map(|(_, value)| value)
    });
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return invalid("name（file の名）が要る");
    };
    app.pagetray
        .upload(
            &project,
            &key,
            &name,
            headers.get(header::CONTENT_TYPE).cloned(),
            body,
        )
        .await
}

/// pagetray に聞けなかった訳を、画面の誤りにする。
fn failed(e: Failure) -> PageError {
    match e {
        Failure::Rejected(message) => {
            bad_request(format!("pagetray が引数を断りました: {message}"))
        }
        Failure::NotFound => bad_request("pagetray が引数を断りました"),
        Failure::Unavailable(why) => PageError(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("pagetray に聞けません（{why}）。少し待ってから開き直してください。"),
        ),
    }
}

/// 条件を読み、pagetray に聞き、画面を組む。
async fn page(
    app: &App,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Result<PagesPage<'_>, PageError> {
    let view = View::read(params::pairs(q).map_err(|_| bad_request("引数を読めません"))?)?;
    let oldest_first = view.oldest_first();
    let search = PageSearch {
        project: view.project.clone(),
        key: view.key.clone(),
        kind: view.kind.clone(),
        role: view.role.clone(),
        workspace: view.workspace.clone(),
        q: view.q.clone(),
        reply: view.reply.clone(),
        on: view.on.clone(),
        verdict: view.verdict.clone(),
        actor: view.actor.clone(),
        oldest: oldest_first,
        after: view.at.clone(),
        limit: PAGE_SIZE,
    };
    // Key の画面（project と Key を 1 つずつ選んだ時。project の api には file を置けない）だけ、file と受ける種類を聞く。
    let key_of = match (&view.project, &view.key) {
        (Some(project), Some(key))
            if is_segment(project) && is_segment(key) && project != "api" =>
        {
            Some((project.clone(), key.clone()))
        }
        _ => None,
    };
    let key_files = async {
        match &key_of {
            Some((project, key)) => {
                let (files, rules) = tokio::join!(
                    app.pagetray.files(project, key, PAGE_SIZE),
                    app.pagetray.file_rules()
                );
                Ok(Some((files?, rules?)))
            }
            None => Ok(None),
        }
    };
    let (found, facets, key_files) = tokio::join!(
        app.pagetray.pages(&search),
        app.pagetray.page_facets(),
        key_files
    );
    let (found, facets, key_files) = (
        found.map_err(failed)?,
        facets.map_err(failed)?,
        key_files.map_err(failed)?,
    );
    let reviews = app
        .pagetray
        .reviews(found.pages.iter().map(|p| p.path.clone()).collect())
        .await
        .map_err(failed)?;

    let more_href = found.next_token.map(|at| view.with(|v| v.at = Some(at)));
    // 候補の並び。押すと今の条件に重ね、選んである候補をもう 1 度押すと外す。`show` が、候補の字と添える字を決める。
    let group = |title,
                 counts: Vec<FacetCount>,
                 field: fn(&mut View) -> &mut Option<String>,
                 show: fn(&FacetCount) -> (String, String)| FilterGroup {
        title,
        choices: counts
            .into_iter()
            .map(|c| {
                let mut probe = view.clone();
                let active = field(&mut probe).as_deref() == Some(c.value.as_str());
                let (label, note) = show(&c);
                let next = (!active).then_some(c.value);
                Choice {
                    label,
                    note,
                    count: c.count,
                    href: view.with(|v| *field(v) = next),
                    active,
                }
            })
            .collect(),
    };
    // 値そのものを出す候補（空の値は「（不明）」）と、添える字。
    let plain: fn(&FacetCount) -> (String, String) = |c| {
        let label = if c.value.is_empty() {
            "（不明）".to_owned()
        } else {
            c.value.clone()
        };
        (label, c.note.clone().unwrap_or_default())
    };
    let waits = |reply: &str, on: &str| {
        view.reply.as_deref() == Some(reply) && view.on.as_deref().unwrap_or("") == on
    };
    // 返事の候補は、値でなく画面の字で出す。件数が 0 の候補は出さない。
    let replies = FilterGroup {
        title: "返事",
        choices: [
            ("waiting", "", facets.waiting),
            ("answered", "", facets.answered),
            ("waiting", "review", facets.review_waiting),
            ("waiting", "tpm", facets.escalated),
        ]
        .into_iter()
        .filter(|(_, _, count)| *count > 0)
        .map(|(state, on, count)| {
            let active = waits(state, on);
            Choice {
                label: reply_label(state == "answered", on).to_owned(),
                note: String::new(),
                count,
                href: view.with(|v| {
                    v.reply = (!active).then(|| state.to_owned());
                    v.on = (!active && !on.is_empty()).then(|| on.to_owned());
                }),
                active,
            }
        })
        .collect(),
    };
    // 操作の帯の釦は、ほかの絞り込みを外して、その待ちだけに絞る（並びは保つ）。
    let counter = |id, label, title, on: &str, count| Counter {
        id,
        label,
        title,
        count,
        href: View {
            reply: Some("waiting".to_owned()),
            on: (!on.is_empty()).then(|| on.to_owned()),
            sort: view.sort.clone(),
            ..View::default()
        }
        .href(),
        active: waits("waiting", on),
    };
    let key_view = key_of
        .zip(key_files)
        .map(|((project, key), (files, rules))| KeyView {
            upload: format!("/api/pagetray/projects/{project}/keys/{key}/files"),
            files: files.into_iter().map(file_item).collect(),
            types: serde_json::to_string(
                &rules
                    .types
                    .iter()
                    .map(|t| (t.ext.as_str(), t.r#type.as_str()))
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_default(),
            accept: rules
                .types
                .iter()
                .map(|t| format!(".{}", t.ext))
                .collect::<Vec<_>>()
                .join(","),
            max: rules.max_bytes,
            project,
            key,
        });
    let mut hidden_view = view.clone();
    let hidden = hidden_view
        .fields()
        .into_iter()
        .filter(|(name, _)| !matches!(*name, "q" | "at" | "open"))
        .filter_map(|(name, value)| Some((name, value.clone()?)))
        .collect();
    Ok(PagesPage {
        css: &CSS_HREF,
        nav: &app.nav,
        script: &PAGES_JS_HREF,
        events: EVENTS_PATH,
        store: app.pagetray.link(),
        q: view.q.clone().unwrap_or_default(),
        hidden,
        sort_href: view.with(|v| v.sort = (!oldest_first).then(|| "created_at".to_owned())),
        sort_label: if oldest_first {
            "古い順"
        } else {
            "新しい順"
        },
        reload_href: view.with(|_| {}),
        reset_href: View {
            sort: view.sort.clone(),
            ..View::default()
        }
        .href(),
        filtered: [
            &view.project,
            &view.key,
            &view.kind,
            &view.role,
            &view.workspace,
            &view.q,
            &view.reply,
            &view.verdict,
            &view.actor,
        ]
        .iter()
        .any(|v| v.is_some()),
        waiting: facets.waiting,
        counters: vec![
            counter(
                "waiting",
                "返事待ち",
                "Product Manager の返事を待っているページだけに絞る",
                "",
                facets.waiting,
            ),
            counter(
                "review-waiting",
                "レビュー待ち",
                "レビューの判定を待っている依頼書だけに絞る",
                "review",
                facets.review_waiting,
            ),
            counter(
                "escalated",
                "上げた",
                "判定を受けずに TPM 担当へ上げ、まだ決まっていないレビューだけに絞る",
                "tpm",
                facets.escalated,
            ),
        ],
        filters: vec![
            replies,
            // 判定と送り手の候補は、値でなく画面の字（判定の字・castlist の名）で出す。
            group(
                "判定",
                facets.verdicts,
                |v| &mut v.verdict,
                |c| {
                    (
                        verdict_label(&c.value).map_or("", |(_, l)| l).to_owned(),
                        String::new(),
                    )
                },
            ),
            group(
                "送り手",
                facets.senders,
                |v| &mut v.actor,
                |c| (c.note.clone().unwrap_or_default(), String::new()),
            ),
            group("担当者（役割）", facets.roles, |v| &mut v.role, plain),
            group("workspace", facets.workspaces, |v| &mut v.workspace, plain),
            group("種類", facets.kinds, |v| &mut v.kind, plain),
            group("Key", facets.keys, |v| &mut v.key, plain),
            group("project", facets.projects, |v| &mut v.project, plain),
        ],
        rows: found
            .pages
            .into_iter()
            .map(|p| row(&view, &reviews, p))
            .collect(),
        more_href,
        viewer: view.open.clone(),
        key_view,
    })
}

/// 見た物の link。置き場のページは、行と同じく一覧の中の本文の枠で開く。字は、同じ Key なら `<kind>-<n>`、同じ project なら
/// `<Key>/<kind>-<n>`。PR の URL（`…/<repo>/pull/<番号>`）は `<repo>#<番号>`。ほかの URL はそのまま。
fn link_view(view: &View, project: &str, key: &str, target: &str) -> LinkView {
    if let Some((p, k, kind, n)) = page_parts(target) {
        let label = match (p == project, k == key) {
            (true, true) => format!("{kind}-{n}"),
            (true, false) => format!("{k}/{kind}-{n}"),
            _ => format!("{p}/{k}/{kind}-{n}"),
        };
        return LinkView {
            href: view.opening(target),
            label,
            external: false,
        };
    }
    let pr = target.split_once("/pull/").and_then(|(head, rest)| {
        let repo = head.rsplit('/').next().filter(|r| !r.is_empty())?;
        let n = rest.split(['/', '?', '#']).next()?;
        (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then(|| format!("{repo}#{n}"))
    });
    LinkView {
        href: target.to_owned(),
        label: pr.unwrap_or_else(|| target.to_owned()),
        external: true,
    }
}

fn row(view: &View, reviews: &[ReviewOf], p: Page) -> Row {
    let answered = match (&p.asked_at, &p.answered_at) {
        (_, Some(_)) => Some(true),
        (Some(_), None) => Some(false),
        (None, None) => None,
    };
    // 待ちの相手（`review`・`tpm`）。Product Manager の待ちには無い。
    let on = match p.waits_on.as_deref() {
        Some("review") => "review",
        Some("tpm") => "tpm",
        _ => "",
    };
    let (verdict, verdict_label_) = p
        .verdict
        .as_deref()
        .and_then(verdict_label)
        .unwrap_or(("", ""));
    let targets = p
        .targets
        .iter()
        .flatten()
        .map(|t| link_view(view, &p.project, &p.key, t))
        .collect();
    let reviews = reviews
        .iter()
        .filter(|r| r.target == p.path)
        .map(|r| {
            let (verdict, verdict_label) = r
                .verdict
                .as_deref()
                .and_then(verdict_label)
                .unwrap_or(("", ""));
            ReviewView {
                link: link_view(view, &p.project, &p.key, &r.path),
                verdict,
                verdict_label,
            }
        })
        .collect();
    Row {
        href: view.opening(&p.path),
        active: view.open.as_deref() == Some(p.path.as_str()),
        targets_label: if p.kind == "review" {
            "見た物"
        } else {
            "見る物"
        },
        name: format!("{}-{}", p.kind, p.n),
        created: short(&p.created_at),
        reply: match answered {
            Some(true) => "answered",
            Some(false) => "waiting",
            None => "",
        },
        on: if answered.is_some() { on } else { "" },
        reply_label: answered.map_or("", |a| reply_label(a, on)),
        verdict,
        verdict_label: verdict_label_,
        targets,
        reviews,
        sender: sender_view(p.sender),
        path: p.path,
        key: p.key,
        title: p.title,
        role: p.role,
        workspace: p.workspace,
    }
}

fn file_item(f: File) -> FileItem {
    let name = format!("{}-{}.{}", f.name, f.n, f.ext);
    FileItem {
        reference: format!("files/{name}"),
        name,
        image: f.r#type.starts_with("image/"),
        size: size(f.size),
        created: short(&f.created_at),
        sender: sender_view(f.sender),
        path: f.path,
        ext: f.ext,
        role: f.role,
    }
}

/// 人が読む大きさ（`512 B`・`47.1 KiB`・`3.2 MiB`）。
fn size(bytes: i64) -> String {
    const KIB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KIB {
        format!("{bytes} B")
    } else if b < KIB * KIB {
        format!("{:.1} KiB", b / KIB)
    } else {
        format!("{:.1} MiB", b / KIB / KIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs<const N: usize>(pairs: [(&str, &str); N]) -> Pairs {
        pairs
            .into_iter()
            .map(|(n, v)| (n.to_owned(), v.to_owned()))
            .collect()
    }

    #[test]
    fn link_は条件を運び_空の条件は出さない() {
        let view = View::read(pairs([
            ("key", "a b?c&d#e/f"),
            ("workspace", ""),
            ("sort", "created_at"),
            ("at", "6d65"),
        ]))
        .ok()
        .unwrap();
        assert_eq!(
            view.href(),
            "/pages/?key=a%20b%3Fc%26d%23e%2Ff&workspace=&sort=created_at&at=6d65"
        );
        assert_eq!(
            view.with(|v| v.key = None),
            "/pages/?workspace=&sort=created_at"
        );
        assert_eq!(
            view.opening("/p/K/plan-1.html"),
            "/pages/?key=a%20b%3Fc%26d%23e%2Ff&workspace=&sort=created_at&at=6d65&open=%2Fp%2FK%2Fplan-1.html"
        );
        assert_eq!(View::default().href(), "/pages/");
    }

    #[test]
    fn ページの_path_の形() {
        assert_eq!(
            page_parts("/p/A-1/plan-12.html"),
            Some(("p", "A-1", "plan", "12"))
        );
        assert_eq!(
            page_parts("/p/A-1/pm-answer-3.html"),
            Some(("p", "A-1", "pm-answer", "3"))
        );
        for bad in [
            "p/A-1/plan-1.html",
            "/p/A-1/plan-01.html",
            "/p/A-1/plan-.html",
            "/p/A-1/Plan-1.html",
            "/p/A-1/plan-1.htm",
            "/p/A-1/files/a-1.png",
            "/p/../plan-1.html",
            "/p/A-1/x/plan-1.html",
            "https://example.com/p/A-1/plan-1.html",
        ] {
            assert_eq!(page_parts(bad), None, "{bad}");
        }
    }

    #[test]
    fn 大きさは_b_kib_mib_で読む() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(48213), "47.1 KiB");
        assert_eq!(size(3 * 1024 * 1024 + 200 * 1024), "3.2 MiB");
    }
}
