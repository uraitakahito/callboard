//! 呼ぶ先（castlist・pagetray）。まだ形だけ。
use std::time::Duration;

/// 1 度に待つ長さ。
pub const TIMEOUT: Duration = Duration::from_secs(4);

/// 生きているかを聞く時に待つ長さ。
pub const HEALTH_TIMEOUT: Duration = Duration::from_secs(1);
