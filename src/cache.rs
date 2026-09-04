//! 取得結果のディスクキャッシュ。
//!
//! 日足データは1日1回しか変わらないため、同じ営業日のうちは
//! 保存したレスポンスを再利用して通信を省く。
//!
//! キャッシュの読み書きに失敗しても処理は止めない。
//! 速くするための仕組みであって、成否を左右するものではないため。

use chrono::{DateTime, Local};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

/// キャッシュ置き場。$XDG_CACHE_HOME か ~/.cache の下に作る。
const DIR_NAME: &str = "golden_cross";

pub struct Cache {
    /// 置き場所。決められなかった場合は None（キャッシュなしで動く）
    dir: Option<PathBuf>,
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl Cache {
    /// 既定の置き場所でキャッシュを用意する。
    /// enabled が false のときは何もしないキャッシュになる。
    pub fn new(enabled: bool) -> Self {
        let dir = if enabled { default_dir() } else { None };

        Self {
            dir,
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
        }
    }

    /// 保存済みのレスポンスを返す。
    ///
    /// 取得日が今日でないものは古いとみなして無視する。
    pub fn read(&self, symbol: &str, range: &str) -> Option<String> {
        let path = self.path(symbol, range)?;

        let modified = fs::metadata(&path).ok()?.modified().ok()?;
        if !is_fresh(modified, SystemTime::now()) {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }

        match fs::read_to_string(&path) {
            Ok(body) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(body)
            }
            Err(_) => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// レスポンスを保存する。失敗しても黙って諦める。
    pub fn write(&self, symbol: &str, range: &str, body: &str) {
        let Some(path) = self.path(symbol, range) else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }

        // 複数スレッドから同時に書いても壊れないよう、
        // 一時ファイルへ書いてから置き換える
        let temp = path.with_extension(format!("tmp{}", std::process::id()));
        if fs::write(&temp, body).is_ok() && fs::rename(&temp, &path).is_err() {
            let _ = fs::remove_file(&temp);
        }
    }

    /// キャッシュから読めた件数
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }

    /// キャッシュが無効、または置き場所を決められなかったか
    pub fn is_disabled(&self) -> bool {
        self.dir.is_none()
    }

    /// 1件分の保存先。銘柄コードは検証済み（英数字と . ^ - =）なので
    /// パス区切りは含まれないが、念のため区切りだけは弾く。
    fn path(&self, symbol: &str, range: &str) -> Option<PathBuf> {
        if symbol.contains(['/', '\\']) || symbol.is_empty() {
            return None;
        }

        Some(
            self.dir
                .as_ref()?
                .join(range)
                .join(format!("{symbol}.json")),
        )
    }
}

/// 取得日が今日と同じなら使える、とみなす。
///
/// 日足は日中に増えないため、当日中に取り直しても内容は変わらない
/// （当日の終値が確定していない点はキャッシュの有無にかかわらず同じ）。
fn is_fresh(modified: SystemTime, now: SystemTime) -> bool {
    let modified: DateTime<Local> = modified.into();
    let now: DateTime<Local> = now.into();

    modified.date_naive() == now.date_naive()
}

/// $XDG_CACHE_HOME/golden_cross、なければ $HOME/.cache/golden_cross
fn default_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME")
        && !xdg.is_empty()
    {
        return Some(Path::new(&xdg).join(DIR_NAME));
    }

    let home = std::env::var("HOME").ok()?;
    if home.is_empty() {
        return None;
    }

    Some(Path::new(&home).join(".cache").join(DIR_NAME))
}

#[cfg(test)]
mod tests {
    use super::{Cache, is_fresh};
    use std::time::{Duration, SystemTime};

    #[test]
    fn 同じ日に取得したものは使える() {
        let now = SystemTime::now();

        assert!(is_fresh(now, now));
        assert!(is_fresh(now - Duration::from_secs(60), now));
    }

    #[test]
    fn 前日以前に取得したものは使わない() {
        let now = SystemTime::now();
        let two_days_ago = now - Duration::from_secs(2 * 24 * 3600);

        assert!(!is_fresh(two_days_ago, now));
    }

    #[test]
    fn 無効にすると置き場所を持たない() {
        let cache = Cache::new(false);

        assert!(cache.is_disabled());
        assert_eq!(cache.read("7203.T", "6mo"), None);
    }

    #[test]
    fn パス区切りを含む銘柄コードは保存先を作らない() {
        let cache = Cache::new(true);

        assert_eq!(cache.path("../etc/passwd", "6mo"), None);
        assert_eq!(cache.path("", "6mo"), None);
    }
}
