#!/usr/bin/env python3
"""macOS のメニューバーの語を、**Appleの訳から引く**（発明しない）。

`src-tauri/src/menu.rs` の表はこれで作る。出力をそのまま貼る。

**macOSでしか動かない**（OSに入っている `.loctable` を読むため）。
`python3 tools/menu-strings.py` で標準出力に出る。

## なぜ手で訳さないか

`ファイル` / `編集` / `ほかを非表示` は**OSの語**であって、こちらの言葉ではない。
6言語ぶん手で書けば、**どこかで他のMacアプリと違う語**になる——利用者から見れば
それは「訳が下手」ではなく「**このアプリだけ挙動が違う**」に見える。
`de` の `„%@“ ausblenden`（かぎ括弧つき）のような綴りは、手では出てこない。

## どこから引くか

- **`SwiftUI.framework/Resources/MainMenu.loctable`** … アプリメニューと編集メニュー。
  SwiftUI が既定のメニューバーを組むときに使う表そのもの
- **`AppKit.framework/Resources/MenuCommands.loctable`** … `Close Window` はこちらにしかない

**言語の綴りはOS側の綴り**（`zh_CN` / `zh_TW`）で、辞書のコード（`zh` / `zh-hant`）とは違う。
"""
import json, subprocess, sys

SWIFTUI = "/System/Library/Frameworks/SwiftUI.framework/Resources/MainMenu.loctable"
APPKIT = "/System/Library/Frameworks/AppKit.framework/Resources/MenuCommands.loctable"

# 辞書のコード → loctable の綴り
LOCALES = [
    ("en", "en"),
    ("ja", "ja"),
    ("de", "de"),
    ("es", "es"),
    ("es-419", "es_419"),
    ("zh", "zh_CN"),
    ("zh-hant", "zh_TW"),
]

# (Rustのフィールド名, loctableのキー, どちらの表か)
KEYS = [
    ("about", "About %@", SWIFTUI),
    ("services", "Services", SWIFTUI),
    ("hide", "Hide %@", SWIFTUI),
    ("hide_others", "Hide Others", SWIFTUI),
    ("quit", "Quit %@", SWIFTUI),
    ("file", "File", SWIFTUI),
    ("close_window", "Close Window", APPKIT),
    ("edit", "Edit", SWIFTUI),
    ("undo", "Undo", SWIFTUI),
    ("redo", "Redo", SWIFTUI),
    ("cut", "Cut", SWIFTUI),
    ("copy", "Copy", SWIFTUI),
    ("paste", "Paste", SWIFTUI),
    ("select_all", "Select All", SWIFTUI),
    ("view", "View", SWIFTUI),
    ("fullscreen", "Enter Full Screen", SWIFTUI),
    ("window", "Window", SWIFTUI),
    ("minimize", "Minimize", SWIFTUI),
    ("zoom", "Zoom", SWIFTUI),
    ("help", "Help", SWIFTUI),
]


def load(path: str) -> dict:
    out = subprocess.run(["plutil", "-convert", "json", "-o", "-", path],
                         capture_output=True, check=True).stdout
    return json.loads(out)


def main() -> int:
    tables = {p: load(p) for p in {SWIFTUI, APPKIT}}
    print("// **ここから下は `tools/menu-strings.py` が作る。手で直さない。**")
    print("// 出どころは macOS 自身の訳（SwiftUI の MainMenu.loctable と")
    print("// AppKit の MenuCommands.loctable）。**OSの語なので、こちらで訳さない。**")
    print()
    for code, spell in LOCALES:
        rows = []
        for field, key, path in KEYS:
            v = tables[path].get(spell, {}).get(key)
            if v is None:
                print(f"!! {spell} に {key!r} が無い", file=sys.stderr)
                return 1
            rows.append(f'    {field}: "{v}",')
        ident = code.upper().replace("-", "_")
        print(f"static {ident}: MenuText = MenuText {{")
        print("\n".join(rows))
        print("};")
        print()
    print("/// 辞書のコードと、その語。**`ui/src/i18n/index.ts` の `DICTS` と同じ顔ぶれ**")
    print("static TEXTS: &[(&str, &MenuText)] = &[")
    for code, _ in LOCALES:
        ident = code.upper().replace("-", "_")
        print(f'    ("{code}", &{ident}),')
    print("];")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
