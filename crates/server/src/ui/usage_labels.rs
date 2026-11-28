//! 使用量の画面（`GET /usage/`）の絞り込みの、ラベルの候補（t-nd3w）。外に触らない純な計算。
//!
//! - 候補は、表示と同じ期間に稼働した席（castlist の `ListActors`）のラベルから作る。ラベルで絞っても候補は減らない。
//! - `tpm.role`（役割）と `tpm.subject`（課題）は選ぶ一覧（`<select>`）、`tpm.launch`（起こした回の id）は候補にせず、
//!   ほかの鍵は「ほかのラベル」の欄の候補（`<datalist>`）にする。並びは席の多い順、同じなら値の字の順。
//! - URL の `label` は、初めの `tpm.role:…` を役割の一覧、初めの `tpm.subject:…` を課題の一覧に入れ、残りを「ほかのラベル」の欄に出す。
//!   一覧に入れた値が候補に無ければ（期間の外の席など）、席の数を付けずに候補に足す（送り直しても条件が消えないように）。
use std::collections::HashMap;

/// 役割の鍵。
pub const ROLE: &str = "tpm.role";
/// 課題の鍵。
pub const SUBJECT: &str = "tpm.subject";
/// 候補にしない鍵（起こした回の id。ほぼ 1 席に 1 つで、覚える物でも選ぶ物でもない）。
const SKIPPED: &[&str] = &["tpm.launch"];

/// 候補 1 つ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// 送る値（`鍵:値`）。
    pub value: String,
    /// 値の部分（一覧に出す名）。
    pub name: String,
    /// そのラベルを持つ席の数。候補に無かった今の値は無い。
    pub seats: Option<usize>,
}

/// 鍵ごとに分けた候補。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choices {
    pub roles: Vec<Choice>,
    pub subjects: Vec<Choice>,
    pub others: Vec<Choice>,
}

/// URL の `label` を、一覧と欄に振り分けた物（どれも `鍵:値`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picked {
    pub role: Option<String>,
    pub subject: Option<String>,
    /// 「ほかのラベル」の欄に出す値（URL の順）。
    pub others: Vec<String>,
}

/// 席ごとのラベル（鍵 → 値。席ごとに 1 つずつ）から、候補を作る。
pub fn choices<'a>(actors: impl IntoIterator<Item = &'a HashMap<String, String>>) -> Choices {
    let mut seats: HashMap<(&str, &str), usize> = HashMap::new();
    for labels in actors {
        for (key, value) in labels {
            if !SKIPPED.contains(&key.as_str()) {
                *seats.entry((key.as_str(), value.as_str())).or_default() += 1;
            }
        }
    }
    let mut out = Choices::default();
    for ((key, value), n) in seats {
        let choice = Choice {
            value: format!("{key}:{value}"),
            name: value.to_owned(),
            seats: Some(n),
        };
        match key {
            ROLE => out.roles.push(choice),
            SUBJECT => out.subjects.push(choice),
            _ => out.others.push(choice),
        }
    }
    for list in [&mut out.roles, &mut out.subjects, &mut out.others] {
        list.sort_by(|a, b| b.seats.cmp(&a.seats).then_with(|| a.value.cmp(&b.value)));
    }
    out
}

/// URL の `label`（確かめた `鍵:値`）を、一覧と欄に振り分ける。
pub fn pick(labels: &[String]) -> Picked {
    let mut picked = Picked::default();
    for label in labels {
        let slot = match label.split_once(':') {
            Some((ROLE, _)) => &mut picked.role,
            Some((SUBJECT, _)) => &mut picked.subject,
            _ => {
                picked.others.push(label.clone());
                continue;
            }
        };
        if slot.is_none() {
            *slot = Some(label.clone());
        } else {
            picked.others.push(label.clone());
        }
    }
    picked
}

impl Choices {
    /// 一覧に入れた今の値が候補に無ければ、席の数を付けずに足す。
    pub fn keep(&mut self, picked: &Picked) {
        for (list, value) in [
            (&mut self.roles, &picked.role),
            (&mut self.subjects, &picked.subject),
        ] {
            let Some(value) = value else { continue };
            if list.iter().any(|c| &c.value == value) {
                continue;
            }
            let name = value.split_once(':').map_or(value.as_str(), |(_, v)| v);
            list.push(Choice {
                value: value.clone(),
                name: name.to_owned(),
                seats: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn choice(value: &str, seats: Option<usize>) -> Choice {
        Choice {
            value: value.to_owned(),
            name: value.split_once(':').unwrap().1.to_owned(),
            seats,
        }
    }

    fn values(choices: &[Choice]) -> Vec<(&str, Option<usize>)> {
        choices
            .iter()
            .map(|c| (c.value.as_str(), c.seats))
            .collect()
    }

    #[test]
    fn 役割と課題は一覧に_起こした回の_id_は除き_ほかは欄の候補() {
        let actors = [
            labels(&[
                ("tpm.role", "software-engineer"),
                ("tpm.subject", "t-nd3w"),
                ("tpm.key", "t-nd3w"),
                ("tpm.launch", "0f8f5700-639d-411f-b170-b4346dfcebff"),
            ]),
            labels(&[
                ("tpm.role", "staff-engineer"),
                ("tpm.subject", "t-nd3w"),
                ("tpm.key", "t-nd3w.staff"),
                ("tpm.launch", "1000ce54-d733-4d44-aa29-cc1637b8d9c7"),
            ]),
            labels(&[("tpm.role", "software-engineer"), ("project", "castlist")]),
            labels(&[]),
        ];
        let got = choices(&actors);
        assert_eq!(
            values(&got.roles),
            [
                ("tpm.role:software-engineer", Some(2)),
                ("tpm.role:staff-engineer", Some(1)),
            ]
        );
        assert_eq!(got.roles[0].name, "software-engineer");
        assert_eq!(values(&got.subjects), [("tpm.subject:t-nd3w", Some(2))]);
        // ほかの鍵は、席の多い順、同じなら値の字の順（鍵も含めた字の順）。tpm.launch は無い。
        assert_eq!(
            values(&got.others),
            [
                ("project:castlist", Some(1)),
                ("tpm.key:t-nd3w", Some(1)),
                ("tpm.key:t-nd3w.staff", Some(1)),
            ]
        );
    }

    #[test]
    fn 席の多い順_同じ数なら字の順() {
        let a = labels(&[("tpm.subject", "t-b")]);
        let b = labels(&[("tpm.subject", "t-a")]);
        let c = labels(&[("tpm.subject", "t-c")]);
        let d = labels(&[("tpm.subject", "t-c")]);
        let got = choices([&a, &b, &c, &d]);
        assert_eq!(
            values(&got.subjects),
            [
                ("tpm.subject:t-c", Some(2)),
                ("tpm.subject:t-a", Some(1)),
                ("tpm.subject:t-b", Some(1)),
            ]
        );
        assert!(got.roles.is_empty());
        assert!(got.others.is_empty());
    }

    #[test]
    fn 初めの役割と課題を一覧に_残りは欄に_url_の順で() {
        let got = pick(&[
            "tpm.key:t-1".to_owned(),
            "tpm.subject:t-a".to_owned(),
            "tpm.role:tpm".to_owned(),
            "tpm.role:auditor".to_owned(),
            "tpm.subject:t-b".to_owned(),
        ]);
        assert_eq!(
            got,
            Picked {
                role: Some("tpm.role:tpm".to_owned()),
                subject: Some("tpm.subject:t-a".to_owned()),
                others: vec![
                    "tpm.key:t-1".to_owned(),
                    "tpm.role:auditor".to_owned(),
                    "tpm.subject:t-b".to_owned(),
                ],
            }
        );
        // 鍵が同じ字で始まるだけの物（tpm.roles）は、役割ではない。
        assert_eq!(
            pick(&["tpm.roles:x".to_owned()]),
            Picked {
                others: vec!["tpm.roles:x".to_owned()],
                ..Picked::default()
            }
        );
        assert_eq!(pick(&[]), Picked::default());
    }

    #[test]
    fn 一覧に入れた値が候補に無ければ_数を付けずに足す() {
        let actors = [labels(&[("tpm.role", "tpm")])];
        let mut got = choices(&actors);
        got.keep(&Picked {
            role: Some("tpm.role:auditor".to_owned()),
            subject: Some("tpm.subject:t-old".to_owned()),
            others: vec!["tpm.key:t-old".to_owned()],
        });
        assert_eq!(
            values(&got.roles),
            [("tpm.role:tpm", Some(1)), ("tpm.role:auditor", None)]
        );
        assert_eq!(values(&got.subjects), [("tpm.subject:t-old", None)]);
        // 欄の値は候補に足さない（欄に字が入っているので、消えない）。
        assert!(got.others.is_empty());
        // 候補に在る値は足さない。
        got.keep(&Picked {
            role: Some("tpm.role:tpm".to_owned()),
            ..Picked::default()
        });
        assert_eq!(got.roles.len(), 2);
        assert_eq!(choice("tpm.role:tpm", Some(1)), got.roles[0]);
    }
}
