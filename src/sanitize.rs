//! 端末へ出力する文字列の無害化。
//!
//! ユーザー入力をそのまま出力すると、含まれていたエスケープシーケンスが
//! 端末に解釈され、着色・カーソル移動・画面消去などを起こせてしまう。
//! 表示前に必ずこのモジュールを通す。

/// エラー表示に載せる最大文字数（エスケープ後の長さで数える）
const MAX_DISPLAY_LEN: usize = 30;

/// 端末の解釈を招く文字か。
fn is_dangerous(c: char) -> bool {
    // C0/C1 制御文字。ESC (\u{1b}) はここに含まれる
    c.is_control()
        // 双方向テキスト制御とゼロ幅文字。
        // 制御文字ではないが、表示順を偽装したり文字を隠したりできる
        || matches!(c,
            '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}')
}

/// 1文字を表示用に整える。危険な文字は \u{XX} 形式にする。
fn escape_char(c: char) -> String {
    if is_dangerous(c) {
        format!("\\u{{{:02x}}}", c as u32)
    } else {
        c.to_string()
    }
}

/// 危険な文字をエスケープする。長さは変えない。
///
/// 銘柄名など、長さを保ったまま表示したい文字列に使う。
pub fn escape(text: &str) -> String {
    text.chars().map(escape_char).collect()
}

/// エラーメッセージへ載せる文字を整える。
pub fn char_for_display(c: char) -> String {
    escape_char(c)
}

/// エラーメッセージへ載せるユーザー入力を整える。
///
/// エスケープに加えて長さも制限する。検証を通らなかった入力は
/// いくらでも長くなりうるため。
pub fn for_display(text: &str) -> String {
    let escaped = escape(text);

    if escaped.chars().count() <= MAX_DISPLAY_LEN {
        return escaped;
    }

    let head: String = escaped.chars().take(MAX_DISPLAY_LEN).collect();
    format!("{head}…(以下略)")
}

#[cfg(test)]
mod tests {
    use super::{char_for_display, escape, for_display};

    #[test]
    fn 制御文字をエスケープする() {
        assert_eq!(escape("\u{1b}[31mRED"), "\\u{1b}[31mRED");
        assert_eq!(escape("a\nb"), "a\\u{0a}b");
    }

    #[test]
    fn 双方向制御文字とゼロ幅文字をエスケープする() {
        assert_eq!(escape("a\u{202e}b"), "a\\u{202e}b");
        assert_eq!(escape("a\u{200b}b"), "a\\u{200b}b");
    }

    #[test]
    fn 通常の文字はそのまま残す() {
        assert_eq!(escape("7203.T"), "7203.T");
        assert_eq!(escape("トヨタ自動車"), "トヨタ自動車");
    }

    #[test]
    fn 短い入力はそのまま表示する() {
        assert_eq!(for_display("7203.T"), "7203.T");
    }

    #[test]
    fn 長い入力は切り詰めて表示する() {
        let actual = for_display(&"A".repeat(10_000));
        assert!(
            actual.chars().count() < 50,
            "切り詰められていない: {actual}"
        );
        assert!(actual.ends_with("…(以下略)"));
    }

    #[test]
    fn エスケープ後の長さで切り詰める() {
        // 制御文字1個が \u{1b} の6文字に膨らむため、6個で上限30文字を超える
        let actual = for_display(&"\u{1b}".repeat(6));
        assert!(actual.ends_with("…(以下略)"), "実際: {actual}");
    }

    #[test]
    fn 単体の文字も整える() {
        assert_eq!(char_for_display('\u{1b}'), "\\u{1b}");
        assert_eq!(char_for_display('?'), "?");
    }
}
