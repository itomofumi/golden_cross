//! CLI としての振る舞いを外側から確認する結合テスト。
//!
//! 単体テストは各モジュール内の #[cfg(test)] に置いている。
//! ここでは実際にバイナリを起動し、引数・終了コード・出力を確認する。
//!
//! 通信は行わない。株価が必要なテストは、あらかじめキャッシュへ
//! フィクスチャを置き、そこから読ませている。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// テストごとに別のキャッシュ置き場を使うための連番
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 用意したキャッシュ置き場でバイナリを実行する。
///
/// XDG_CACHE_HOME をテスト専用のディレクトリに向けるため、
/// 利用者の ~/.cache には触らない。
fn run(args: &[&str], cache_home: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_golden_cross"))
        .args(args)
        .env("XDG_CACHE_HOME", cache_home)
        .output()
        .expect("バイナリを起動できなかった")
}

/// このテスト専用の空ディレクトリを作る
fn temp_dir(label: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "golden_cross_test_{}_{label}_{id}",
        std::process::id()
    ));

    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("一時ディレクトリを作れなかった");

    dir
}

/// フィクスチャをキャッシュとして配置する。
///
/// キャッシュは「取得日が当日なら使う」ため、今書けばそのまま読まれる。
fn place_fixture(cache_home: &Path, range: &str) {
    let dir = cache_home.join("golden_cross").join(range);
    fs::create_dir_all(&dir).expect("キャッシュ置き場を作れなかった");

    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chart_test_t.json"
    );
    fs::copy(fixture, dir.join("TEST.T.json")).expect("フィクスチャを配置できなかった");
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn ヘルプは使い方と判定方法を示す() {
    let cache = temp_dir("help");
    let output = run(&["--help"], &cache);

    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("Usage: golden_cross"), "実際: {text}");
    assert!(text.contains("判定方法"), "実際: {text}");
}

#[test]
fn バージョンを表示できる() {
    let cache = temp_dir("version");
    let output = run(&["--version"], &cache);

    assert!(output.status.success());
    assert!(stdout(&output).contains("golden_cross"));
}

#[test]
fn 表示件数に0を指定すると引数エラーになる() {
    let cache = temp_dir("top0");
    let output = run(&["--top", "0"], &cache);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--top には1以上"),
        "実際: {}",
        stderr(&output)
    );
}

#[test]
fn 同時取得数が上限を超えると引数エラーになる() {
    let cache = temp_dir("conc");

    for value in ["0", "9"] {
        let output = run(&["--concurrency", value], &cache);

        assert_eq!(output.status.code(), Some(2), "--concurrency {value}");
        assert!(stderr(&output).contains("--concurrency には1〜8"));
    }
}

#[test]
fn 知らないオプションは引数エラーになる() {
    let cache = temp_dir("unknown");
    let output = run(&["--nonexistent"], &cache);

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn 不正な銘柄コードは通信せずに失敗する() {
    let cache = temp_dir("badsymbol");
    let output = run(&["--symbols", ".."], &cache);

    // 検証で弾かれるため1件も取得できず、終了コード1で終わる
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("すべての取得に失敗"),
        "実際: {}",
        stderr(&output)
    );
}

#[test]
fn キャッシュからゴールデンクロスを検出する() {
    let cache = temp_dir("hit");
    place_fixture(&cache, "6mo");

    // フィクスチャのクロスは末尾から8営業日前。--within 30 なら対象になる
    let output = run(&["--symbols", "TEST.T", "--within", "30"], &cache);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("TEST.T"), "実際: {text}");
    assert!(text.contains("Test Corporation"), "実際: {text}");
    assert!(
        text.contains("1,234,567"),
        "出来高が3桁区切りで出ていない: {text}"
    );
    assert!(text.contains("1銘柄中 1件"), "実際: {text}");
}

#[test]
fn 期間の外のクロスは対象にしない() {
    let cache = temp_dir("miss");
    place_fixture(&cache, "6mo");

    // クロスは8営業日前なので、--within 3 では対象外
    let output = run(&["--symbols", "TEST.T", "--within", "3"], &cache);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("1銘柄中 0件"), "実際: {text}");
    assert!(text.contains("ありませんでした"), "実際: {text}");
}

#[test]
fn キャッシュを使うと通信しない() {
    let cache = temp_dir("cached");
    place_fixture(&cache, "6mo");

    let output = run(&["--symbols", "TEST.T", "--within", "30"], &cache);

    assert!(
        stderr(&output).contains("キャッシュ利用 1件 / 取得 0件"),
        "実際: {}",
        stderr(&output)
    );
}

#[test]
fn 遡る日数に応じて別の期間のキャッシュを見る() {
    let cache = temp_dir("range");
    // --within 100 は 1y を要求する。1y にだけ置いて、そこから読めることを確認する
    place_fixture(&cache, "1y");

    let output = run(&["--symbols", "TEST.T", "--within", "100"], &cache);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("キャッシュ利用 1件"),
        "実際: {}",
        stderr(&output)
    );
}
