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
    fn 履歴が空でも落ちない() {
        let stock = stock_with_closes(&[]);

        assert!(find_latest(&stock).is_none());
    }
}
