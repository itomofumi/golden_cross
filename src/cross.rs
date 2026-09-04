//! ゴールデンクロスの判定。
//!
//! ゴールデンクロス = 短期移動平均線が長期移動平均線を下から上へ抜けること。
//! 日本株では 25日線 と 75日線 の組み合わせが一般的。

use chrono::{DateTime, FixedOffset};

use crate::stock::Stock;

/// 短期移動平均の期間（日）
pub const SHORT_PERIOD: usize = 25;

/// 長期移動平均の期間（日）
pub const LONG_PERIOD: usize = 75;

/// 取得期間の候補と、実際に返ってくる営業日数（7203.T で実測）。
/// 短い順に並べる。
const RANGES: [(&str, usize); 4] = [("6mo", 126), ("1y", 244), ("2y", 488), ("5y", 1223)];

/// 営業日数の見積もりに持たせる余裕。
/// 銘柄によって休場や上場時期が異なり、実際の件数は上下するため。
const MARGIN: usize = 5;

/// 判定に必要な件数を満たす、最も短い取得期間を選ぶ。
///
/// 必要なのは「長期線が引ける件数 + 遡る営業日数」。
/// 固定値にすると、既定では余分に取り、長い --within では静かに検出漏れする。
pub fn required_range(within: usize) -> &'static str {
    let needed = LONG_PERIOD + within + MARGIN;

    for (range, days) in RANGES {
        if days >= needed {
            return range;
        }
    }

    // どれでも足りない場合は最長を使う（それ以上は取得できない）
    RANGES[RANGES.len() - 1].0
}

/// 検出したゴールデンクロス
pub struct GoldenCross {
    /// クロスが起きた日
    pub date: DateTime<FixedOffset>,
    /// クロス日から数えて何営業日前か（0 = 最新日）
    pub days_ago: usize,
    /// クロス時点の短期線の値
    pub short: f64,
    /// クロス時点の長期線の値
    pub long: f64,
}

/// 直近のゴールデンクロスを探す。
///
/// 前日に「短期 <= 長期」だったものが当日「短期 > 長期」になった日をクロスとみなす。
/// 複数あれば最も新しいものを返す。
pub fn find_latest(stock: &Stock) -> Option<GoldenCross> {
    let short = stock.moving_average(SHORT_PERIOD);
    let long = stock.moving_average(LONG_PERIOD);
    let last_index = stock.history.len().checked_sub(1)?;

    // 新しい日から遡り、最初に見つかったクロスを返す
    for i in (1..stock.history.len()).rev() {
        // 両方の線がそろっていない日は判定できない
        let (Some(short_prev), Some(long_prev)) = (short[i - 1], long[i - 1]) else {
            continue;
        };
        let (Some(short_now), Some(long_now)) = (short[i], long[i]) else {
            continue;
        };

        if short_prev <= long_prev && short_now > long_now {
            return Some(GoldenCross {
                date: stock.history[i].date,
                days_ago: last_index - i,
                short: short_now,
                long: long_now,
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{LONG_PERIOD, SHORT_PERIOD, find_latest};
    use crate::stock::tests::stock_with_closes;

    /// 長期線が下がり続けたあとに短期線が追い抜く形の終値を作る。
    ///
    /// 前半は下降、後半は上昇。十分な長さがあれば途中でクロスが発生する。
    fn v_shaped_closes() -> Vec<f64> {
        let mut closes: Vec<f64> = (0..LONG_PERIOD * 2).map(|i| 1000.0 - i as f64).collect();
        closes.extend((0..LONG_PERIOD * 2).map(|i| 1000.0 - (LONG_PERIOD * 2) as f64 + i as f64));
        closes
    }

    #[test]
    fn 下降から上昇へ転じるとクロスを検出する() {
        let stock = stock_with_closes(&v_shaped_closes());

        let cross = find_latest(&stock).expect("クロスが検出されなかった");
        assert!(
            cross.short > cross.long,
            "クロス時点では短期線が上にあるはず: {} <= {}",
            cross.short,
            cross.long
        );
    }

    #[test]
    fn 上昇し続ける場合はクロスしない() {
        // 最初から短期線が上にあり、下から抜ける瞬間が存在しない
        let closes: Vec<f64> = (0..LONG_PERIOD * 2).map(|i| 1000.0 + i as f64).collect();
        let stock = stock_with_closes(&closes);

        assert!(find_latest(&stock).is_none());
    }

    #[test]
    fn データが足りなければ判定しない() {
        let closes: Vec<f64> = (0..SHORT_PERIOD).map(|i| 1000.0 + i as f64).collect();
        let stock = stock_with_closes(&closes);

        assert!(find_latest(&stock).is_none());
    }

    #[test]
    fn 既定の遡り日数では最も短い期間を選ぶ() {
        use super::required_range;

        assert_eq!(required_range(5), "6mo");
    }

    #[test]
    fn 遡る日数に応じて期間を広げる() {
        use super::required_range;

        // 126営業日で足りる境界（75 + within + 5 <= 126）
        assert_eq!(required_range(46), "6mo");
        assert_eq!(required_range(47), "1y");
        assert_eq!(required_range(164), "1y");
        assert_eq!(required_range(165), "2y");
        assert_eq!(required_range(408), "2y");
        assert_eq!(required_range(409), "5y");
    }

    #[test]
    fn 最長を超える指定でも最長を返す() {
        use super::required_range;

        assert_eq!(required_range(100_000), "5y");
    }

    #[test]
    fn 履歴が空でも落ちない() {
        let stock = stock_with_closes(&[]);

        assert!(find_latest(&stock).is_none());
    }
}
