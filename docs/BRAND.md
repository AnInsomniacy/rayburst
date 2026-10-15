# Rayburst brand

Product: **Rayburst**. Package: `rayburst`. Application ID: `dev.aninsomniacy.rayburst`.

Redefining the open-source download manager

`public/logo.svg` is the artwork source. Run `pnpm icons` to generate desktop icons
with the Tauri CLI. Windows ICO and Linux PNG icons retain transparent backgrounds.
The same command synchronizes the master into `src-tauri/icons/Rayburst.icon`.
macOS compiles this Icon Composer asset with Apple's `actool`, with mist
lavender (`#E9E4F2`) and ink violet (`#211A30`) backgrounds for default and dark
appearances. Apple renders the enclosure and appearance variants; the logo stays
opaque and undistorted. Edit appearance settings in Icon Composer. Packaging
requires Xcode 26 or later. The macOS `beforeBundleCommand` compiles the source
into `generated-icons/Assets.car`; Tauri bundles that catalog directly. The hook
gives `actool` an explicit standard input to avoid the Node CLI descriptor issue
tracked in [tauri#15991](https://github.com/tauri-apps/tauri/pull/15991).
The tray remains a separate transparent template image. The Windows tray uses
`src-tauri/icons/tray/`, one PNG per display scale, so the shell never resizes it.
`docs/brand/banner.png` is the English README banner with the current slogan. Use no terminal punctuation in slogans, including translations.

The default interface seed is `#9E74D5`, a soft lavender. Material Color Utilities
generates both themes through the content palette, preserving the seed chroma.
The original logo artwork retains its purple gradients.
The empty-list background reuses `public/logo.svg` as a monochrome CSS mask,
without a wordmark. It follows the rendered list immediately, independent of
engine startup or database readiness.
CSS, Naive UI and canvas drawing use semantic roles. Warning, error and success
colors describe state; they are not aliases for the brand color. Button foregrounds
must remain readable in normal, hover, focus and pressed states.

Use the product name without translation. Keep interface labels short and literal.
Use the slogan in the README, website and About panel. Review prose with Sepia;
remove unsupported claims and unnecessary adjectives.

The engine remains aria2-next. Its code, protocols and binary contents are independent
of this branding change. The desktop bundle uses the engine's actual executable name.

Interface slogans use the existing i18n dictionaries in all 27 supported locales.
The approved Simplified Chinese slogan is “重新定义开源下载器”
Use its Traditional Chinese equivalent for zh-TW. Preserve the approved English
slogan in English interfaces, README banners and promotional artwork.

The website is a standalone Vue project in `website/`. Its light and dark
colors use the desktop's default palette; the SVG logos retain their original colors.
Website artwork copies come from `public/logo.svg`, Rayburst Connect's
`public/icon/icon.svg`, and the screenshots in `docs/media/`.
The companion section uses the approved browser-to-desktop campaign artwork.
Its engine panel uses Aria2 Next's approved black-and-gold `docs/media/banner.png`.
Keep promotional scenes separate from the unedited interface screenshots; preserve
their proportions and omit release numbers from campaign artwork.
Serve full-resolution WebP copies on the website: lossless for interface screenshots
and quality 90 for campaign artwork. Keep the original PNGs in their source locations.
Use the current repository URLs throughout the app, website and documentation.
Keep the previous product name in the README and website migration notices only.
The website continues to offer the latest stable release, even before the first
release under the new brand. Localize website copy in all 27 languages.

Product names belong in visible copy and distributable filenames. Internal symbols
use their responsibility, and published protocol and storage identities stay fixed.
Rebranding does not rotate signing keys, rename update channels or change engine APIs.

On macOS, `pnpm icons` also exports `macos-dev.icns` through Apple’s `ictool`
for the development Dock icon. Install Icon Composer first, or set
`ICON_COMPOSER_TOOL` to its executable path. Restart `pnpm tauri dev` after
regenerating icons. This preview uses the default appearance; packaged apps use
the Icon Composer appearance variants.
