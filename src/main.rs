//! ゴールデンクロス検出 CLI のエントリポイント。
//!
//! 実際の処理は各モジュールに任せ、ここでは
//! 「引数を読む → 並列取得 → クロスを絞り込む → 出来高順に表示」だけを行う。

mod cache;
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

use cache::Cache;
use cli::Args;
use output::Row;
use stock::Stock;
use universe::Entry;
use yahoo::FetchError;

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

    let cache = Cache::new(!args.no_cache);

    // 取得期間は --within から決める。既定なら 6mo で足りる
    let range = cross::required_range(args.within);

    let mut rows: Vec<Row> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let total = entries.len();
    let mut done = 0;
    let mut succeeded = 0;
    // レート制限を受けたら Some になり、以降のチャンクは取得しない
    let mut rate_limited: Option<Option<u64>> = None;

    for chunk in entries.chunks(args.concurrency) {
        for (entry, result) in chunk.iter().zip(fetch_chunk(&client, chunk, &cache, range)) {
            match result {
                Ok(stock) => {
                    succeeded += 1;

                    // クロスが「直近 within 営業日以内」の銘柄だけ残す。
                    // days_ago が 0 なら最新日のクロス。
                    if let Some(cross) = cross::find_latest(&stock)
                        && cross.days_ago < args.within
                    {
                        rows.push(Row { stock, cross });
                    }
                }
                // レート制限は個別の失敗として数えず、全体の中断理由として扱う
                Err(FetchError::RateLimited { retry_after }) => {
                    rate_limited = Some(retry_after);
                }
                Err(e) => errors.push(describe_error(entry, &e)),
            }
        }

        done += chunk.len();
        // 進捗は標準エラー出力へ。表本体をパイプで加工しても混ざらない
        if show_progress {
            eprint!("\r取得中 {done}/{total}");
        }

        // 弾かれると分かっているリクエストを投げ続けない。
        // 同じチャンク内のスレッドは既に走っているため、区切りは chunk 単位。
        if rate_limited.is_some() {
            break;
        }
    }
    if show_progress {
        eprintln!();
    }

    if let Some(retry_after) = rate_limited {
        report_rate_limit(done, total, retry_after);
    }

    // 1件も取得できなかったのは設定や通信の問題とみなす
    if succeeded == 0 {
        return Err(format!("{done}銘柄すべての取得に失敗しました").into());
    }

    // 出来高の多い順に並べ、上位だけ残す
    rows.sort_by_key(|row| std::cmp::Reverse(row.stock.volume));
    let hits = rows.len();
    rows.truncate(args.top);

    report_cache(&cache, succeeded);

    output::print_header(args.within, succeeded, hits);

    if rows.is_empty() {
        output::print_empty(args.within);
    } else {
        output::print_rows(&rows);
    }

    report_errors(&errors, args.show_errors);

    // 中断した回は結果が不完全なので、正常終了と区別できるようにする
    if rate_limited.is_some() {
        std::process::exit(1);
    }

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
/// FetchError は Send なのでそのままスレッド境界を越えられる。
fn fetch_chunk(
    client: &Client,
    entries: &[Entry],
    cache: &Cache,
    range: &str,
) -> Vec<Result<Stock, FetchError>> {
    thread::scope(|scope| {
        let handles: Vec<_> = entries
            .iter()
            .map(|entry| {
                // scope 内のスレッドは呼び出し元の変数を借用できる。
                // スコープを抜けるまでに必ず join されるため 'static は要らない。
                scope.spawn(move || yahoo::fetch(client, &entry.symbol, range, cache))
            })
            .collect();

        handles
            .into_iter()
            .map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    Err(FetchError::Other(
                        "取得スレッドが異常終了しました".to_string(),
                    ))
                })
            })
            .collect()
    })
}

/// 取得に失敗した銘柄を、同梱リストの名称を添えて説明する
fn describe_error(entry: &Entry, error: &FetchError) -> String {
    let symbol = sanitize::for_display(&entry.symbol);

    if entry.name.is_empty() {
        format!("{symbol}: {error}")
    } else {
        format!("{symbol} {}: {error}", sanitize::escape(&entry.name))
    }
}

/// レート制限による中断を標準エラー出力へ報告する
fn report_rate_limit(done: usize, total: usize, retry_after: Option<u64>) {
    eprintln!("レート制限を受けたため取得を中断しました（{done}/{total}銘柄）");

    match retry_after {
        Some(seconds) => eprintln!("{seconds}秒ほど待ってから再実行してください"),
        None => eprintln!("しばらく待ってから再実行してください"),
    }

    eprintln!("取得済みの分はキャッシュに残るため、再実行時は続きから取得します");
}

/// キャッシュの利用状況を標準エラー出力へ報告する
fn report_cache(cache: &Cache, total: usize) {
    if cache.is_disabled() {
        return;
    }

    let hits = cache.hits();
    if hits > 0 {
        eprintln!("キャッシュ利用 {hits}件 / 取得 {}件", total - hits);
        // 終値と移動平均は日中に変わらないが、株価と出来高は変わる。
        // キャッシュを使った回はその時点の値になる。
        eprintln!("※ 株価と出来高はキャッシュ取得時点の値です（--no-cache で取得し直せます）");
    }
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
