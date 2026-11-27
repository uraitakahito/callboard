//! pagetray を呼ぶ所（決まりは [`super`]）。
use pagetray_client::Client;
use pagetray_client::config::retry::RetryConfig;
use pagetray_client::config::timeout::TimeoutConfig;
use pagetray_client::config::{BehaviorVersion, Config};

use super::{Health, TIMEOUT, http_client, reason, unreadable, within};
use crate::config::Base;

/// 記録に書く、呼ぶ先の名。
const NAME: &str = "pagetray";

/// pagetray の居場所と、呼ぶ client。
#[derive(Clone, Debug)]
pub struct Pagetray {
    /// 聞く先（`http://<host>:<port>`）。
    api: String,
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
            api: base.api.clone(),
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
        within(NAME, &self.api, async {
            let output = self
                .client
                .get_health()
                .customize()
                .mutate_request(|request| {
                    request.headers_mut().insert("host", "127.0.0.1");
                })
                .send()
                .await
                .map_err(|e| reason(NAME, &self.api, &e))?;
            // 生成した client は、答えに無い `@required` の欄を既定の値（空の字）で埋める。status が ok でない答えは、読めない物とする。
            if output.status() != "ok" || output.version().is_empty() {
                return Err(unreadable(NAME, &self.api));
            }
            Ok(Health {
                version: output.version().to_owned(),
            })
        })
        .await
    }
}
