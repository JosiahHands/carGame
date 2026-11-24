# Game packs

Packs are optional add-ons that live in this folder, one folder each. To install
them, see the [Packs section of the main README](../README.md#packs). This file
is for anyone writing a pack.

| Pack | Folder | What it does |
| --- | --- | --- |
| 🌅 Photo Real | `graphics/` | Sky and sunlight, image-based lighting, wet roads, sculpted car bodies, post-processing |
| 🧪 Real Physics | `physics/` | Tyre model, weight transfer, engine and gears, crash spins and dents, telemetry |
| 🧠 Living City | `npc/` | Twelve city people with memories, feelings and personalities, written by Claude |

## How packs get into the game

- `packs/installed.json` lists the installed packs. It's in each player's copy
  of the game, not in Git. `node scripts/packs.mjs add|remove|list|check` keeps
  it up to date. `install.sh --pack NAME` (or `--pack all`) calls it.
- The build reads that list (`vite.config.mts`) and bundles only those packs into
  the game through `import packs from 'virtual:installed-packs'`. Packs that
  aren't installed add nothing to the game.
- `scripts/setup.sh` runs the build. The install and update commands run setup,
  so **updating the game updates its packs**. A pack that has been taken out of
  the repo is dropped from the list.
- To build with packs that aren't installed, set `CAR_GAME_PACKS`. Use `all`,
  or a list like `graphics,npc`. The checks on GitHub build both with no packs
  and with all of them.
- The Packs tab in the garage switches packs on and off, shows their options,
  and removes them. Removing a pack takes it out of `installed.json`, and the
  next update builds the game without it.

## Writing a pack

Make a folder, `packs/<name>/`, with an `index.ts` whose default export is a
`Pack` (see `game/packs.ts`):

```ts
import type { GameApi, Pack, PackInstance } from '../../game/packs';

const pack: Pack = {
  id: 'example', // the folder name
  name: 'Example',
  icon: '✨',
  about: 'One or two sentences for the Packs tab.',
  activate(game: GameApi): PackInstance {
    // Set things up here. `game` has the renderer, scene, camera, world,
    // traffic, pedestrians, the player's car, settings, and more.
    return {
      frame(dt) {
        /* every frame */
      },
      dispose() {
        /* undo everything activate() did */
      },
    };
  },
};
export default pack;
```

The hooks a pack can use, all optional except `dispose`:

| Hook | When |
| --- | --- |
| `mapLoaded()` | A map or track was just built |
| `frame(dt, realDt)` | Every frame, before drawing. `dt` is 0 while paused |
| `render()` | Draws the frame itself (return `true`). The graphics pack uses it for post-processing |
| `resize(w, h)` | The window changed size |
| `nightChanged(night)` | Day or night changed |
| `carBuilt(rig)` | A car model was built: the player's, a rival's, traffic or parked. `rig.config` holds the garage settings |
| `settings(panel)` | Adds options to the pack's section of the Packs tab |
| `dispose()` | The pack was switched off or removed. Put everything back |

`game.on('crash' | 'destroyed' | 'jump' | 'nearMiss' | 'explosion' | 'horn' | 'drift', listener)`
reports what the player does. `game.onKey('KeyK', handler)` adds a key while
driving. `game.load` and `game.save` keep the pack's data between sessions.
`game.overlay` is a layer over the game for labels and panels. The host removes
listeners and keys when the pack stops.

Rules that keep packs safe to switch on and off:

- **Undo everything in `dispose`.** Put back changed materials, lights and
  settings, and remove anything added to the scene or page. The game keeps
  running without a restart.
- **Don't throw.** The host catches errors. A pack that keeps failing is
  switched off, but a broken frame still looks bad.
- **Keep it optional.** The game never imports from `packs/`. Packs import from
  `game/` and `libs/`.
- If the pack has options, save them with `game.save('options', ...)` and
  show them in `settings(panel)`.

### A main-process part (optional)

A pack that needs Node or Electron (files, secrets, network calls that must not
come from the page) can add `packs/<name>/main.ts` exporting
`activate(host: PackMainHost)`. `scripts/build-electron.mjs` bundles it into
`dist-electron/packs/<name>.js`. It gets its own data folder (`host.dataDir`)
and offers functions with `host.handle(name, fn)`. The page calls them with
`game.callMain(name, ...args)`. Only the game's own page can call them.

The Living City pack works this way. The API key is encrypted with Electron's
`safeStorage` in the pack's data folder and never reaches the page. The page
sends a request (`npc/protocol.ts`), and the main process checks it,
rate-limits it and calls Claude with structured outputs.

## The packs in more detail

### graphics

- `sky.ts`: the physical sky, sun and stars, plus the environment map made from
  them (PMREM), including the city's glow at night.
- `post.ts`: the effects chain. GTAO ambient occlusion, bloom, depth of field in
  photo mode, tone mapping, then a lens pass (looks, vignette, grain, chromatic
  aberration, speed blur).
- `surfaces.ts`: wet roads and puddles, detail on the ground, contact shadows
  under cars, and metal flake in metallic paint.
- `carBody.ts`: sculpted car bodies, lofted from each body type's profile. They
  follow the garage settings (length, width, height, wheel size and track,
  paint, livery, stripes, tint, rims and accent colour).
- `lights.ts`: street lamps that really light the road at night, and headlights.

### physics

- `model.ts`: the tyre model (a simplified Pacejka curve with a friction
  ellipse for combined slip), weight transfer, drivetrain, aero and driving
  aids. It's used for every car through `CarPhysics.dynamics`.
- `telemetry.ts`: the panel showing each tyre's load and grip.

### npc

- `cast.ts`: who everyone is. Looks, traits, jobs, friends.
- `mind.ts`: memory (scored by recency, importance and relevance), feelings,
  needs, mood, learned likes, and reflection into beliefs and habits.
- `brain.ts`: what they say. `AiBrain` asks Claude. `RuleBrain` runs without a
  key and covers conversations out of earshot.
- `protocol.ts`: the request and answer shapes, the models, the standing
  instructions, and the JSON schemas.
- `city.ts`: the places and walking routes. `body.ts`: their bodies and
  animation. `labels.ts`: name tags and speech bubbles.
- `index.ts`: the simulation, reactions to the player, BUDDY's tickets, the chat
  box (K), the journal (L), and the Packs tab options.
- `main.ts`: the main-process side that keeps the key and calls Claude.
