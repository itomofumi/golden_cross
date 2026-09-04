//! Yahoo Finance からの株価取得。
//!
//! 外に公開するのは fetch() だけで、HTTP のやり取りと JSON の形は
//! このモジュールの中に閉じ込めている。

mod model;

use chrono::{DateTime, FixedOffset, TimeZone};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use std::error::Error;
use std::fmt;

use crate::cache::Cache;
use crate::sanitize;
use crate::stock::{DailyClose, Stock};
use model::ChartResponse;

const ENDPOINT: &str = "https://query1.finance.yahoo.com/v8/finance/chart";

/// 銘柄コードの長さの上限。
const MAX_SYMBOL_LEN: usize = 20;

/// 取得の失敗理由。
///
/// レート制限だけは呼び出し側の判断（以降の取得を打ち切る）に関わるため、
/// 他の失敗と区別できる形にしている。
/// スレッド間で受け渡すため Send である必要がある。
#[derive(Debug)]
pub enum FetchError {
    /// レート制限（HTTP 429）。Retry-After があればその秒数
    RateLimited { retry_after: Option<u64> },
    /// それ以外の失敗
    Other(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::RateLimited {
                retry_after: Some(seconds),
            } => {
                write!(f, "レート制限を受けました（{seconds}秒後に再試行可能）")
            }
            FetchError::RateLimited { retry_after: None } => {
                write!(f, "レート制限を受けました")
            }
            FetchError::Other(message) => write!(f, "{message}"),
        }
    }
}

impl From<Box<dyn Error>> for FetchError {
    fn from(error: Box<dyn Error>) -> Self {
        FetchError::Other(error.to_string())
    }
}

impl From<reqwest::Error> for FetchError {
    fn from(error: reqwest::Error) -> Self {
        FetchError::Other(error.to_string())
    }
}

/// 指定した銘柄・期間の株価を取得する。
///
/// 同じ日にすでに取得していればキャッシュを使い、通信しない。
pub fn fetch(
    client: &Client,
    symbol: &str,
    range: &str,
    cache: &Cache,
) -> Result<Stock, FetchError> {
    validate_symbol(symbol)?;

    // 壊れたキャッシュは読み飛ばして取得し直す
    if let Some(cached) = cache.read(symbol, range)
        && let Ok(stock) = parse(&cached, symbol)
    {
        return Ok(stock);
    }

    let body = request(client, symbol, range)?;
    let stock = parse(&body, symbol)?;

    cache.write(symbol, range, &body);

    Ok(stock)
}

/// API を呼び、レスポンス本文を文字列で返す
fn request(client: &Client, symbol: &str, range: &str) -> Result<String, FetchError> {
    // クエリは query() に組み立てさせる。文字列連結だと、値に含まれる
    // & や = がそのまま構造として解釈されてしまうため。
    let response = client
        .get(format!("{ENDPOINT}/{symbol}"))
        .query(&[("range", range), ("interval", "1d")])
        .send()?;

    // 存在しない銘柄コードには Yahoo が 404 を返す。
    // error_for_status() に任せると HTTP の生エラーがそのまま出てしまうため、
    // その手前でステータスを見て分かりやすい案内に差し替える。
    if response.status() == StatusCode::NOT_FOUND {
        return Err(FetchError::Other(format!(
            "銘柄コード {} が見つかりませんでした",
            sanitize::for_display(symbol)
        )));
    }

    // レート制限。呼び出し側が以降の取得を打ち切れるよう、専用の値で返す
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = parse_retry_after(
            response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok()),
        );
        return Err(FetchError::RateLimited { retry_after });
    }

    Ok(response.error_for_status()?.text()?)
}

/// Retry-After ヘッダの秒数を読む。
///
/// 日時形式で返ってくることもあるが、その場合は解釈せず None にする
/// （待ち時間を案内できないだけで、中断する動きは変わらないため）。
fn parse_retry_after(value: Option<&str>) -> Option<u64> {
    value?.trim().parse().ok()
}

/// レスポンス本文を Stock に変換する
fn parse(text: &str, symbol: &str) -> Result<Stock, FetchError> {
    let body: ChartResponse =
        serde_json::from_str(text).map_err(|e| FetchError::Other(e.to_string()))?;

    let result = body.chart.result.into_iter().next().ok_or_else(|| {
        FetchError::Other(format!(
            "銘柄 {} のデータが取得できませんでした",
            sanitize::for_display(symbol)
        ))
    })?;

    let meta = result.meta;
    let quote = result
        .indicators
        .quote
        .into_iter()
        .next()
        .ok_or_else(|| FetchError::Other("価格データが取得できませんでした".to_string()))?;

    // 日付と終値をペアにする。終値が null の日（休場など）は除外する
    let mut history = Vec::with_capacity(result.timestamp.len());
    for (unixtime, close) in result.timestamp.iter().zip(quote.close.iter()) {
        if let Some(close) = close {
            history.push(DailyClose {
                date: to_jst(*unixtime)?,
                close: *close,
            });
        }
    }

    Ok(Stock {
        // 正式名称がない銘柄（指数など）は略称で代用する
        name: meta
            .long_name
            .or(meta.short_name)
            .unwrap_or_else(|| meta.symbol.clone()),
        symbol: meta.symbol,
        price: meta.regular_market_price,
        volume: meta.regular_market_volume,
        history,
    })
}

/// 銘柄コードとして許可する文字か検証する。
///
/// symbol は URL のパスに埋め込まれるため、? や & や / を通すと
/// リクエスト先やクエリを差し替えられてしまう。
fn validate_symbol(symbol: &str) -> Result<(), Box<dyn Error>> {
    if symbol.is_empty() {
        return Err("銘柄コードが空です".into());
    }

    // 長さの確認は文字種より先に行う。長すぎる入力を弾く際に、
    // その中身をエラーメッセージへ載せないため。
    let len = symbol.chars().count();
    if len > MAX_SYMBOL_LEN {
        return Err(
            format!("銘柄コードが長すぎます（最大{MAX_SYMBOL_LEN}文字、指定は{len}文字）").into(),
        );
    }

    let is_allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '^' | '-' | '=');

    if let Some(c) = symbol.chars().find(|c| !is_allowed(*c)) {
        return Err(format!(
            "銘柄コード {} に使用できない文字 '{}' が含まれています（英数字と . ^ - = のみ）",
            sanitize::for_display(symbol),
            sanitize::char_for_display(c)
        )
        .into());
    }

    if !symbol.chars().any(|c| c.is_ascii_alphanumeric()) {
        return Err(format!(
            "銘柄コード {} が英数字を含んでいません",
            sanitize::for_display(symbol)
        )
        .into());
    }

    Ok(())
}

/// API が返す UNIX 秒を JST（UTC+9）の日時に変換する
fn to_jst(unixtime: i64) -> Result<DateTime<FixedOffset>, Box<dyn Error>> {
    let jst = FixedOffset::east_opt(9 * 3600).ok_or("タイムゾーンの生成に失敗しました")?;
    let datetime = jst
        .timestamp_opt(unixtime, 0)
        .single()
        .ok_or("時刻の変換に失敗しました")?;
    Ok(datetime)
}

#[cfg(test)]
mod tests {
    use super::validate_symbol;

    #[test]
    fn 実在する形式の銘柄コードを受け付ける() {
        for symbol in ["7203.T", "6758.T", "AAPL"] {
            assert!(validate_symbol(symbol).is_ok(), "{symbol} が弾かれた");
        }
    }

    #[test]
    fn url_の構造を変えうる文字を弾く() {
        for symbol in ["7203.T?range=1y", "7203.T&x=1", "../../etc"] {
            assert!(
                validate_symbol(symbol).is_err(),
                "{symbol} が通ってしまった"
            );
        }
    }

    #[test]
    fn retry_after_の秒数を読む() {
        use super::parse_retry_after;

        assert_eq!(parse_retry_after(Some("60")), Some(60));
        assert_eq!(parse_retry_after(Some(" 30 ")), Some(30));
        // 日時形式は解釈しない
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(parse_retry_after(None), None);
    }

    #[test]
    fn レート制限のエラーは待ち時間を含めて表示する() {
        use super::FetchError;

        let with_wait = FetchError::RateLimited {
            retry_after: Some(60),
        };
        assert!(with_wait.to_string().contains("60秒"));

        let without_wait = FetchError::RateLimited { retry_after: None };
        assert!(without_wait.to_string().contains("レート制限"));
    }

    #[test]
    fn 空文字や記号だけの値を弾く() {
        for symbol in ["", "..", "---"] {
            assert!(
                validate_symbol(symbol).is_err(),
                "{symbol} が通ってしまった"
            );
        }
    }
}
