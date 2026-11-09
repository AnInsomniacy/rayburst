# Rayburst website

The site for Rayburst, built with Vue 3, TypeScript and Vite. It is an independent
pnpm project; the desktop app's dependencies are not involved.

| Concern | Library |
| --- | --- |
| Components | Vue 3 (`<script setup lang="ts">`) |
| Languages | vue-i18n, messages precompiled by `@intlify/unplugin-vue-i18n` |
| Motion | GSAP (timelines, SplitText, DrawSVG, Flip, MotionPath, CustomEase) |
| Menus, tabs, radios, dialog | Reka UI (headless; styled here) |
| Browser utilities | VueUse |
| Icons | `@vicons/ionicons5`, the set Rayburst uses |
| Color schemes | `@material/material-color-utilities`, as in the app |
| Fonts | Fontsource (Mona Sans for names and headlines, Inter, JetBrains Mono) |

## Develop

```bash
pnpm install
pnpm dev          # http://localhost:5173
pnpm build        # type check + production build into dist/
pnpm preview      # serve dist/
```

## Layout

```
index.html, 404.html     entries (theme is set before first paint)
src/
  main.ts, App.vue       the page; not-found.ts, NotFound.vue for 404
  i18n.ts                locale detection, lazy loading, right-to-left
  locales/*.json         27 languages
  motion/                GSAP setup, the scene clock, canvas sizing, directives
  art/                   the promo artwork's ripples, aria and palettes, drawn live
  sim/                   the demo tasks as pure functions of time
  composables/           theme, language, GitHub releases and statistics
  components/            one folder per section; mock/ rebuilds the app window;
                         art/ holds the mark's ring and the section backdrops
  styles/                tokens, base, shared controls, the window mock
  data/schemes.ts        the ten color schemes, generated like colorScheme.ts
tools/                   maintenance scripts
```

## Motion

- A scene is a paused GSAP timeline used as a clock (`motion/useScene.ts`). It
  starts when it comes into view, runs only while visible, and everything it
  shows is computed from its time, so scrolling speed never affects it.
- State changes inside a scene transition with CSS; one-shot choreography (the
  hero intro, reveals, counters, size changes) uses GSAP.
- With reduced motion, every scene shows a representative still frame.

All interface mock-ups are rebuilt from the apps' components and show the apps'
own labels in every language. Speeds and sizes are illustrative.

## Maintenance

| Task | Command |
| --- | --- |
| Lint, format, keys, placeholders, local assets and size budget | `pnpm lint`, `pnpm format:check`, `pnpm check` |
| Refresh interface strings from the apps' locales | `pnpm strings` (`python3 tools/sync-ui-strings.py --check` to verify) |

`sync-ui-strings.py` finds the Rayburst and Rayburst Connect repositories
through `$RAYBURST_LAB`, `../rayburst-lab`, or `../..` when the site lives in
`rayburst/website`.

New copy must exist in all 27 locale files; `pnpm check` fails otherwise. Edit
locales with a batch script, never one file at a time. Headlines and slogans
carry no terminal punctuation. The three slogans are the repositories' GitHub
descriptions and stay word for word:

- Rayburst: Redefining the open-source download manager
- Rayburst Connect: Redefining the companion browser extension
- Aria2 Next: Redefining the next generation of aria2

## The film

The English film opens in a dialog using YouTube's privacy-enhanced player. The
cover is local; no YouTube resources load until the visitor opens it, and
closing the dialog removes the player. The player preserves the HTTP Referer
with `strict-origin-when-cross-origin`; do not suppress it in deployment headers.

## Deployment

Cloudflare Pages builds this folder: root directory `website`, build command
`pnpm build`, output directory `dist`. The page loads local assets and requests
GitHub release metadata and statistics. Opening the film connects to YouTube,
as described in `docs/PRIVACY.md`.
