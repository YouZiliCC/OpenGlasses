# Theme packs

OpenGlasses themes are folders of **CSS and static assets**, loaded at runtime. Built-in and
community themes use exactly the same format; the built-in ones in [`themes/`](../themes) are the
reference examples (start from `themes/terminal/theme.css`). Design background: `docs/DESIGN.md` §10.

## How it works

- **Where:** built-in themes ship with the app. User themes go in
  `%APPDATA%\io.github.youzilicc.openglasses\themes\<id>\`
  (Settings → Appearance → **Open themes folder** creates and opens it). A user theme with the
  same id as a built-in one replaces it.
- **Runtime:** themes are discovered while the app runs. Switching themes needs no restart, and
  edits to any theme file are picked up within about 1 s (hot reload). **Reload themes** forces a rescan.
- **Loading:** every window (lens overlays, answer panel, settings) loads the app's `base.css`,
  then your `theme.css` as `<link id="og-theme">`, the last element in `<head>`, so at equal
  specificity your rules win. The root element gets `data-theme="<id>"`.
- **Choosing:** Settings → Appearance → Theme previews the selection in the settings window at
  once; **Save** applies it everywhere. If the saved id is missing, the app falls back to
  `terminal`, then to the first theme found, then to `base.css` alone (still usable).

## Pack layout and manifest

```
<id>/                 folder name = id, must match ^[a-z0-9][a-z0-9-]{0,31}$
├── theme.json        manifest (required)
├── theme.css         loaded in the lens, panel and settings windows
└── assets            .svg .png .jpg .jpeg .webp .gif .woff2 .woff .ttf .otf, referenced by relative url()
```

```json
{ "name": "Holo Blue", "author": "OpenGlasses", "version": "1.0.0",
  "description": "Ice-blue holographic HUD. Boot: putting on glasses.", "bootMs": 1600 }
```

| Field | Meaning |
|---|---|
| `name`, `author`, `version`, `description` | Shown in Settings → Appearance |
| `bootMs` | Length of your boot animation at normal speed. The boot stage is removed after `bootMs × speed`. Missing or invalid → 1000; clamped to 300–4000 |

A folder whose `theme.json` does not parse is skipped (the reason goes to the log).

## Security model

The overlay windows can call the AI, take screenshots and read the selection, so themes get
**no code and no network**:

- Only `.css`, `.json` and the asset extensions listed above are served, at most 8 MB each.
  There is no JavaScript: `.js` and HTML files are never served, and nothing runs a script.
- Files are served by the local `ogtheme` protocol (`http://ogtheme.localhost/<id>/<path>`),
  only from inside that theme's own folder (`..`, backslashes and absolute paths give 404).
- The Content Security Policy allows styles, images and fonts from `ogtheme` only, and
  `connect-src` is unchanged. Remote `url()`s and `@import`s from the internet are blocked, so
  CSS cannot send anything anywhere.

## Tokens

Override these on `:root`. `base.css` defines all of them with the Terminal Green values.
`--og-accent-faint`, `--og-accent-2`, `--og-text`, `--og-glow`, `--og-font-display`,
`--og-panel-border` and `--og-scanlines` default to values derived from `--og-accent` /
`--og-font`, so a quick recolor can set just `--og-accent`.

| Token | Default | Use |
|---|---|---|
| `--og-accent` | `#39ff7a` | Main color: highlights, lines, brackets, carets, primary buttons |
| `--og-accent-dim` | `#1f9c4a` | Secondary text, hints, inactive controls |
| `--og-accent-faint` | accent at 18% | Hairlines, separators, frames |
| `--og-accent-2` | `= --og-accent` | Second accent (scan anchor dots; Terminal uses `#3dffc8`) |
| `--og-text` | `= --og-accent` | Body text |
| `--og-warn` | `#ffcc33` | Warnings, errors, toasts |
| `--og-glow` / `--og-glow-warn` | `0 0 6px` accent / warn at 55% | Text glow, a `text-shadow` value. It is also appended to other shadows (`0 0 2px #000, var(--og-glow)`), so for "no glow" use `0 0 0 transparent`, not `none` |
| `--og-font` / `--og-font-display` | `"Cascadia Mono", "Consolas", "Microsoft YaHei UI", monospace` / `= --og-font` | Body text / titles, HUD, title bar, tabs |
| `--og-glass-tint` | `rgba(2,12,5,.62)` | Tint over the snapshot glass (the blurred screenshot behind callouts and panels) |
| `--og-panel-bg` | `rgba(3,14,7,.9)` | Panel/callout background when there is no snapshot; toast and size-label background |
| `--og-panel-border` | `1px solid var(--og-accent-faint)` | Panel and settings-window frame (a `border` shorthand) |
| `--og-radius` | `0px` | Corner radius of panels, glass, toasts, code blocks |
| `--og-scanlines` | 1px/3px accent lines at 6% | Texture over glass and panels (a `background-image` value; may be `none`) |
| `--og-settings-bg` | `#030e07` | Opaque background of the settings window |
| `--og-speed` | `0.6` / `1` / `1.6` | **Read-only**, set by the app from Settings → Animation speed (Fast / Normal / Slow) |

### The `--og-speed` rule

Write **every** duration and delay as `calc(<ms> * var(--og-speed))`, including the boot
stage. The only exceptions are timings the app enforces with timers, which are fixed:
`.toast` (1.8 s), the 150 ms fade of `.session.closing` / `.panel.closing`, and the
streaming caret blink. Don't make those longer; the element is removed on time anyway.

## Boot stage

Shown on the primary monitor's lens at startup and every time the glasses are put on (if
"Lens-On" is enabled). The app mounts this fixed DOM:

```html
<div class="og-boot [og-boot-noframes]"
     style="--og-frame-0: url(…1/32…); --og-frame-1: url(…1/12…); --og-frame-2: url(…1/4…)">
  <div class="og-boot-frame og-boot-f0"></div>   <!-- most blurred -->
  <div class="og-boot-frame og-boot-f1"></div>
  <div class="og-boot-frame og-boot-f2"></div>   <!-- sharpest -->
  <div class="og-boot-fx og-boot-fx1"></div>     <!-- three empty full-screen layers -->
  <div class="og-boot-fx og-boot-fx2"></div>
  <div class="og-boot-fx og-boot-fx3"></div>
</div>
```

- **Frames** are screenshots of the screen at 1/32, 1/12 and 1/4 size. Stretched to full screen
  they are the blur, so you need no `filter: blur()`. By default each `.og-boot-frame` is a
  full-screen `background-image: var(--og-frame-N)` with `background-size: 100% 100%` and
  `opacity: 0`; the default animation fades **f0 from 1 to 0 over `1000ms × speed`**.
  Animate opacity rather than `display`, so all three frames are loaded at mount.
- **No screenshot** (capture failed or was too slow): the root also has `og-boot-noframes`
  and the three variables are `none`. Give this variant its own look (Terminal: vignette plus
  one sweeping scan line).
- **fx1–fx3** are yours: draw with backgrounds, masks, borders, transforms and
  `::before`/`::after`. Stacking is DOM order (frames under fx, sharper frames above blurrier).
- **Lifetime:** the stage is unmounted `bootMs × speed` ms (plus a small margin) after it
  mounts. **By then it must be completely transparent**, with every visible layer at
  `opacity: 0`, so the real desktop shows with no pop. Nothing may keep animating afterwards.
- While the stage is mounted, the lens root is `.lens.og-booting`.
- **HUD corners** are remounted on every boot with `.og-hud-corner.lit` and `--i` = 0, 1, 2, 3
  for top-left, top-right, bottom-left, bottom-right; stagger with
  `animation-delay: calc((<start> + var(--i) * <step>) * var(--og-speed))`. Their numbers roll
  up from 0 over the stage length.

### Restyle hooks (`@keyframes`)

If two `@keyframes` rules share a name, the last one loaded wins everywhere. Your theme loads
last, so redefining these restyles the effect wherever it is used:

| Keyframes | Used by | Base |
|---|---|---|
| `og-crt-on` | appear effect of `.callout`, `.caption`, `.panel.crt-on` | fade in (Terminal: two CRT flickers) |
| `og-hud-on` | `.og-hud-corner.lit` light-up | fade in from 0. Leave out the 100% keyframe, so it ends on the corner's own opacity (0.8, or 0.15 near the cursor) |

Name all your other keyframes with your theme id (`og-holo-…`, `og-term-…`), so they never
replace a base animation by accident.

## Stable class names

These class names (and the `og-*` ones) are public API: renaming one is a breaking change.

**All windows**

| Class | What |
|---|---|
| `.glow` / `.warn` / `.dim` / `.small` | Glowing accent text / warning text / secondary text / small text |
| `.caret` | Streaming cursor `▌` (only while streaming) |
| `.md` | Rendered Markdown answer (KaTeX math inside as `.katex`) |
| `.crt-on` | Plays the `og-crt-on` appear effect once |

**Lens** (full-screen, click-through overlay per monitor; it never receives hover, except in SELECT)

| Class | What |
|---|---|
| `.lens` / `.og-booting` | Overlay root / while the boot stage is mounted |
| `.og-boot`, `.og-boot-noframes`, `.og-boot-frame`, `.og-boot-f0`–`f2`, `.og-boot-fx`, `.og-boot-fx1`–`fx3` | Boot stage (above) |
| `.session` / `.closing` | Wrapper of the current answer / its 150 ms dismiss fade |
| `.svg-layer` | Full-screen SVG holding brackets and leader lines |
| `.brackets` (`.warn` on error) | Lock-on corner brackets around the selection (SVG paths, colored by `color`) |
| `.leader`, `.line`, `.marker` | Leader line to the callout or rail (drawn via `stroke-dashoffset`, delay in `--delay`) and its ◈ end marker |
| `.callout`, `.tldr`, `.clamp3` (`.streaming`) | Lock-on callout, its TL;DR line, its 3-line body |
| `.glass` / `.glass.fallback` | Feathered snapshot-glass background behind callout and caption / variant without a snapshot |
| `.caption-band`, `.caption`, `.caption-text` | Caption style: bottom band, subtitle box, its 2 lines |
| `.anchor` | Scan anchor hit box; the dot is its `::after` |
| `.toast` | One-line feedback near the cursor (also `.warn`) |
| `.hud`, `.hud-tl` / `-tr` / `-bl` / `-br`, `.og-hud-corner`, `.lit`, `.near` | HUD corner, its position, boot light-up, faded near the cursor |
| `.breathe` | THINKING label in the HUD (the only looping HUD animation) |
| `.freeze`, `.freeze-dim`, `.sel`, `.sel-size` (`.inside`) | SELECT: frozen frame, dimming, drag rectangle, its size label |

**Panel** (the answer card, expanded callout, scan list and Rail; the whole window is `.panel`)

| Class | What |
|---|---|
| `.panel` (`.closing`, `.crt-on`) | Panel root, glass background set inline |
| `.ph`, `.glyph`, `.title` | Header row, kind glyph, headline |
| `.tb` (`.copy`, `.pin`, `.pin.on`, `.warn`) | Small header/inline buttons |
| `.pb`, `.pb-inner` | Scrolling body and its padding box |
| `.src` (`.open`) | Source quote (click to expand) |
| `.ans`, `.live` | An answer / the one currently streaming |
| `.q` | A follow-up question |
| `.err`, `.err-msg` | Error line and its detail |
| `.scan`, `.scan-head`, `.scan-detail` | Scan result list, item row, expanded detail |
| `.pf`, `.composer`, `.prompt` | Footer, follow-up input row, its `›` prompt |
| `.hist`, `.hist-head`, `.hist-list`, `.hist-item`, `.hist-tldr`, `.hist-text` | Rail history |

**Settings**

| Class | What |
|---|---|
| `.og-titlebar`, `.og-titlebar-title`, `.og-titlebar-btn`, `.og-titlebar-close` | Custom title bar (≈36 px; drag region), its title, minimize/close buttons, the close button |
| `.set`, `.set-loading` | Window frame (`--og-panel-border`) / loading or error message |
| `.set-tabs`, `.set-brand`, `.tab` (`.on`) | Tab row, ◉ OPENGLASSES brand, tab buttons |
| `.set-main`, `.set-bar`, `.set-status` | Scrolling content, bottom bar with Save/Revert, its status text |
| `.sec`, `.field`, `.lbl`, `.hint`, `.note` | Section (title is its `h3`), label/control row, label, help line, intro note |
| `.inline`, `.grid2`, `.opt` | Row of controls, two-column grid, checkbox/radio with its label |
| `.btn` (`.primary`) | Buttons |

Plain `input`, `select`, `option` and `textarea` are styled by element.

## Tips

- **Assets:** `url(lens.svg)` resolves next to your `theme.css`. `mask-image` and `@font-face`
  work, because the protocol sends CORS headers.
- **Fonts:** Windows 11 fonts (Bahnschrift, Segoe UI Variable, Cascadia Mono, …) cost
  nothing. Bundle others as `.woff2` only if their license allows it.
- **Loops:** avoid infinite animations, except while an answer streams or the model thinks
  (`.caret`, `.live`, `.streaming`, `.breathe`). The overlay is always on screen.
- **Performance:** animate `opacity` and `transform`/`scale`. A full-screen `filter` costs a lot
  on a 4K screen, and the frames already supply the blur.
- **Legibility:** answers appear over any page, white ones included. Test the glow
  and `--og-glass-tint` there, as well as on dark pages and with the "Acrylic" glass setting.
- **Panel height:** the app sizes the panel from `.ph` + `.pb-inner` + `.pf` plus `.panel`'s
  vertical border and padding. Margins on those three are not counted, so use padding.
- Don't hide stable elements with `display: none`. Everything is click-through, so `:hover`
  does nothing in the lens.

## Preview without Windows

`dev/preview/` renders the lens, panel and settings windows in a normal browser with mocked
IPC. It can replay the boot animation, sample answers and the HUD, so themes can be built on
macOS or Linux too:

```sh
pnpm install
pnpm dev            # then open http://localhost:1420/dev/preview/
```

Develop your pack in `themes/<id>/` of a checkout. It is only for development and is not
part of the installer. On Windows you can also just run the app: edit the files in the user
themes folder and they apply within about a second.

## PR checklist (built-in themes)

- [ ] `themes/<id>/` with a valid id, a complete `theme.json`, and `bootMs` equal to the real stage length.
- [ ] CSS and whitelisted assets only: no remote URLs, no web `@import`, small files.
- [ ] Every duration and delay is `calc(… * var(--og-speed))`. Checked at Fast, Normal and Slow.
- [ ] The boot stage is fully transparent at `bootMs × speed` in both variants (with frames, `og-boot-noframes`).
- [ ] No animation keeps running after the stage unmounts. No infinite animation outside streaming/THINKING.
- [ ] Uses only the tokens and the stable class names above. Own keyframes are prefixed with the theme id.
- [ ] Readable over white and dark pages, on snapshot glass and without it, and in the settings
      window (inputs, `select` options, disabled buttons, title-bar hover).
- [ ] Fonts are system fonts, or licensed for redistribution.
- [ ] Screenshots or a GIF of the boot, a Lock-on answer and the panel in the PR description.
