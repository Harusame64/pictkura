//! 開発用の調べ道具。**配布物には入らない**。
//!
//! 渡したパスのドライブが「シークに時間がかかる」と OS が答えるか（pictkura-dev #73）:
//! `cargo run -p pictkura-core --example seek_penalty -- C:\ D:\ G:\ J:\`
fn main() {
    for arg in std::env::args().skip(1) {
        let answer = pictkura_core::seek::incurs_seek_penalty(std::path::Path::new(&arg));
        println!("{arg}\t{answer:?}");
    }
}
