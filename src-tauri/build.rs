fn main() {
    tauri_build::build();

    // Hand the lib target's test binary the same Windows manifest the app gets.
    //
    // `rfd` (pulled in by `tauri-plugin-dialog`) statically imports `TaskDialogIndirect`,
    // and that function exists **only in comctl32 version 6**. Binding to it requires an
    // application manifest asking for `Microsoft.Windows.Common-Controls`; without one the
    // loader resolves against the 5.82 copy in System32, which does not export it, and the
    // process dies with STATUS_ENTRYPOINT_NOT_FOUND **before `main` runs**. No test failure
    // is reported, because no test ever starts.
    //
    // `tauri_build::build()` emits the compiled resource as `rustc-link-arg-bins`, so the
    // **bin** target's test harness gets the manifest and the **lib** target's does not.
    // Same crate, same dependencies, same imports — only the manifest differs (measured
    // 2026-09-01: `pictkura-*.exe` starts, `pictkura_lib-*.exe` exits 0xc0000139).
    //
    // **`rustc-link-arg-tests` does not work here.** It only reaches `[[test]]` targets —
    // the files under `tests/` — and this crate has none, so cargo refuses the whole build
    // with "does not have a test target". There is no instruction scoped to a lib's own
    // unit-test harness, so the only way in is the unscoped `rustc-link-arg`, which also
    // hits the bin. The bin already links the resource through `rustc-link-arg-bins`, so it
    // ends up with **a second copy of the manifest** (measured 2026-09-01: `pictkura.exe`
    // goes from 2 to 3 RT_MANIFEST entries). Both the app and its test harness still build
    // and start, but the binary is no longer byte-for-byte what it was.
    //
    // **That is why this is limited to the GNU toolchain.** A build script is compiled for
    // the host, so `target_env` here is the toolchain doing the building. Releases and CI
    // are msvc and never take this branch — **the binaries we ship are untouched** — while
    // people building with `*-pc-windows-gnu` get a test suite that runs at all. Prefer
    // matching the shipped toolchain (`rustup default stable-msvc`) if that is an option;
    // this exists so the GNU path is not silently broken.
    #[cfg(all(windows, target_env = "gnu"))]
    {
        // Nothing to add if the variable is missing — better a link error naming the real
        // problem than a panic in the build script (`unwrap`/`expect` are denied here)
        if let Ok(out) = std::env::var("OUT_DIR") {
            let resource = std::path::Path::new(&out).join("libresource.a");
            if resource.exists() {
                println!("cargo:rustc-link-arg={}", resource.display());
            }
        }
    }
}
