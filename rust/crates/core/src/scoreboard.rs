//! The high-score table.
//!
//! The original shows a **16 row** leaderboard (docs/11 section 6):
//!
//! ```text
//! 总榜
//! ┌──────────────────────────────┐
//! │  1. 挑战者              160  │
//! │  2. 国王                100  │
//! │  3. - 空的 -              0  │
//! │  ...                        │
//! └──────────────────────────────┘
//! ```
//!
//! The empty slots show the placeholder `- 空的 -` with a score of 0.
//! The model also tracks the **two players**: the CPU, `国王` (the King, the
//! red figure on the left) and the human, `挑战者` (the Challenger, the
//! magenta figure with the sword on the right).
//!
//! Everything here is pure logic: the table is formatted here and the
//! renderer only draws the strings it is given.

use crate::save::ScoreRecord;

/// The number of rows in the table.
pub const ROWS: usize = 16;
/// The CPU's default name.
pub const KING_NAME: &str = "国王";
/// The human's default name.
pub const CHALLENGER_NAME: &str = "挑战者";
/// The placeholder shown for an empty slot.
pub const EMPTY_LABEL: &str = "- 空的 -";
/// The score the table starts with.
///
/// The original's table is not empty on a fresh install: its first slot holds
/// the CPU, `国王`, with 100 points (the static analysis found the "best
/// score" global defaulting to 100, and the screenshot of the leaderboard
/// shows `国王 100` in second place).
pub const DEFAULT_KING_SCORE: u32 = 100;

/// One row of the rendered table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The rank, starting at 1.
    pub rank: usize,
    /// The name, or the empty placeholder.
    pub name: String,
    /// The score.
    pub score: u32,
    /// `true` when the slot is empty.
    pub empty: bool,
}

impl Row {
    /// The row as the original renders it: `N. name    score`.
    pub fn format(&self) -> String {
        format!("{}. {:<24}{}", self.rank, self.name, self.score)
    }
}

/// The leaderboard model.
#[derive(Clone, Debug, Default)]
pub struct Scoreboard {
    rows: Vec<Row>,
}

impl Scoreboard {
    /// Build the table from a score list, sorted best first.
    pub fn new(mut records: Vec<ScoreRecord>) -> Self {
        records.sort_by_key(|r| std::cmp::Reverse(r.score));
        records.truncate(ROWS);
        let mut rows = Vec::with_capacity(ROWS);
        for (i, r) in records.iter().enumerate() {
            rows.push(Row {
                rank: i + 1,
                name: if r.name.is_empty() {
                    EMPTY_LABEL.to_string()
                } else {
                    r.name.clone()
                },
                score: r.score,
                empty: r.name.is_empty(),
            });
        }
        while rows.len() < ROWS {
            let rank = rows.len() + 1;
            rows.push(Row {
                rank,
                name: EMPTY_LABEL.to_string(),
                score: 0,
                empty: true,
            });
        }
        Scoreboard { rows }
    }

    /// The rendered rows.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The best score.
    pub fn best(&self) -> u32 {
        self.rows.first().map(|r| r.score).unwrap_or(0)
    }

    /// `true` when the score would make the table.
    ///
    /// The original (`0x402CB1`) scans the 16 rows and inserts when the score
    /// is strictly greater than the row it stops at.  Empty rows count as
    /// zero, so any positive score qualifies while the table is not full; once
    /// it is full, the score has to beat the last row.
    pub fn qualifies(&self, score: u32) -> bool {
        score > 0 && score > self.rows.last().map(|r| r.score).unwrap_or(0)
    }
}

/// The two players of a game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Players {
    /// The CPU's name.
    pub king: String,
    /// The CPU's score.
    pub king_score: u32,
    /// The human's name.
    pub challenger: String,
    /// The human's score.
    pub challenger_score: u32,
}

impl Default for Players {
    fn default() -> Self {
        Players {
            king: KING_NAME.to_string(),
            king_score: 0,
            challenger: CHALLENGER_NAME.to_string(),
            challenger_score: 0,
        }
    }
}

impl Players {
    /// The score of the given name, if it is one of the two.
    pub fn score_of(&self, name: &str) -> Option<u32> {
        if name == self.king {
            Some(self.king_score)
        } else if name == self.challenger {
            Some(self.challenger_score)
        } else {
            None
        }
    }
}

/// The name the original pre-fills in the name entry dialog.
pub fn default_player_name() -> &'static str {
    CHALLENGER_NAME
}

/// The score table of a fresh installation.
///
/// It holds a single record: the CPU, `国王`, with the default score.  This
/// is what makes the leaderboard of a new game show `国王 100` and the HUD's
/// left score box show `100` (docs/11 section 5b).
pub fn default_table() -> Vec<crate::save::ScoreRecord> {
    vec![crate::save::ScoreRecord {
        name: KING_NAME.to_string(),
        score: DEFAULT_KING_SCORE,
    }]
}

/// The score the CPU holds in the given table.
///
/// The HUD's **left** score box shows this value (docs/11 section 5b):
/// the CPU, `国王`, on the left, the human, `挑战者`, on the right.
pub fn king_score_in(records: &[crate::save::ScoreRecord]) -> u32 {
    records
        .iter()
        .find(|r| r.name == KING_NAME)
        .map(|r| r.score)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(name: &str, score: u32) -> ScoreRecord {
        ScoreRecord {
            name: name.into(),
            score,
        }
    }

    #[test]
    fn the_table_always_has_sixteen_rows() {
        let sb = Scoreboard::new(vec![]);
        assert_eq!(sb.rows().len(), ROWS);
        for r in sb.rows() {
            assert!(r.empty);
            assert_eq!(r.score, 0);
            assert_eq!(r.name, EMPTY_LABEL);
        }
    }

    #[test]
    fn the_empty_placeholder_is_used() {
        let sb = Scoreboard::new(vec![rec("挑战者", 160), rec("国王", 100)]);
        assert_eq!(sb.rows()[0].name, "挑战者");
        assert_eq!(sb.rows()[0].score, 160);
        assert_eq!(sb.rows()[1].name, "国王");
        assert_eq!(sb.rows()[1].score, 100);
        assert_eq!(sb.rows()[2].name, EMPTY_LABEL);
        assert_eq!(sb.rows()[15].name, EMPTY_LABEL);
    }

    #[test]
    fn the_rows_are_sorted_best_first() {
        let sb = Scoreboard::new(vec![rec("C", 5), rec("A", 90), rec("B", 40)]);
        assert_eq!(sb.rows()[0].name, "A");
        assert_eq!(sb.rows()[1].name, "B");
        assert_eq!(sb.rows()[2].name, "C");
        assert_eq!(sb.best(), 90);
    }

    #[test]
    fn a_row_renders_like_the_original() {
        let sb = Scoreboard::new(vec![rec("挑战者", 160)]);
        let line = sb.rows()[0].format();
        assert!(line.starts_with("1. 挑战者"), "{line:?}");
        assert!(line.ends_with("160"), "{line:?}");
        assert!(sb.rows()[1].format().contains(EMPTY_LABEL));
    }

    #[test]
    fn the_table_is_truncated_to_sixteen() {
        let many: Vec<ScoreRecord> = (0..30).map(|i| rec(&format!("P{i}"), i)).collect();
        let sb = Scoreboard::new(many);
        assert_eq!(sb.rows().len(), ROWS);
        assert_eq!(sb.rows()[0].name, "P29");
    }

    #[test]
    fn a_fresh_table_holds_the_cpu_with_the_default_score() {
        let t = default_table();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].name, KING_NAME);
        assert_eq!(t[0].score, DEFAULT_KING_SCORE);
        assert_eq!(king_score_in(&t), DEFAULT_KING_SCORE);
    }

    #[test]
    fn the_kings_score_is_read_from_the_table() {
        let t = vec![rec("挑战者", 160), rec("国王", 100)];
        assert_eq!(king_score_in(&t), 100);
        assert_eq!(king_score_in(&[]), 0);
    }

    #[test]
    fn the_two_players_have_the_documented_names() {
        let p = Players::default();
        assert_eq!(p.king, KING_NAME);
        assert_eq!(p.challenger, CHALLENGER_NAME);
        assert_eq!(default_player_name(), CHALLENGER_NAME);
    }

    #[test]
    fn the_score_of_a_player_is_found_by_name() {
        let p = Players {
            king: "国王".into(),
            king_score: 100,
            challenger: "挑战者".into(),
            challenger_score: 160,
        };
        assert_eq!(p.score_of("国王"), Some(100));
        assert_eq!(p.score_of("挑战者"), Some(160));
        assert_eq!(p.score_of("nobody"), None);
    }
}
