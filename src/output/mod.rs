//! 標準出力への表示をまとめたモジュール。
//!
//! 中身は table.rs にあるが、呼び出し側からは
//! output::print_rows のように使えるよう再公開している。

mod table;

pub use table::{Row, print_empty, print_header, print_rows};
