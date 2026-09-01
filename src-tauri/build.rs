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
    // The lib harness is **not** manifest-less — the toolchain links a default one in
    // (longPathAware + asInvoker, 599 bytes). What it lacks is the Common-Controls
    // dependency, which lives in the resource `tauri-build`
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
    // up with a second copy of **everything** — manifest, icon, group icon and VERSIONINFO —
    // and the lib harness picks up an icon and a VERSIONINFO it has no use for.
    //
    // **Count the leaves, not the names.** ld puts the duplicates at different depths: the
    // manifests land as extra *name* entries (2 -> 3), the rest as extra *language* entries
    // under one name. `EnumResourceNames` alone therefore reports icon and VERSIONINFO
    // unchanged and only the manifest moving, which is wrong and cost gate 2 a round trip.
    // Descending with `EnumResourceLanguages` shows the real picture on the bin harness:
    // RT_ICON 1 -> 2, RT_GROUP_ICON 1 -> 2, RT_VERSION 1 -> 2, RT_MANIFEST 2 -> 3.
    // `FindResourceEx` resolves type -> name -> language, so the second copies are
    // unreachable dead weight rather than a behaviour change.
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
        // **The branch is the target's, but the filename -- and what is inside it -- is the
        // host's.** `embed-resource` is a build-dependency, so it is compiled for the machine
        // doing the building and its `cfg`s answer for that machine (`lib.rs:146-151`):
        //
        // - windows-gnu host -> `libresource.a` (`windows_not_msvc.rs:54`), a COFF object.
        //   This is the path measured working on Windows 11. **One target inside it is not
        //   COFF**: `Compiler::choose` (`windows_not_msvc.rs:69-77`) picks `llvm-rc` for
        //   `aarch64-*-gnullvm` and still writes the `.a` name, so an ARM64 gnullvm build
        //   gets a `.res` under it. Nothing is gained by excluding it here -- `tauri-build`
        //   hands the same file to the bin link, so the app build hits it first -- but the
        //   claim above is not true for that one target (gate 2, round 3).
        // - non-windows host cross-compiling with mingw -> `resource.lib`
        //   (`non_windows.rs:31`), also COFF: that backend drives `windres` with
        //   `--output-format=coff` (`lib.rs:743`). Linkable by the same GNU `ld`.
        // - windows-msvc host -> `resource.lib` (`windows_msvc.rs:32`), but that one is
        //   `rc.exe /fo` output, a raw `.res` blob. **GNU `ld` has no `.res` reader**, so
        //   handing it over would turn a test that dies at 0xc0000139 into a link error.
        //   Nothing here can save that combination -- `tauri-build`'s own
        //   `rustc-link-arg-bins` feeds the same `.res` to the same linker -- so this warns
        //   instead, and `rustup default stable-msvc` remains the answer (gate 2, round 2).
        //
        // So ask the host for the spelling. `cfg!(windows)` is the right question *here*,
        // unlike the branch above: it is the build machine that decides.
        //
        // `OUT_DIR` as an `OsString` so the `exists()` check is right on any path (the emit
        // below is still lossy -- cargo's build-script protocol is UTF-8 lines)
        let name = if cfg!(windows) {
            "libresource.a"
        } else {
            "resource.lib"
        };
        let resource = std::env::var_os("OUT_DIR")
            .map(|out| std::path::Path::new(&out).join(name))
            .filter(|path| path.exists());
        match resource {
            // **Unscoped on purpose, for now.** `rustc-link-arg` reaches every target of
            // this crate, so a hand-run `cargo tauri build` on a gnu-default machine also
            // ships the duplicated icon/VERSIONINFO and the third manifest described above.
            // `cargo:rustc-link-arg-tests` would narrow it to the harnesses and leave the
            // exe untouched, which is the better shape -- but **the only evidence this
            // workaround works at all is a run on the real Windows machine** (487 tests,
            // Windows 11 26200), CI never builds a gnu target, and nothing here would catch
            // the narrower spelling silently not applying to the lib's unit-test harness.
            // Swap it only after checking it there (gate 2, round 2).
            Some(path) => println!("cargo:rustc-link-arg={}", path.display()),
            // **Say it out loud rather than skip quietly.** These are tauri-build's own
            // filenames, not a documented contract; if they are renamed or stop being
            // written, this branch does nothing and the harness goes back to dying before
            // `main`. Nothing else would catch that — the tests are the only check there is,
            // and CI never builds for a gnu target, so it stays green while the developer in
            // front of it gets an exit code and no explanation. A warning is that
            // explanation. Panicking here is not an option (`unwrap`/`expect` are denied)
            // and would be wrong anyway: a missing workaround should not stop the build.
            //
            // **This only catches the file going missing.** If tauri-build keeps writing
            // `libresource.a` but stops putting the Common-Controls dependency inside it,
            // the link arg still resolves, nothing warns, and the harness is back to
            // 0xc0000139 with no clue. Reading the resource to check would mean parsing it;
            // the exit code is the signal there.
            //
            // **The cause differs by host, and so does the remedy** (gate 2, round 3). A
            // single message naming only the msvc case would send someone cross-compiling
            // from macOS to "use an msvc target", when what they are missing is the mingw
            // resource compiler (`non_windows.rs:41-56` fails to probe
            // `<arch>-w64-mingw32-windres` and writes nothing).
            None if cfg!(windows) => println!(
                "cargo:warning=windows-gnu: tauri-build's compiled resource (libresource.a) \
                 was not found in OUT_DIR, so the Common-Controls manifest is not linked in. \
                 `cargo test -p pictkura --lib` will exit 0xc0000139 before it runs a single \
                 test. On an msvc host this is expected -- its resource is a .res blob that \
                 GNU ld cannot read. Build for an msvc target instead."
            ),
            None => println!(
                "cargo:warning=windows-gnu: tauri-build's compiled resource (resource.lib) \
                 was not found in OUT_DIR, so the Common-Controls manifest is not linked in. \
                 `cargo test -p pictkura --lib` will exit 0xc0000139 before it runs a single \
                 test. Cross-compiling needs the mingw resource compiler \
                 (<arch>-w64-mingw32-windres) on PATH, or RC set to one."
            ),
        }
    }
}
