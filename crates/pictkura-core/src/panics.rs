//! **1枚の失敗で全体を道連れにしない**ための網（`dev/loadmap.md` 1.3）。
//!
//! 読み取りはどれも `Option` / `Result` を返す形にしてあり、壊れたファイルは
//! そこで止まる。それでも網が要るのは、**自分で書いていない解読器**を通るため
//! ——JPEG（libjpeg-turbo）・AVIF（rav1d）・PNG/GIF/TIFF（`image`）は、
//! 規格外のバイト列に当たると `Err` ではなく**パニックで**知らせてくることがある。
//!
//! パニックをそのままにすると、**作業スレッドが1本死ぬ**。サムネイルの
//! ワーカーで起きれば、そのIDは「処理中」のまま残ってキューが詰まり、
//! 以後そのフォルダのサムネイルが永久に出てこない。**1枚が失敗しただけ**へ
//! 均すのがここの役目。
//!
//! **握りつぶすためのものではない**。捕まえたことは `stderr` に出す
//! ——黙って絵が出ないのは、落ちるより追いかけにくい。
//!
//! **ただし配布ビルドでは、その `stderr` がどこにも届かない**（ゲート2の指摘）。
//! Windowsの配布物はコンソールを持たない（`windows_subsystem = "windows"`）ので、
//! 開発中にターミナルから起動したときしか読めない。捕まえたラベルを
//! **DBの隣のログファイルへ残す**のは次の作業に置く——利用者はまだ0名で、
//! いま要るのは「落ちないこと」の方が先だと判断した。

use std::panic::AssertUnwindSafe;

/// `f` がパニックしたら `None` にして、何が起きたかを `stderr` に残す。
///
/// `label` には**どのファイルで起きたか**を入れる（IDやパス）。これが無いと、
/// 数万枚の中のどれが地雷なのか分からず、直しようがない。
///
/// `AssertUnwindSafe` を使う理由: 失敗したときは**結果を丸ごと捨てる**ので、
/// 途中まで書き換えた状態を外へ持ち出さない。DBの接続は途中で巻き戻っても
/// そのまま使える（rusqlite は文の途中で落ちても接続を壊さない）。
pub fn catching<T>(label: &str, f: impl FnOnce() -> T) -> Option<T> {
    match std::panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => Some(value),
        Err(payload) => {
            eprintln!("パニックを捕まえた（{label}）: {}", describe(&payload));
            None
        }
    }
}

/// パニックの中身を人が読める形にする。
///
/// `panic!("...")` の文字列は `&str` か `String` のどちらかで届く。
/// それ以外（`panic_any`）は型が分からないので、そう書く。
fn describe(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "（内容の分からないパニック）".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn パニックは値なしに均される() {
        let out = catching("テスト", || -> i32 { panic!("わざと落とす") });
        assert_eq!(out, None);
    }

    #[test]
    fn 落ちなければそのまま返る() {
        assert_eq!(catching("テスト", || 1 + 1), Some(2));
    }

    /// 添字外れ（壊れたファイルで実際に起きる形）も捕まること。
    #[test]
    fn 添字外れも捕まえる() {
        let empty: Vec<u8> = Vec::new();
        let out = catching("テスト", || empty[3]);
        assert_eq!(out, None);
    }
}
