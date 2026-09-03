//! 結果の表表示

use crate::cross::{GoldenCross, LONG_PERIOD, SHORT_PERIOD};
use crate::sanitize;
use crate::stock::Stock;

/// 1銘柄分の結果
pub struct Row {
    pub stock: Stock,
    pub cross: GoldenCross,
}

/// 見出しを表示する
pub fn print_header(within: usize, scanned: usize, hits: usize) {
    println!(
        "直近{within}営業日に{SHORT_PERIOD}日線が{LONG_PERIOD}日線を上抜けた銘柄（{scanned}銘柄中 {hits}件）"
    );
}

/// 各列の幅（半角換算）
const COLUMNS: [usize; 7] = [9, 30, 10, 14, 10, 10, 10];

/// 結果を出来高順の表として表示する
pub fn print_rows(rows: &[Row]) {
    println!();
    print_line(&[
        "コード",
        "銘柄名",
        "株価",
        "出来高",
        "クロス日",
        &format!("{SHORT_PERIOD}日線"),
        &format!("{LONG_PERIOD}日線"),
    ]);
    println!(
        "{}",
        "─".repeat(COLUMNS.iter().sum::<usize>() + COLUMNS.len() - 1)
    );

    for row in rows {
        print_line(&[
            &sanitize::escape(&row.stock.symbol),
            &truncate(&sanitize::escape(&row.stock.name), COLUMNS[1]),
            &format!("{:.1}", row.stock.price),
            &format_volume(row.stock.volume),
            &row.cross.date.format("%m/%d").to_string(),
            &format!("{:.1}", row.cross.short),
            &format!("{:.1}", row.cross.long),
        ]);
    }
}

/// 1行を列幅にそろえて表示する。
/// 先頭2列は左寄せ、数値の列は右寄せ。
fn print_line(cells: &[&str; 7]) {
    let mut line = String::new();

    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            line.push(' ');
        }
        if i < 2 {
            line.push_str(&pad_right(cell, COLUMNS[i]));
        } else {
            line.push_str(&pad_left(cell, COLUMNS[i]));
        }
    }

    println!("{}", line.trim_end());
}

/// 全角文字を2桁として数えた表示幅。
///
/// 日本語の見出しを {:<9} のような書式で並べると、Rust は文字数で
/// 数えるため桁がずれる。表示幅で数え直して自前で詰める。
fn display_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

fn char_width(c: char) -> usize {
    match c as u32 {
        0x1100..=0x115f
        | 0x2e80..=0xa4cf
        | 0xac00..=0xd7a3
        | 0xf900..=0xfaff
        | 0xfe30..=0xfe6f
        | 0xff00..=0xff60
        | 0xffe0..=0xffe6
        | 0x20000..=0x3fffd => 2,
        _ => 1,
    }
}

fn pad_right(text: &str, width: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(display_width(text)))
    )
}

fn pad_left(text: &str, width: usize) -> String {
    format!(
        "{}{text}",
        " ".repeat(width.saturating_sub(display_width(text)))
    )
}

/// 該当0件のときの案内
pub fn print_empty(within: usize) {
    println!();
    println!("直近{within}営業日にゴールデンクロスした銘柄はありませんでした。");
    println!("--within で期間を広げると見つかることがあります（例: --within 20）。");
}

/// 桁が多い出来高を3桁区切りにする
fn format_volume(volume: u64) -> String {
    let digits = volume.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);

    for (i, c) in digits.chars().enumerate() {
        // 先頭からの位置ではなく末尾からの位置で区切る
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }

    out
}

/// 表の幅に収まるよう銘柄名を切り詰める
fn truncate(name: &str, max: usize) -> String {
    if display_width(name) <= max {
        return name.to_string();
    }

    // 末尾の … の分（2桁）を空けながら詰める
    let mut head = String::new();
    let mut width = 0;

    for c in name.chars() {
        if width + char_width(c) > max - 2 {
            break;
        }
        width += char_width(c);
        head.push(c);
    }

    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::{display_width, format_volume, pad_left, pad_right, truncate};

    #[test]
    fn 出来高を3桁区切りにする() {
        assert_eq!(format_volume(0), "0");
        assert_eq!(format_volume(999), "999");
        assert_eq!(format_volume(1_000), "1,000");
        assert_eq!(format_volume(22_107_500), "22,107,500");
    }

    #[test]
    fn 長い銘柄名を切り詰める() {
        assert_eq!(truncate("短い名前", 28), "短い名前");
        assert_eq!(truncate("abcdef", 4), "ab…");
    }

    #[test]
    fn 全角文字を2桁として数える() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("銘柄名"), 6);
        assert_eq!(display_width("25日線"), 6);
    }

    #[test]
    fn 表示幅にそろえて詰める() {
        assert_eq!(pad_right("銘柄名", 8), "銘柄名  ");
        assert_eq!(pad_left("株価", 6), "  株価");
    }
}
