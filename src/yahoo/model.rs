//! Yahoo Finance のレスポンスを受け取るための構造体。
//!
//! レスポンスは
//!   { "chart": { "result": [ { "meta": {...}, "timestamp": [...], "indicators": {...} } ] } }
//! という入れ子構造。必要な階層だけ定義すれば、他のフィールドは serde が無視する。

use serde::Deserialize;

#[derive(Deserialize)]
pub struct ChartResponse {
    pub chart: Chart,
}

#[derive(Deserialize)]
pub struct Chart {
    pub result: Vec<ChartResult>,
}

#[derive(Deserialize)]
pub struct ChartResult {
    pub meta: Meta,
    /// 各営業日の時刻（UNIX秒）。indicators の配列と同じ順番で並んでいる
    pub timestamp: Vec<i64>,
    pub indicators: Indicators,
}

#[derive(Deserialize)]
pub struct Indicators {
    pub quote: Vec<Quote>,
}

#[derive(Deserialize)]
pub struct Quote {
    /// 終値。データ欠損の日は null になるので Option で受ける
    pub close: Vec<Option<f64>>,
}

/// JSON 側は longName のようなキャメルケース、Rust 側はスネークケースが慣習。
/// rename_all を付けると serde が自動で対応付けてくれる。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub symbol: String,
    pub long_name: Option<String>,
    pub short_name: Option<String>,
    pub regular_market_price: f64,
    pub regular_market_volume: u64,
}
