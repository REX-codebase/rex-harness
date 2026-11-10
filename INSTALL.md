# Installing REX Harness (desktop app)

Download installers from the **Releases** page of
[REX-codebase/rex-harness](https://github.com/REX-codebase/rex-harness).
Releases are cut from tags like `v0.1.2` — the tag must match the `version`
in `src-tauri/tauri.conf.json`.

## Windows

- Download the `…x64-setup.exe` from the latest release and run it. It is a
  setup wizard: a welcome page, the licence (MIT OR Apache-2.0), who to install
  for (just you, which needs no administrator rights, or everyone on the PC),
  the install folder, the Start menu folder, and a "Run REX Harness" box on the
  last page.
- Each release also carries a `.sha256` file. To check the download:
  `Get-FileHash -Algorithm SHA256 .\<installer>.exe` should print the same hash.
- The installer is **unsigned**, so Windows SmartScreen may warn on first run
  ("Windows protected your PC"). Click **More info → Run anyway**. This is
  expected until the project has a code-signing certificate.
- The setup installs Microsoft's WebView2 runtime if your PC does not have it
  (Windows 11 and current Windows 10 already do), which needs an internet
  connection for that one step.
- To remove it: Settings → Apps → REX Harness → Uninstall. Your runs, keys and
  config live outside the app folder and are kept unless you delete them.

## macOS (Apple Silicon, arm64)

- Download `REX-Harness_0.1.2_aarch64.dmg`, open it, drag **REX Harness**
  into **Applications**.
- The app is **unsigned and not notarized**, so Gatekeeper will block the
  first launch ("Apple could not verify…"). Workaround: in Finder,
  **right-click (Ctrl-click) REX Harness → Open → Open**. You only need to do
  this once; afterwards it launches normally.
- Intel Macs: no Intel build is produced yet (CI builds on `macos-14`,
  arm64 only). It may run under Rosetta 2 — untested.

## Linux

- **AppImage**: download `REX Harness_0.1.2_amd64.AppImage`,
  `chmod +x` it, and run it. Works on most distros, no install needed.
- **deb** (Debian/Ubuntu): download `REX Harness_0.1.2_amd64.deb` and
  `sudo apt install ./"REX Harness_0.1.2_amd64.deb"` (pulls dependencies).
- System dependencies (already declared by the deb, needed for the AppImage
  on minimal systems): `libwebkit2gtk-4.1-0`, `libayatana-appindicator3-1`.

## Command line only (`rex`)

The desktop installers are for the app. The `rex` command (scriptable runs for CI,
`rex verify`, `rex runs`) has its own prebuilt archives on every release, so you do
not need Rust to get it.

- **Linux x86_64 and macOS Apple Silicon:**

  ```sh
  curl -fsSL https://raw.githubusercontent.com/REX-codebase/rex-harness/main/scripts/install.sh | sh
  ```

  It downloads `rex-<platform>.tar.gz` and the `.sha256` beside it, refuses to install
  if they do not match, and puts `rex` in `~/.local/bin` (`REX_INSTALL_DIR` changes
  that; `REX_VERSION=v0.1.2` pins a release). It prints a line to add to your `PATH`
  if that folder is not on it. To read the script first, open
  [scripts/install.sh](scripts/install.sh).
- **Windows:** download `rex-x86_64-pc-windows-msvc.zip` from the release, check it
  against the `.sha256` file, and put `rex.exe` somewhere on your `PATH`.
- **Any other platform, or to build it yourself:** `cargo install --path crates/rex-cli --locked`,
  or `./quickstart.sh --no-app`, which also installs the frontend dependencies.

## From source

```sh
git clone https://github.com/REX-codebase/rex-harness
cd rex-harness
./quickstart.sh            # dependencies, the rex CLI, then the app in dev mode
./quickstart.sh --build    # ... then a production build instead
./quickstart.sh --no-app   # dependencies and the rex CLI only
```

It checks Node.js 20+, Rust and, on Linux, the WebKitGTK libraries (it prints the
`apt` line if any are missing). If `~/.cargo/bin` is not on your `PATH` it tells you
what to add instead of failing.

## Notes

- **No auto-update yet.** The updater plugin is disabled in the build
  because there are no signing keys. To get a new version, download the new
  installer from Releases and install over the old one.
- Your data (saved keys, your ChatGPT sign-in, the runs ledger, signing key,
  config) lives outside the app, in `%LOCALAPPDATA%\rex-harness` on Windows. On
  macOS and Linux the desktop app uses `~/.config/rex-harness` and the `rex`
  command uses `~/.rex/harness` (they keep the folders earlier versions used; set
  `REX_STATE_DIR` to the same folder for both to share one sign-in). It survives
  reinstalls and uninstalls. Everything stays on your computer. See
  [docs/data-and-storage.md](docs/data-and-storage.md) for what is in it, how it is
  protected, and how to erase it.
- **Upgrading from v0.1.0 on Windows:** that version kept its data in
  `%TEMP%\rex-harness`. Newer builds use `%LOCALAPPDATA%\rex-harness`; connect again
  or copy `secrets.json` across.
- **Upgrading from v0.1.1 on Windows:** tasks opened with *Delegate to external host*
  were kept in a `.rex\harness` folder next to wherever the app was started. They are
  now kept in `%LOCALAPPDATA%\rex-harness\tasks`; copy the old `tasks` folder there
  if you want to keep them.

## For maintainers: cutting a release

- **Windows installer only (publishes it):** Actions → *Release* → *Run workflow*,
  tag `v` + the app version (for example `v0.1.2`; the first job refuses a tag that
  does not match `src-tauri/tauri.conf.json`). Tick *all_platforms* to also build
  macOS and Linux, or *draft* to review before publishing. The release gets the
  setup `.exe` and a `.sha256` next to it.
- **Every platform, as a draft:** push the tag (`git tag v0.1.2 && git push origin v0.1.2`).
- **The setup wizard** is configured in `src-tauri/tauri.conf.json`
  (`bundle.windows.nsis`) with its artwork and licence page in
  `src-tauri/installer/`. To change the artwork, edit `scripts/installer-art/*.html`
  and run `node scripts/installer-art/build.mjs` (needs Chromium and Pillow); the
  generated bitmaps are committed. `cargo test -p rex-cli --test installer` checks the
  bitmaps, the licence page and the version are consistent.
- The build is **unsigned**. A code-signing certificate would remove the SmartScreen
  warning; add it as a repository secret and set `bundle.windows.certificateThumbprint`
  (or use Azure Trusted Signing) when one exists.
