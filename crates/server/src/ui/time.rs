//! 画面に出す時刻。castlist が受けた時刻（UTC）を日本の時刻で出し、ISO の字を `<time datetime>` と `title` に置く。
use aws_smithy_types::DateTime;
use time::{OffsetDateTime, UtcOffset};

/// 画面に出す時刻のずれ（日本の時刻）。
pub const OFFSET: UtcOffset = match UtcOffset::from_hms(9, 0, 0) {
    Ok(offset) => offset,
    Err(_) => UtcOffset::UTC,
};

/// 時刻: 画面の字（日本の時刻）と、元の ISO の字。
pub struct Time {
    pub iso: String,
    pub local: String,
}

/// 時刻を、castlist の API が出す字と同じ形（UTC・ミリ秒まで・24 字）にする。
pub fn stamp(t: OffsetDateTime) -> String {
    let t = t.to_offset(UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year().clamp(0, 9999),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    )
}

/// 日本の時刻の字（`2026-11-23 09:00:00`）。
fn local(t: OffsetDateTime) -> String {
    let t = t.to_offset(OFFSET);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// castlist の答えの時刻を、画面に出す形にする。扱える時刻の外なら、秒の数をそのまま出す。
pub fn time(t: &DateTime) -> Time {
    match OffsetDateTime::from_unix_timestamp_nanos(t.as_nanos()) {
        Ok(t) => Time {
            iso: stamp(t),
            local: local(t),
        },
        Err(_) => Time {
            iso: t.secs().to_string(),
            local: t.secs().to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 時刻は_utc_のミリ秒までの字と_日本の時刻の字で出す() {
        // 2026-11-23T00:00:00.123456Z
        let t = time(&DateTime::from_secs_and_nanos(1_795_392_000, 123_456_000));
        assert_eq!(t.iso, "2026-11-23T00:00:00.123Z");
        assert_eq!(t.local, "2026-11-23 09:00:00");
        // 日付をまたぐ。
        let t = time(&DateTime::from_secs(1_795_392_000 + 15 * 3600 + 59));
        assert_eq!(t.iso, "2026-11-23T15:00:59.000Z");
        assert_eq!(t.local, "2026-11-24 00:00:59");
    }
}
