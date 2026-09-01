fn main() {
    tauri_build::build();

    // Give the lib target's test binary the Common-Controls dependency the app already has.
    //
    // `rfd` (pulled in by `tauri-plugin-dialog`) statically imports `TaskDialogIndirect`,
    // and only comctl32 **version 6** exports it — measured 2026-09-01 on Windows 11 26200:
    // `System32\comctl32.dll` and the WinSxS 5.82 assembly export no `TaskDialog*` at all,
    // the 6.0 assembly exports three. Binding to version 6 takes an application manifest
    // naming `Microsoft.Windows.Common-Controls`; without one the loader resolves against
    // 5.82 and the process dies with STATUS_ENTRYPOINT_NOT_FOUND **before `main` runs**.
    // No test fails, because no test starts.
    //
    // The lib harness is **not** manifest-less — rustc gives every executable one. What it
    // lacks is the Common-Controls dependency, which lives in the resource `tauri-build`
    // compiles and then emits as `rustc-link-arg-bins`: bins get it, the lib harness does
    // not. Measured RT_MANIFEST counts on this host, before and after this file's change:
    //
    //     pictkura_lib-*.exe (lib harness)   1 -> 2   <- what we are here for
    //     pictkura-*.exe     (bin harness)   2 -> 3
    //     pictkura.exe       (the app)       2 -> 3
    //
    // **`rustc-link-arg-tests` does not work here.** It reaches only `[[test]]` targets and
    // this crate has none, so cargo refuses the whole build ("does not have a test target").
    // Adding a file under `tests/` would not help either — a lib's own unit-test harness is
    // not a test target, so it would still be missed. The unscoped `rustc-link-arg` is the
    // only way in, and it hands every non-rlib target the **whole** resource: the bin ends
    // up with a second copy of the manifest, and the lib harness picks up an icon and a
    // VERSIONINFO it has no use for. The icon and VERSIONINFO do **not** double on the bin —
    // only the manifest does. That asymmetry is measured, not reasoned about.
    //
    // A narrower fix exists: compile a manifest-only `.rc` of our own instead of relinking
    // tauri's resource. It would drop the icon/VERSIONINFO the harness does not need and cut
    // the dependency on tauri-build's private filename below. It costs a direct
    // `embed-resource` build-dependency, which is not obviously worth it for a path only
    // developers on one toolchain take.
    //
    // **Ask about the target, not the host.** A build script is compiled and run on the
    // machine doing the building, so `cfg!(windows)` and `cfg!(target_env = "gnu")` written
    // here answer for *that* machine, not for the binary being produced. Cross-compiling to
    // `*-pc-windows-gnu` would then skip the workaround and stay broken. Cargo passes the
    // real target through the environment, so read it from there (gate 1, P2). This also
    // covers `*-pc-windows-gnullvm`, whose `target_env` is `gnu` and which needs the same
    // thing.
    //
    // **What we publish is untouched**: `release.yml` builds on `windows-latest`, which is
    // msvc, and an msvc target never enters this branch. That is a property of the release
    // workflow, not of the repository — nothing pins the toolchain (there is no
    // `rust-toolchain.toml`), and `tools/release.ps1` runs a bare `cargo tauri build`, so
    // running it by hand on a machine whose default is gnu does produce the extra manifest.
    // Matching the shipped toolchain (`rustup default stable-msvc`) is the better answer for
    // anyone who can; this exists so the gnu path is not silently broken.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "gnu" {
        // `OUT_DIR` as an `OsString`, so a path that is not valid UTF-8 still resolves
        let resource = std::env::var_os("OUT_DIR")
            .map(|out| std::path::Path::new(&out).join("libresource.a"))
            .filter(|path| path.exists());
        match resource {
            Some(path) => println!("cargo:rustc-link-arg={}", path.display()),
            // **Say it out loud rather than skip quietly.** `libresource.a` is tauri-build's
            // own filename, not a documented contract; if it is renamed or stops being
            // written, this branch does nothing and the harness goes back to dying before
            // `main`. Nothing else would catch that — the tests are the only check there is,
            // and CI never builds for a gnu target, so it stays green while the developer in
            // front of it gets an exit code and no explanation. A warning is that
            // explanation. Panicking here is not an option (`unwrap`/`expect` are denied)
            // and would be wrong anyway: a missing workaround should not stop the build.
            None => println!(
                "cargo:warning=windows-gnu: tauri-build's compiled resource was not found in \
                 OUT_DIR, so the Common-Controls manifest is not linked into the test \
                 binaries. `cargo test -p pictkura --lib` will exit 0xc0000139 before it runs \
                 a single test."
            ),
        }
    }
}
