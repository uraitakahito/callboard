//! 費えの図（棒と、段々の線）の置き場の計算。外に触らない純な計算（タイムラインの並べ方 [`super::timeline_layout`] と同じく、
//! 単体の試験を持つ）。座標は SVG の px（整数）、時刻は UNIX の時からの秒。
//!
//! - 図の大きさは決まった物（[`WIDTH`]×[`HEIGHT`]）。左に縦の目盛りの字、下に横の目盛りの字の余白を取る。
//! - 縦の目盛りは 0 から、1・2・5 × 10 の冪の刻みで 4 つほど（[`axis`]）。いちばん上は、いちばん大きい値を刻みに切り上げた物。
//! - 棒は、横を等しく分けた枠（[`slots`]）の中に置く。0 の値の棒は描かない。0 でない値は、低くても 1 px の高さにする。
//! - 段々の線（[`steps`]）は、時ごとの値を、その時の初めから終わりまでの横の線にし、続く時は縦の線でつなぐ。

/// 図の幅と高さ。
pub const WIDTH: u32 = 720;
pub const HEIGHT: u32 = 220;
/// 描く所の左（縦の目盛りの字の分）・右・上・下（横の目盛りの字の分）の余白。
pub const LEFT: u32 = 64;
pub const RIGHT: u32 = 16;
pub const TOP: u32 = 16;
pub const BOTTOM: u32 = 28;
/// 描く所の幅と高さと、0 の線の y。
pub const PLOT_W: u32 = WIDTH - LEFT - RIGHT;
pub const PLOT_H: u32 = HEIGHT - TOP - BOTTOM;
pub const BASE: u32 = TOP + PLOT_H;

/// 1 時間と 1 日（秒）。
pub const HOUR: i64 = 3600;
pub const DAY: i64 = 24 * HOUR;

/// 縦の目盛り: 刻みと、いちばん上の値（刻みの倍数）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Axis {
    pub step: i64,
    pub top: i64,
}

impl Axis {
    /// 目盛りの値（0 から、いちばん上まで）と、その y。
    pub fn ticks(&self) -> Vec<(i64, u32)> {
        Vec::new()
    }

    /// 値の y（0 は [`BASE`]、いちばん上は [`TOP`]。外の値は端に寄せる。切り捨て）。
    pub fn y(&self, _value: i64) -> u32 {
        0
    }
}

/// いちばん大きい値 `max` から、縦の目盛りを決める。刻みは 1・2・5 × 10 の冪で、`max` の 4 分の 1 以上の
/// いちばん小さい物（`min_step` より細かくしない。`min_step` は 10 の冪）。値が 0 なら、0 から `min_step` まで。
pub fn axis(_max: i64, _min_step: i64) -> Axis {
    Axis { step: 1, top: 1 }
}

/// 横を等しく分けた枠 1 つ（左の端と幅）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub left: u32,
    pub width: u32,
}

impl Slot {
    /// 枠の真ん中。
    pub fn mid(&self) -> u32 {
        self.left + self.width / 2
    }
}

/// 描く所の幅を `n` の枠に分ける（左から。端は切り捨てで揃え、枠の間に隙間を作らない）。
pub fn slots(_n: usize) -> Vec<Slot> {
    Vec::new()
}

/// 棒 1 本の置き場。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bar {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 枠の中の、`base` から `base + value` までの棒（積む棒は、下の棒の値を `base` に渡す）。左右に枠の幅の 6 分の 1 ずつ空ける。
/// `value` が 0 以下なら無い。0 でなければ、低くても 1 px。
pub fn bar(_axis: Axis, _slot: &Slot, _base: i64, _value: i64) -> Option<Bar> {
    None
}

/// `n` の枠に横の目盛りの字を置く時の間引き: 字の間を `min_px` より狭くしない、いちばん小さい間（枠の数）。
pub fn label_every(_n: usize, _min_px: u32) -> usize {
    0
}

/// 時刻の窓 [`from`, `to`) を、日（`offset_s` だけずらした時刻の 0 時の境。日本時間なら 9 時間）で区切った、区切りの初め。
/// 初めは `from`（日の途中でも）。
pub fn day_starts(_from: i64, _to: i64, _offset_s: i64) -> Vec<i64> {
    Vec::new()
}

/// 時刻の窓 [`from`, `to`) を、1 時間ごとに区切った、区切りの初め（`from` は時の境に揃えた物）。
pub fn hour_starts(_from: i64, _to: i64) -> Vec<i64> {
    Vec::new()
}

/// 時刻の窓 [`start`, `end`] の中の時刻の x（外は端に寄せる。切り捨て）。
pub fn x_at(_start: i64, _end: i64, _t: i64) -> u32 {
    0
}

/// 時ごとの値（時の初めの時刻と値。時の順）の、段々の線の SVG の path（`d`）。時の初めから 1 時間の横の線にし、
/// 前の時に続く時は縦の線でつなぐ。記録の無い時は、線を切る。
pub fn steps(_points: &[(i64, i64)], _start: i64, _end: i64, _axis: Axis) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 縦の目盛りは_1_2_5_の刻みで_いちばん大きい値を刻みに切り上げる() {
        // $7.26 → 刻み $2・上は $8。
        let a = axis(7_260_000, 10_000);
        assert_eq!(
            a,
            Axis {
                step: 2_000_000,
                top: 8_000_000
            }
        );
        let values: Vec<i64> = a.ticks().iter().map(|t| t.0).collect();
        assert_eq!(values, [0, 2_000_000, 4_000_000, 6_000_000, 8_000_000]);
        // ちょうど刻みの倍数なら、そのまま。
        assert_eq!(
            axis(3_000_000, 10_000),
            Axis {
                step: 1_000_000,
                top: 3_000_000
            }
        );
        assert_eq!(
            axis(8_000_000, 10_000),
            Axis {
                step: 2_000_000,
                top: 8_000_000
            }
        );
        // 5 の刻み。
        assert_eq!(axis(45, 1), Axis { step: 20, top: 60 });
        assert_eq!(axis(18, 1), Axis { step: 5, top: 20 });
        // 文脈の長さ（23 万）: 刻み 10 万・上は 30 万。
        assert_eq!(
            axis(230_000, 10_000),
            Axis {
                step: 100_000,
                top: 300_000
            }
        );
    }

    #[test]
    fn 値が_0_か_とても小さい時は_いちばん細かい刻みの_1_つ分() {
        let a = axis(0, 10_000);
        assert_eq!(
            a,
            Axis {
                step: 10_000,
                top: 10_000
            }
        );
        assert_eq!(a.ticks(), [(0, BASE), (10_000, TOP)]);
        assert_eq!(
            axis(1, 10_000),
            Axis {
                step: 10_000,
                top: 10_000
            }
        );
        assert_eq!(
            axis(-5, 10_000),
            Axis {
                step: 10_000,
                top: 10_000
            }
        );
    }

    #[test]
    fn 値の_y_は_0_が下の線_いちばん上が上の端で_外は端に寄せる() {
        let a = Axis {
            step: 1_000_000,
            top: 3_000_000,
        };
        assert_eq!((TOP, PLOT_H, BASE), (16, 176, 192));
        assert_eq!(a.y(0), BASE);
        assert_eq!(a.y(3_000_000), TOP);
        assert_eq!(a.y(1_500_000), 104);
        assert_eq!(a.y(-5), BASE);
        assert_eq!(a.y(4_000_000), TOP);
        assert_eq!(
            a.ticks(),
            [(0, 192), (1_000_000, 134), (2_000_000, 75), (3_000_000, 16)]
        );
    }

    #[test]
    fn 枠は描く所の幅を隙間なく等しく分ける() {
        assert!(slots(0).is_empty());
        let one = slots(1);
        assert_eq!(
            one,
            [Slot {
                left: LEFT,
                width: PLOT_W
            }]
        );
        assert_eq!(one[0].mid(), LEFT + PLOT_W / 2);
        // 段の図の 41 の枠。
        let s = slots(41);
        assert_eq!(s.len(), 41);
        assert_eq!(s[0].left, LEFT);
        assert_eq!(s[40].left + s[40].width, LEFT + PLOT_W);
        for pair in s.windows(2) {
            assert_eq!(pair[0].left + pair[0].width, pair[1].left);
        }
        assert!(
            s.iter().all(|slot| (15..=16).contains(&slot.width)),
            "{s:?}"
        );
    }

    #[test]
    fn 棒の高さは値に比べ_0_の棒は無く_小さい値も_1_px_の高さ() {
        let a = Axis {
            step: 1_000_000,
            top: 3_000_000,
        };
        let slot = Slot {
            left: 64,
            width: 60,
        };
        assert_eq!(
            bar(a, &slot, 0, 1_500_000),
            Some(Bar {
                x: 74,
                y: 104,
                w: 40,
                h: 88
            })
        );
        // いちばん上まで。
        assert_eq!(
            bar(a, &slot, 0, 3_000_000),
            Some(Bar {
                x: 74,
                y: TOP,
                w: 40,
                h: PLOT_H
            })
        );
        assert_eq!(bar(a, &slot, 0, 0), None);
        assert_eq!(bar(a, &slot, 0, -1), None);
        // とても小さい値も、見えるように 1 px。
        assert_eq!(
            bar(a, &slot, 0, 1),
            Some(Bar {
                x: 74,
                y: BASE - 1,
                w: 40,
                h: 1
            })
        );
        // 積む棒: 下の棒の上から。
        assert_eq!(
            bar(a, &slot, 1_500_000, 1_500_000),
            Some(Bar {
                x: 74,
                y: TOP,
                w: 40,
                h: 88
            })
        );
        // 狭い枠でも、幅は 1 px より細くしない。
        let narrow = Slot { left: 64, width: 1 };
        assert_eq!(
            bar(a, &narrow, 0, 3_000_000).map(|b| (b.x, b.w)),
            Some((64, 1))
        );
    }

    #[test]
    fn 横の目盛りの字は_狭い枠なら間引く() {
        // 段の図（41 の枠・枠の幅 15 px）: 4 つごと（10 万ごと）。
        assert_eq!(label_every(41, 48), 4);
        assert_eq!(label_every(8, 48), 1);
        assert_eq!(label_every(24, 48), 2);
        assert_eq!(label_every(32, 48), 3);
        assert_eq!(label_every(0, 48), 1);
        assert_eq!(label_every(1, 48), 1);
    }

    /// 2026-11-22T01:00:00Z（日本時間の 10:00）。
    const T: i64 = 1_795_309_200;
    const JST: i64 = 9 * HOUR;

    #[test]
    fn 日の区切りは_日本時間の_0_時で_初めは窓の初め() {
        // 日本時間の 10:00 から、3 日後の 10:00 まで。区切りは、初めと、日本時間の 0 時（UTC の 15:00）。
        assert_eq!(
            day_starts(T, T + 3 * DAY, JST),
            [
                T,
                T + 14 * HOUR,
                T + 14 * HOUR + DAY,
                T + 14 * HOUR + 2 * DAY
            ]
        );
        // 日本時間の 0 時ちょうどからなら、余分な区切りを作らない。
        let midnight = T - 10 * HOUR;
        assert_eq!(
            day_starts(midnight, midnight + 2 * DAY, JST),
            [midnight, midnight + DAY]
        );
        // 1 日より短い窓。
        assert_eq!(day_starts(T, T + 3 * HOUR, JST), [T]);
        assert!(day_starts(T, T, JST).is_empty());
    }

    #[test]
    fn 時の区切りは_1_時間ごと() {
        assert_eq!(hour_starts(T, T + 3 * HOUR), [T, T + HOUR, T + 2 * HOUR]);
        assert!(hour_starts(T, T).is_empty());
    }

    #[test]
    fn 段々の線は_続く時を縦の線でつなぎ_記録の無い時は切る() {
        let a = Axis {
            step: 100_000,
            top: 300_000,
        };
        assert_eq!(x_at(0, 4 * HOUR, 0), LEFT);
        assert_eq!(x_at(0, 4 * HOUR, 2 * HOUR), LEFT + PLOT_W / 2);
        assert_eq!(x_at(0, 4 * HOUR, 4 * HOUR), LEFT + PLOT_W);
        // 外は端に寄せる。
        assert_eq!(x_at(0, 4 * HOUR, -HOUR), LEFT);
        assert_eq!(x_at(0, 4 * HOUR, 9 * HOUR), LEFT + PLOT_W);
        // 0 時と 1 時は続き、2 時は記録が無く、3 時から。y は 10 万が 134・20 万が 75・5 万が 163。
        let points = [(0, 100_000), (HOUR, 200_000), (3 * HOUR, 50_000)];
        assert_eq!(
            steps(&points, 0, 4 * HOUR, a),
            "M64,134H224V75H384M544,163H704"
        );
        // 1 つだけ。
        assert_eq!(steps(&[(HOUR, 300_000)], 0, 4 * HOUR, a), "M224,16H384");
        assert_eq!(steps(&[], 0, 4 * HOUR, a), "");
    }
}
