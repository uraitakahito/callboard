//! pagetray を呼ぶ所（決まりは [`super`]）。
use pagetray_client::Client;
use pagetray_client::config::retry::RetryConfig;
use pagetray_client::config::timeout::TimeoutConfig;
use pagetray_client::config::{BehaviorVersion, Config};

use super::{Health, TIMEOUT, http_client, reason, within};
use crate::config::Base;

/// pagetray の居場所と、呼ぶ client。
#[derive(Clone, Debug)]
pub struct Pagetray {
    /// browser が pagetray を開く URL の頭（`http://<host>:<port>`）。
    link: String,
    client: Client,
}

impl Pagetray {
    pub fn new(base: &Base) -> Self {
        let config = Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(&base.api)
            .http_client(http_client())
            .retry_config(RetryConfig::disabled())
            .timeout_config(TimeoutConfig::builder().operation_timeout(TIMEOUT).build())
            .build();
        Self {
            link: base.link.clone(),
            client: Client::from_conf(config),
        }
    }

    /// browser が pagetray を開く URL の頭（`http://<host>:<port>`）。
    pub fn link(&self) -> &str {
        &self.link
    }

    /// 生きているかを聞く（GetHealth）。聞けなければ、その訳。
    pub async fn health(&self) -> Result<Health, String> {
        within(async {
            let output = self
                .client
                .get_health()
                .customize()
                .mutate_request(|request| {
                    request.headers_mut().insert("host", "127.0.0.1");
                })
                .send()
                .await
                .map_err(|e| reason("pagetray", &e))?;
            // 生成した client は、答えに無い `@required` の欄を既定の値（空の字）で埋める。status が ok でない答えは、読めない物とする。
            if output.status() != "ok" || output.version().is_empty() {
                return Err("答えを読めません".to_owned());
            }
            Ok(Health {
                version: output.version().to_owned(),
            })
        })
        .await
    }
}
