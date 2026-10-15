# Theme preview

Renders the real lens, panel and settings windows in a normal browser, with a fake Rust core,
so themes can be built and checked on any OS (DESIGN §10.8). Development only: nothing here
ships in the app.

## Interactive

```sh
pnpm dev
# open http://localhost:1420/dev/preview/
```

- **Theme / Speed** switch `settings.theme` / `settings.animSpeed` live (`og://settings`), like
  the Appearance tab.
- **Scenario** + **Play** replay one situation on a 1600×900 fake desktop:
  `boot` (Lens-On with screenshot frames), `boot-bare` (no frames), `lockon`, `caption`, `card`,
  `rail` (a streamed sample answer), `scan` (anchors), `freeze` (SELECT: drag a rectangle,
  right-click cancels), `toast`, `settings` (the settings window).
- Moving the mouse over the desktop sends `og://cursor`, so hover-to-expand works. Follow-ups,
  pin, copy and close in the panel work too.
- URL parameters: `?theme=<id>&speed=fast|normal|slow&scenario=<name>&autoplay=0`.

Themes are read from the repo's `themes/` folder. Saving `theme.css` (or an asset it uses)
updates the open windows in place; replay `boot` to see a changed boot animation. A new theme
folder or a changed `theme.json` reloads the page.

## Screenshots

```sh
node dev/preview/shoot.mjs --theme terminal --scenario boot --times 0,200,400,600,800,1000,1200 --out /tmp/x
node dev/preview/shoot.mjs --theme holo --speed slow --scenario all
```

- `--scenario boot|boot-bare` poses the boot animation at each of `--times` (ms since the stage
  mounted, at the chosen speed; default: 9 steps over `bootMs × speed`) by pausing and seeking
  every CSS animation in the lens.
- `--scenario all` shoots lockon, caption, card, rail, scan, freeze (+ capture, the answer to
  the dragged selection), settings and toast, each after its answer finished streaming. Any
  single scenario name works too.
- PNGs go to `--out` (default `dev/preview/out/`, git-ignored). The script starts its own Vite
  server on `--port` (1421).
- Browser: `$CHROME`, else the newest `~/.cache/ms-playwright/chromium-*/chrome-linux*/chrome`,
  else an installed Google Chrome (`pnpm exec playwright-core install chromium` downloads one).

## Files

| File | Role |
|---|---|
| `index.html`, `harness.ts` | Control bar + monitor; the fake Rust core (commands, events, panel placement, scenarios) |
| `lens.html`, `panel.html`, `settings.html` | Copies of the real entry pages that load `mock.ts` first |
| `mock.ts` | Fake `window.__TAURI_INTERNALS__`: forwards `invoke` to the harness, delivers its events |
| `data.ts` | Settings, HUD, sample answers, scan items, anchor boxes on the desktop |
| `assets/desktop.html` → `desktop.png` | The fake desktop; regenerate with `shoot.mjs --scenario desktop` |

Type-check the harness with `pnpm exec tsc -p dev/preview` (the app's `pnpm typecheck` does not
include `dev/`). A command added to `CommandMap` in `src/shared/ipc.ts` fails that check until
`harness.ts` answers it.
