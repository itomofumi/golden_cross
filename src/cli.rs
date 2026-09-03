//! コマンドライン引数の定義。

use clap::{CommandFactory, Parser};

/// 同時に投げるリクエスト数の上限。
///
/// 増やしすぎると Yahoo からレート制限を受ける。
pub const MAX_CONCURRENCY: usize = 16;

#[derive(Parser)]
#[command(
    version,
    about = "25日線と75日線のゴールデンクロスが起きた日本株を、出来高順に表示する",
    after_help = "\
判定方法:
  前日に「25日線 <= 75日線」だったものが、当日「25日線 > 75日線」に
  なった日をゴールデンクロスとみなす。
  --within で指定した営業日数以内にクロスした銘柄を、出来高の多い順に並べる。

走査対象:
  同梱の主要銘柄リスト（東証239銘柄）。Yahoo に日本株の出来高
  ランキングを返すエンドポイントがないため、この範囲での順位となる。
  --symbols で対象を指定すればリストを差し替えられる。

例:
  golden_cross                        直近5営業日のクロスを出来高上位10件
  golden_cross --within 20            直近20営業日まで広げる
  golden_cross --top 3                上位3件だけ表示
  golden_cross --symbols 7203.T 6758.T  対象を指定して調べる

終了コード:
  0 = 正常終了（該当0件でも0）   1 = 全銘柄の取得に失敗   2 = 引数が不正"
)]
pub struct Args {
    /// 表示する件数
    #[arg(short, long, default_value_t = 10)]
    pub top: usize,

    /// 何営業日以内のクロスを対象にするか
    #[arg(short, long, default_value_t = 5)]
    pub within: usize,

    /// 同時に取得する銘柄数
    #[arg(short, long, default_value_t = 8)]
    pub concurrency: usize,

    /// 走査する銘柄コード（省略時は同梱の主要銘柄リスト）
    #[arg(short, long, num_args = 1..)]
    pub symbols: Vec<String>,

    /// 取得に失敗した銘柄を一覧表示する
    #[arg(long)]
    pub show_errors: bool,
}

impl Args {
    /// 引数を読み込み、clap だけでは表現できない制約を検証する。
    pub fn parse_and_validate() -> Self {
        let args = Self::parse();

        if args.top == 0 {
            Self::exit_with_error("--top には1以上を指定してください");
        }

        if args.concurrency == 0 || args.concurrency > MAX_CONCURRENCY {
            Self::exit_with_error(&format!(
                "--concurrency には1〜{MAX_CONCURRENCY}を指定してください（指定は{}）",
                args.concurrency
            ));
        }

        args
    }

    /// clap 本来の書式でエラーを表示して終了する（終了コード2）
    fn exit_with_error(message: &str) -> ! {
        Self::command()
            .error(clap::error::ErrorKind::InvalidValue, message)
            .exit()
    }
}
