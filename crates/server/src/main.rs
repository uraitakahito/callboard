//! callboard の server を起こす binary（image の中の /callboard-server）。
//! - 設定は環境変数（[`callboard::config`]）。形が違えば、訳を出して起きない。
//! - 記録は stdout だけ（callboard は data も volume も持たない）。
//! - 止める合図（SIGTERM・SIGINT）で新しい問い合わせを断り、走っている物を終えてから閉じる。
use std::process::ExitCode;

use callboard::config::Settings;
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main]
async fn main() -> ExitCode {
    if std::env::args().len() > 1 {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    tracing_subscriber::fmt().with_ansi(false).init();
    match serve().await {
        Ok(()) => {
            tracing::info!("callboard stopped");
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!("callboard: {e}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "使い方: callboard-server（環境変数 CALLBOARD_LISTEN・CALLBOARD_CASTLIST_URL・CALLBOARD_CASTLIST_LINK_URL・CALLBOARD_PAGETRAY_URL・CALLBOARD_PAGETRAY_LINK_URL で起きる）";

/// 起きて、止める合図まで受け続ける。
async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::from_env()?;
    let listener = tokio::net::TcpListener::bind(&settings.listen).await?;
    // 待ち受けた所を出す（port を 0 にして空いた port を取らせた時も、取れた port が読める）。
    tracing::info!(
        "callboard {} listen={} castlist={} pagetray={}",
        env!("CARGO_PKG_VERSION"),
        listener.local_addr()?,
        settings.castlist.api,
        settings.pagetray.api,
    );

    // container の中では PID 1 になる。PID 1 は、受け口を作らないと止める合図を無視する。
    // 合図が来たら新しい問い合わせを断り、走っている物を終えてから閉じる。
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    axum::serve(listener, callboard::app(&settings))
        .with_graceful_shutdown(async move {
            let name = tokio::select! {
                _ = term.recv() => "SIGTERM",
                _ = int.recv() => "SIGINT",
            };
            tracing::info!("signal {name}");
        })
        .await?;
    Ok(())
}
