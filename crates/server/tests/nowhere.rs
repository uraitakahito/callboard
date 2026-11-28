//! 試験の道具の「だれも待っていない所」（`common::nowhere`）が、この機械で使えるか（AI-56）。
mod common;

use std::time::{Duration, Instant};

use common::{NOWHERE, assert_refused, nowhere};

#[tokio::test]
async fn だれも待っていない所は_番号_1_で_つなぐとすぐ断られる() {
    assert_eq!(nowhere().await, "http://127.0.0.1:1");
    let started = Instant::now();
    assert_refused(NOWHERE);
    let took = started.elapsed();
    assert!(
        took < Duration::from_millis(500),
        "断られるまで {took:?} かかった"
    );
}

#[test]
#[should_panic(expected = "で何かが待ち受けている")]
fn 待ち受ける物があれば_試験は揺れずに落ちて知らせる() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    assert_refused(&listener.local_addr().unwrap().to_string());
}
