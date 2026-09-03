//! ゴールデンクロス検出 CLI のエントリポイント。
//!
//! 実際の処理は各モジュールに任せ、ここでは
//! 「引数を読む → 並列取得 → クロスを絞り込む → 出来高順に表示」だけを行う。

mod cli;
mod cross;
mod output;
mod sanitize;
mod stock;
mod universe;
mod yahoo;

use reqwest::blocking::Client;
use std::error::Error;
use std::io::IsTerminal;
use std::thread;
use std::time::Duration;

use cli::Args;
use output::Row;
use stock::Stock;
use universe::Entry;

/// 取得する期間。75日移動平均に必要な件数を十分に上回る長さを取る。
const RANGE: &str = "1y";

/// 接続確立までの上限
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// 1リクエスト全体（接続・送信・受信）の上限
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse_and_validate();

    let entries = target_entries(&args);
    if entries.is_empty() {
        return Err("走査する銘柄がありません".into());
    }

    // Yahoo は User-Agent がないとリクエストを弾くことがあるので付けておく。
    // Client は使い回すと接続を再利用でき、スレッド間で共有もできる。
    let client = Client::builder()
        .user_agent("golden-cross/0.1")
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()?;

    // 進捗表示は端末のときだけ。リダイレクト先に \r が混ざるのを避ける
    let show_progress = std::io::stderr().is_terminal();

    let mut rows: Vec<Row> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let total = entries.len();
    let mut done = 0;

    for chunk in entries.chunks(args.concurrency) {
        for (entry, result) in chunk.iter().zip(fetch_chunk(&client, chunk)) {
            match result {
                Ok(stock) => {
                    // クロスが「直近 within 営業日以内」の銘柄だけ残す。
                    // days_ago が 0 なら最新日のクロス。
                    if let Some(cross) = cross::find_latest(&stock)
                        && cross.days_ago < args.within
                    {
                        rows.push(Row { stock, cross });
                    }
                }
                // 同梱リストの名称があれば添える（どの銘柄か分かりやすくするため）
                Err(e) => errors.push(match entry.name.is_empty() {
                    true => format!("{}: {e}", sanitize::for_display(&entry.symbol)),
                    false => format!(
                        "{} {}: {e}",
                        sanitize::for_display(&entry.symbol),
                        sanitize::escape(&entry.name)
                    ),
                }),
            }
        }

        done += chunk.len();
        // 進捗は標準エラー出力へ。表本体をパイプで加工しても混ざらない
        if show_progress {
            eprint!("\r取得中 {done}/{total}");
        }
    }
    if show_progress {
        eprintln!();
    }

    // 1件も取得できなかったのは設定や通信の問題とみなす
    if errors.len() == total {
        return Err(format!("{total}銘柄すべての取得に失敗しました").into());
    }

    // 出来高の多い順に並べ、上位だけ残す
    rows.sort_by_key(|row| std::cmp::Reverse(row.stock.volume));
    let hits = rows.len();
    rows.truncate(args.top);

    output::print_header(args.within, total - errors.len(), hits);

    if rows.is_empty() {
        output::print_empty(args.within);
    } else {
        output::print_rows(&rows);
    }

    report_errors(&errors, args.show_errors);

    Ok(())
}

/// 走査対象を決める。--symbols があればそちら、なければ同梱リスト。
fn target_entries(args: &Args) -> Vec<Entry> {
    if args.symbols.is_empty() {
        return universe::embedded();
    }

    args.symbols
        .iter()
        .map(|symbol| Entry {
            symbol: symbol.clone(),
            name: String::new(),
        })
        .collect()
}

/// 複数銘柄を並列に取得する。戻り値は引数と同じ順番に並ぶ。
///
/// yahoo::fetch のエラー型 Box<dyn Error> は Send ではなく、
/// そのままではスレッド境界を越えられない。文字列に変換して返す。
fn fetch_chunk(client: &Client, entries: &[Entry]) -> Vec<Result<Stock, String>> {
    thread::scope(|scope| {
        let handles: Vec<_> = entries
            .iter()
            .map(|entry| {
                // scope 内のスレッドは呼び出し元の変数を借用できる。
                // スコープを抜けるまでに必ず join されるため 'static は要らない。
                scope.spawn(move || {
                    yahoo::fetch(client, &entry.symbol, RANGE).map_err(|e| e.to_string())
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err("取得スレッドが異常終了しました".to_string()))
            })
            .collect()
    })
}

/// 取得に失敗した銘柄を標準エラー出力へ報告する
fn report_errors(errors: &[String], show_details: bool) {
    if errors.is_empty() {
        return;
    }

    if show_details {
        eprintln!("\n取得に失敗した銘柄 {}件:", errors.len());
        for error in errors {
            eprintln!("  {error}");
        }
    } else {
        eprintln!(
            "\n{}銘柄の取得に失敗しました（--show-errors で詳細）",
            errors.len()
        );
    }
}
