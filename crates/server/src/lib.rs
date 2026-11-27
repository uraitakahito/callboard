//! callboard の server: castlist（アクターの台帳）と pagetray（ページの置き場）の画面をまとめる。
//! まだ形だけ（道は無い）。
pub mod config;
pub mod upstream;

use std::time::Duration;

use axum::Router;

use crate::config::Settings;

/// 1 つの問い合わせにかけてよい長さ。
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// 本文の上限。
pub const BODY_LIMIT: usize = 2 * 1024 * 1024;

/// 守りの層の設定。
#[derive(Clone)]
pub struct Config {
    /// 1 つの問い合わせにかけてよい長さ。
    pub timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            timeout: REQUEST_TIMEOUT,
        }
    }
}

/// 動かす Router。
pub fn app(settings: &Settings) -> Router {
    app_with(settings, Config::default())
}

/// 動かす Router（守りの層の設定を渡す）。まだ道は無い。
pub fn app_with(_settings: &Settings, _config: Config) -> Router {
    Router::new()
}
