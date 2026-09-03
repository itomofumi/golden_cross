//! 走査対象の銘柄一覧（ユニバース）。
//!
//! Yahoo には日本株の出来高ランキングを返すエンドポイントがない
//! （predefined screener は米国株専用、カスタム検索は認証が必要）。
//! そのため主要銘柄の一覧を同梱し、この範囲を走査して順位付けする。

/// 同梱のユニバース。1行が「銘柄コード <TAB> 名称」。
///
/// include_str! はコンパイル時にファイルを文字列として埋め込む。
/// 実行時にファイルを探さないので、バイナリ単体で動く。
const EMBEDDED: &str = include_str!("../data/universe.tsv");

/// ユニバースの1件
pub struct Entry {
    pub symbol: String,
    pub name: String,
}

/// 同梱のユニバースを読み込む
pub fn embedded() -> Vec<Entry> {
    parse(EMBEDDED)
}

/// TSV 形式の文字列をユニバースとして解釈する。
///
/// 名称の欄がない行は銘柄コードだけの行として扱う。
fn parse(text: &str) -> Vec<Entry> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| match line.split_once('\t') {
            Some((symbol, name)) => Entry {
                symbol: symbol.trim().to_string(),
                name: name.trim().to_string(),
            },
            None => Entry {
                symbol: line.to_string(),
                name: String::new(),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{embedded, parse};

    #[test]
    fn 同梱ユニバースを読み込める() {
        let entries = embedded();

        assert!(entries.len() > 200, "件数が少なすぎる: {}", entries.len());
        assert!(entries.iter().all(|e| e.symbol.ends_with(".T")));
        assert!(entries.iter().all(|e| !e.name.is_empty()));
    }

    #[test]
    fn 空行とコメント行を無視する() {
        let entries = parse("# コメント\n\n7203.T\tトヨタ\n");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].symbol, "7203.T");
        assert_eq!(entries[0].name, "トヨタ");
    }

    #[test]
    fn 名称のない行も読める() {
        let entries = parse("7203.T\n");

        assert_eq!(entries[0].symbol, "7203.T");
        assert_eq!(entries[0].name, "");
    }
}
