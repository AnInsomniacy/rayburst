# Release configuration

This checkout builds Rayburst locally. It does not create a store listing, publish a
website, change Git remotes or submit a signing request during development.

## Desktop updates

Keep the existing `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets. The matching public key
is in `src-tauri/tauri.conf.json`; `src-tauri/tauri.release.json` enables signed
updater artifacts for release builds. There is no second key or public-key variable.

Update manifests stay under the existing `updater` release tag:

- Stable: `https://github.com/AnInsomniacy/rayburst/releases/download/updater/latest.json`
- Prerelease: `https://github.com/AnInsomniacy/rayburst/releases/download/updater/beta.json`

Previous releases reach these files through GitHub's repository redirect and verify
updates with the same key. Keep the release tag; replace only the channel JSON after
all referenced packages and signatures exist. Missing assets or signatures stop
publication. The website continues to select the latest stable release only.

The new application identifier uses separate settings, task state and history.
There is no data import or installer migration layer. Native installation may
replace the app in place or leave both products installed, depending on the package
type. Existing filenames and shortcuts can retain the previous name. Recommend a
fresh installation in the README and website migration notices.

## Bundled engine

Release builds use the six platform engine binaries committed under
`src-tauri/binaries`. Update those files from a published aria2-next release before
the desktop release, then review their SHA-256 values. The checkout is the single
source used by local and release builds; CI does not download or replace the
engine during packaging.

## Browser identities

`src-tauri/native-messaging/identity.json` owns the allowed Chromium IDs and Firefox
ID. Native Messaging is activation-only. Chrome and Edge retain their published
store identities; Firefox uses the new Rayburst Connect identity.

When an identity changes, update this allowlist and the extension configuration in
the same delivery. Regenerate packaged manifests with `pnpm build:native-launcher`
and distribute the rebuilt app before the extension. No wildcard origins are allowed.

## Native activation and associations

The browser host activates its paired desktop installation, independently of URL
protocol defaults: Windows uses ShellExecuteEx with the sibling executable, macOS
opens the containing application bundle, and Linux starts the sibling executable.
AppImage startup copies the host to persistent app data and atomically updates an
adjacent `rayburst` symlink to the actual AppImage. Moving an AppImage requires one
manual launch to refresh that link. The host never launches a path supplied by an
extension or searches for other installations.

The desktop owns `.torrent`, `magnet`, `ed2k`, `thunder`, and `rayburst` association
status. The Windows installer registers candidates; runtime queries effective
associations without changing protected UserChoice values. If verification fails, the user can open Settings
with an explicit button; macOS uses LaunchServices and Linux uses GIO. Registration alone is
not proof of the effective default. Development mode does not modify associations
or browser registrations. Native Messaging permissions remain activation-only.

Before release, verify cold activation, existing-window activation, tray-only
activation, file opening, protocol opening, unavailable handlers, another default
application, and uninstall ownership on installed Windows, macOS and Linux builds.
Include AppImage relocation and paths containing spaces. An application launch
response confirms dispatch; the extension still verifies API readiness separately.

## Platform distribution

Windows installers own Default Apps capabilities in their selected install scope.
Runtime actions reuse these registrations instead of adding another application.
Installer maintenance removes owned or orphaned historical candidates; it never
rewrites protected UserChoice values or removes another live installation.

macOS packages use macOS 26 runners with Xcode 26.3 for both architectures.
Keep the build host aligned with the Icon Composer asset runtime; compiling on
macOS 15 can crash AssetCatalogAgent even after the application compiles successfully.

Homebrew publication requires `HOMEBREW_ENABLED=true` and `HOMEBREW_TAP_TOKEN`
with write access to `AnInsomniacy/homebrew-rayburst`. The tap owns `Casks/rayburst.rb`;
the stable release job clones the tap with git and runs `.github/scripts/update-homebrew-cask.sh`
to update the version and both architecture checksums without replacing its installation or
cleanup rules. It does not load the tap through Homebrew, so tap trust policy cannot block it. Windows signing requires `SIGNPATH_ENABLED=true` and the existing
`SIGNPATH_API_TOKEN`, `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG` and
`SIGNPATH_RELEASE_ARTIFACT_CONFIGURATION_SLUG` settings. Match the artifact
configuration to the new installer names in the signing service.

When SignPath is enabled, the release build leaves the update channel unchanged.
Run the existing Windows signing workflow to sign the installers, regenerate their
updater signatures and publish the complete channel JSON. Without SignPath, the
release build publishes the channel JSON after all platform builds finish.
Community package entries are not assumed to exist. Keep installation instructions
limited to packages that have been published.

Use `scripts/bump-version.sh` for a chosen release version. Creating a release,
submitting to stores and platform acceptance are separate actions.

## Website

Cloudflare Pages serves `https://rayburst.pages.dev` from this repository's `main`
branch. Use the `website` root directory, `pnpm build` build command, `dist` output
directory and no framework preset.

Include only `website/*` in build watch paths, leave exclusions empty, and disable
preview branch deployments. Cloudflare bypasses path filtering for pushes with no
changed files, at least 3,000 changed files, or at least 20 commits. The website uses
native Pages caching and `404.html`; it has no separate deployment workflow.
