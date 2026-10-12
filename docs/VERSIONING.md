# Versioning and releases

## The version number: `MAJOR.MINOR.COMMIT`

| Part | Where it comes from | Example |
| --- | --- | --- |
| `MAJOR` | the first number in the [`VERSION`](../VERSION) file, changed by hand | `1` |
| `MINOR` | the second number in `VERSION`, changed by hand | `1.2` |
| `COMMIT` | counted automatically: commits on `main` since `VERSION` last changed (not counting those marked `[skip ci]` or `[skip actions]`, which are never released) | `1.2.17` |

`COMMIT` restarts from `0` whenever `VERSION` changes: after `1.1.23`, a commit that sets
`VERSION` to `1.2` is released as `1.2.0`, and the next one as `1.2.1`.

To start a new version, edit one line and push:

```sh
echo 1.2 > VERSION
git commit -am "Version 1.2" && git push
```

`scripts/version.sh` prints the version of the checked-out commit; the game shows it in the
launcher's side bar, in its log and in `openomsi --version` (baked in by
`crates/omsi-app/build.rs`, which uses the same rule).

## Releases

Every push to `main` runs [`.github/workflows/release.yml`](../.github/workflows/release.yml):

1. works out the version with `scripts/version.sh`;
2. builds Windows x64 and ARM64 (MSVC), macOS for Apple silicon and Intel, Linux x64 and
   ARM64 and Android in parallel, and packs the dedicated server from the Linux and Windows
   builds;
3. creates the release `v<version>` (tag on that commit) with the archives
   `openOMSI-<version>-windows-x64.zip`, `-windows-arm64.zip`, `-macos-arm64.zip`,
   `-macos-x64.zip`, `-linux-x64.zip`, `-linux-arm64.zip`, `-android-arm64.apk`,
   `-server-linux-x64.zip`, `-server-linux-arm64.zip`, `-server-windows-x64.zip`,
   `-server-windows-arm64.zip`, and release notes generated from the commits.

Pull requests run the same builds without publishing anything. Build output never goes
into the repository (`target/` and `dist/` are ignored).

### Test builds of a pull request

A pull request's builds are test builds for anyone who wants to try the change:

* their version names the pull request, `<version>-pr<number>` (e.g. `0.1.1313-pr1192`),
  in the launcher's side bar, `game.log` and crash reports;
* the updater never offers them a release, so a test build stays until it is deleted;
* the Android one is an app of its own, *openOMSI PR #N* (`org.openomsi.game.pr`), that
  installs beside the release and shares its `openOMSI` folder;
* [`.github/workflows/pr_builds.yml`](../.github/workflows/pr_builds.yml) comments on the
  pull request with a download link per platform (through [nightly.link](https://nightly.link),
  no GitHub account needed) once they are built, and updates that comment on every push.
  The files are kept for 14 days.

The version badge at the top of the README always shows the newest release.

### A broken release

The start guard (`crates/omsi-guard`, see docs/ARCHITECTURE.md) heals a release that crashes
or hangs at the start on every player's computer by itself: a newer release is installed at
once, or the version before is put back. For a release that starts but is broken otherwise
(saves, physics, a crash a minute in), list it in [`release/health.json`](../release/health.json)
on `main`:

```json
{ "bad": ["0.2.31"], "message": "0.2.31 loses the timetable after a stop" }
```

Every start of a listed version notes it; the next start installs a newer release when there
is one, else goes back to the version before (when its files are still kept, i.e. the listed
version has not been running long), and tells the player why with the message. Then push the
fixed release as usual. The list is read from GitHub with a short wait at each start; offline,
nothing happens. Take a version off the list once nobody runs it any more.

Before any update replaces a program, the new one must pass `openomsi --self-test` (it starts,
says its version and ends): a release that cannot even be loaded on a computer is refused there
and never offered again.


## The website

[`.github/workflows/pages.yml`](../.github/workflows/pages.yml) publishes `site/` together with
the Markdown files of `docs/` to GitHub Pages
(https://openomsi.org/) whenever they change on `main`. The site renders
the Markdown in the browser, so a documentation change is one edit in `docs/`.
