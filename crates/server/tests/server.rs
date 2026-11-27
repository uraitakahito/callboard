//! 受け入れの試験: 本物の binary（callboard-server）を空いた port で起こし、生の HTTP/1.1 で叩く。
// 試験の道具は同期で書く（async の中ではないので、糸を塞いでも困らない）。
#![allow(clippy::disallowed_methods)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 起こす binary。
fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_callboard-server")
}

/// だれも待っていない聞く先（この試験は、呼ぶ先が止まっている形で起こす）。
const NOWHERE: &str = "http://127.0.0.1:9";

/// 設定の環境変数を付けた命令（外の `CALLBOARD_…` は渡さない）。
fn command(env: &[(&str, &str)]) -> Command {
    let mut command = Command::new(binary());
    for (name, _) in std::env::vars() {
        if name.starts_with("CALLBOARD_") {
            command.env_remove(name);
        }
    }
    command.envs(env.iter().copied());
    command
}

/// 起こした callboard-server。落とす時に止める。
struct Server {
    child: Child,
    port: u16,
    /// 起きた時の 1 行（`callboard <版> listen=… castlist=… pagetray=…`）。
    line: String,
    /// その後に stdout に出た行。
    lines: Arc<Mutex<Vec<String>>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    /// port を 0 にして起こし、stdout に出た待ち受けの行から、取れた port を読む。
    fn start() -> Self {
        let mut child = command(&[
            ("CALLBOARD_LISTEN", "127.0.0.1:0"),
            ("CALLBOARD_CASTLIST_URL", NOWHERE),
            ("CALLBOARD_PAGETRAY_URL", NOWHERE),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("callboard-server を起こす");
        let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
        let line = lines
            .next()
            .expect("起きた時の行が出る")
            .expect("stdout を読む");
        let port = line
            .split_whitespace()
            .find_map(|word| word.strip_prefix("listen=127.0.0.1:"))
            .and_then(|port| port.parse().ok())
            .unwrap_or_else(|| panic!("待ち受けの行に port が無い: {line}"));
        // 残りの行は読み続けて残す（pipe を詰まらせない）。
        let rest: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = rest.clone();
        std::thread::spawn(move || {
            for line in lines.map_while(Result::ok) {
                sink.lock().unwrap().push(line);
            }
        });
        Self {
            child,
            port,
            line,
            lines: rest,
        }
    }

    /// 生の要求を送り、状態と、答えの全部（頭と本文）を読む。
    fn send(&self, request: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("つなぐ");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).expect("答えを読む");
        let status = answer
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| panic!("状態が読めない: {answer}"));
        (status, answer)
    }

    fn get(&self, path: &str, host: &str) -> (u16, String) {
        self.send(&format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        ))
    }

    /// stdout に、`words` をみな含む行が出るまで待つ（2 秒まで）。出なければ、出た行を付けて落とす。
    fn logged(&self, words: &[&str]) -> String {
        let started = Instant::now();
        loop {
            let lines = self.lines.lock().unwrap().clone();
            if let Some(line) = lines.iter().find(|l| words.iter().all(|w| l.contains(w))) {
                return line.clone();
            }
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "{words:?} の行が記録に無い: {lines:#?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// 止める合図（SIGTERM）を送り、終わるのを待つ。
    fn terminate(mut self) -> ExitStatus {
        let status = Command::new("kill")
            .arg(self.child.id().to_string())
            .status()
            .expect("kill");
        assert!(status.success());
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                return status;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "止める合図の後、10 秒で終わらない"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn 起きて_healthz_に答え_外の_host_を断り_止める合図で自分から終わる() {
    let server = Server::start();
    // 起きた時の行に、版と、聞く先が出る。
    assert!(
        server
            .line
            .contains(&format!("callboard {} listen=", env!("CARGO_PKG_VERSION"))),
        "{}",
        server.line
    );
    assert!(
        server
            .line
            .ends_with(&format!("castlist={NOWHERE} pagetray={NOWHERE}")),
        "{}",
        server.line
    );

    let (status, answer) = server.get("/healthz", "127.0.0.1");
    assert_eq!(status, 200, "{answer}");
    assert!(answer.contains(r#""status":"ok""#), "{answer}");

    let (status, answer) = server.get("/healthz", "evil.example");
    assert_eq!(status, 403, "{answer}");
    assert!(
        answer
            .to_ascii_lowercase()
            .contains("x-amzn-errortype: forbidden"),
        "{answer}"
    );

    // 呼ぶ先が止まっていても、入口は出る。
    let (status, answer) = server.get("/", "localhost");
    assert_eq!(status, 200, "{answer}");
    assert!(answer.contains("届きません"), "{answer}");

    // 本物の接続でも、HEAD は本文なしで、GET と同じ Content-Length。
    let (status, answer) =
        server.send("HEAD / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert_eq!(status, 200, "{answer}");
    assert!(answer.ends_with("\r\n\r\n"), "{answer}");
    assert!(
        answer.to_ascii_lowercase().contains("content-length: "),
        "{answer}"
    );

    let status = server.terminate();
    assert_eq!(status.code(), Some(0), "{status:?}");
}

#[test]
fn 呼ぶ先に聞けなかった訳と_守りが断った要求を_記録に残す() {
    let server = Server::start();
    // 呼ぶ先に聞けない時は、どの呼ぶ先か・聞く先・訳を 1 行に残す（画面には聞く先を出さないので、記録から引けるようにする）。
    let (status, _) = server.get("/", "127.0.0.1");
    assert_eq!(status, 200);
    for upstream in ["castlist", "pagetray"] {
        let line = server.logged(&["WARN", &format!("upstream=\"{upstream}\"")]);
        assert!(line.contains("呼ぶ先に聞けない"), "{line}");
        assert!(line.contains("127.0.0.1:9"), "{line}");
    }
    // 守りが断った要求は、訳と Host と道を 1 行に残す。
    let (status, _) = server.get("/actors/x", "evil.example");
    assert_eq!(status, 403);
    let line = server.logged(&["refused", "evil.example"]);
    assert!(line.contains("reason=\"host\""), "{line}");
    assert!(line.contains("/actors/x"), "{line}");
    let (status, _) = server.send(
        "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://evil.example\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(status, 403);
    let line = server.logged(&["refused", "reason=\"origin\""]);
    assert!(line.contains("http://evil.example"), "{line}");
    // 断らない要求は、行を残さない。
    let (status, _) = server.get("/healthz", "127.0.0.1");
    assert_eq!(status, 200);
    let lines = server.lines.lock().unwrap().clone();
    assert!(!lines.iter().any(|l| l.contains("/healthz")), "{lines:#?}");
}

#[test]
fn 聞く先の設定が無い_形が違う時は_訳を出して起きない() {
    for (env, word) in [
        (
            vec![("CALLBOARD_PAGETRAY_URL", NOWHERE)],
            "CALLBOARD_CASTLIST_URL",
        ),
        (
            vec![("CALLBOARD_CASTLIST_URL", NOWHERE)],
            "CALLBOARD_PAGETRAY_URL",
        ),
        (
            vec![
                ("CALLBOARD_CASTLIST_URL", NOWHERE),
                ("CALLBOARD_PAGETRAY_URL", "https://pagetray:8080"),
            ],
            "https://pagetray:8080",
        ),
    ] {
        let mut env = env;
        env.push(("CALLBOARD_LISTEN", "127.0.0.1:0"));
        let output = command(&env).output().expect("callboard-server を起こす");
        assert_eq!(output.status.code(), Some(1), "{word}");
        let out = String::from_utf8_lossy(&output.stdout);
        assert!(out.contains(word), "{word}: {out}");
        assert!(!out.contains(" listen="), "{out}");
    }
}

#[test]
fn 引数を付けると_使い方を出して_2_で終わる() {
    let output = command(&[]).arg("serve").output().expect("起こす");
    assert_eq!(output.status.code(), Some(2));
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("使い方: callboard-server"), "{err}");
}
