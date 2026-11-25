# Game packs

Packs are optional add-ons that live in this folder, one folder each. To install
them, see the [Packs section of the main README](../README.md#packs). This file
is for anyone writing a pack.

| Pack | Folder | What it does |
| --- | --- | --- |
| 🌅 Photo Real | `graphics/` | Sky and sunlight, image-based lighting, wet roads, real car models, headlight beams and shadows, post-processing |
| 🧪 Real Physics | `physics/` | Suspension, tyre model with heat and wear, engines, turbos, gearbox and differentials, brake fade, crash damage, telemetry, rumble |
| 🧠 Living City | `npc/` | A city of 48 people with routines, memories, friendships and conversations, written by a local open-source AI |

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

The Living City pack works this way. Its main part runs the local AI model with
[node-llama-cpp](https://node-llama-cpp.withcat.ai) and keeps the city's saved
memories in its data folder (`userData/packs/npc`), which updates don't touch.
The page sends a request (`npc/protocol.ts`), and the main process checks it,
queues it (your chats first) and has the model answer in a JSON schema.

A package that loads native code can't be bundled into one file. List it for
your pack in `scripts/native.mjs`: the build leaves it out of the bundle, and
packaging copies it (and what it depends on) into the app, with its native
files outside the archive.

## The packs in more detail

### graphics

- `sky.ts`: the physical sky, sun and stars, plus the environment map made from
  them (PMREM), including the city's glow at night.
- `post.ts`: the effects chain. GTAO ambient occlusion, bloom, depth of field in
  photo mode, tone mapping, then a lens pass (looks, vignette, grain, chromatic
  aberration, speed blur, raindrops on the lens).
- `surfaces.ts`: wet roads and puddles, detail on the ground, contact shadows
  under cars, and metal flake in metallic paint.
- `carBody.ts`: sculpted car bodies, lofted from each body type's profile. They
  follow the garage settings (length, width, height, wheel size and track,
  paint, livery, stripes, tint, rims and accent colour).
- `realCars.ts`: real car models from `models/cars.glb`, one per body type, put
  over the sculpted body once loaded (it stays if they can't load). Each is
  stretched round the game's own wheels and takes the garage's paint, pattern,
  stripes, tint and lights; far-away cars use a simpler copy. The models are
  by Daniel Zhabotinsky (CC BY 4.0) with every badge and plate removed; see
  `models/CREDITS.md`. The Packs tab's "Car models" setting switches between
  real and sculpted bodies.
- `beams.ts`: headlight beams you can see in the night air, stronger in rain and snow.
- `lights.ts`: street lamps that really light the road at night, and
  headlights (with shadows, if switched on).

### physics

- `model.ts`: the driving model, used for every car through
  `CarPhysics.dynamics`. It runs four times per physics step (480 Hz). It
  brings together the parts below, plus aero, the driving aids, the realism
  levels (Friendly, Real, Simulation) and crash damage. It sets the body's
  lean on the model through the `pose` hook.
- `suspension.ts`: a sprung body that can heave, pitch and roll on four
  springs. It has two-stage dampers, bump stops and anti-roll bars. The road
  under each wheel comes from `world.heightAt`, plus a fixed pattern of small
  bumps. The springs give the weight on each tyre.
- `tyres.ts`: the grip curve, a simplified Pacejka "Magic Formula" with a
  friction ellipse for combined slip. Also tyre temperature and wear, and how
  they change grip.
- `powertrain.ts`: torque curves for each engine, turbo lag, the automatic
  gearbox, and limited-slip differentials.
- `telemetry.ts`: the panel. It shows the g circle, and each tyre's grip,
  load, spring travel, temperature and wear. It also shows revs, boost, gear,
  brake temperature, the aid lights and damage.
- `rumble.ts`: gamepad rumble (the browser Gamepad API's `dual-rumble`) from
  tyre slip, bumps, landings and crashes.

### npc

- `cast.ts`: who everyone is. The twelve main characters, written by hand,
  and `residents.ts`, which makes the other 36 (the same people every time):
  looks, traits, jobs, friends, ways of talking.
- `mind.ts`: memory (scored by recency, importance and relevance), feelings,
  needs, mood, learned likes, reflection into beliefs, habits and a life
  story, talks with you, and friendship with you. Standing decisions too:
  up to five, in their own words, kept until they drop one (with no room
  left, a new one waits until they do). `forbids` reads the ones it can,
  conditions included ("unless they're speeding"), so the built-in brain
  keeps them as well.
- `brain.ts`: what they say and decide. `LocalBrain` asks the local AI.
  `RuleBrain` needs nothing, and covers whatever the AI hasn't time for.
  `decide` is a free choice: a private thought, a goal, an action from an open
  menu (with a place or a person) or "something else" in their own words, a
  standing decision made or dropped, and anything said out loud. In a chase
  BUDDY uses it to decide whether to keep going. `plan` is a goal and a plan
  for the day.
- `protocol.ts`: the request and answer shapes, the models, the standing
  instructions, the actions anyone can choose (and BUDDY's police ones), and
  the JSON schemas.
- `city.ts`: the places, homes and walking routes. `body.ts`: their bodies and
  animation. `labels.ts`: name tags (click one to talk), speech and thought
  bubbles.
- `drive.ts`: people's cars. Routes along the roads (on the right, turning at
  the crossings, pulling in at the kerb) and `NpcCar`, a world car driven
  along one. At most three are out at once.
- `police.ts`: BUDDY's police car. It uses the game's car physics and is
  driven the way the rival drivers drive, with a light bar, a siren, a roof
  gun and a hit box for your weapons. It patrols along routes and chases you
  (straight at you, or round the blocks).
- `index.ts`: the simulation. It runs the city's clock and daily routines and
  each person's decisions: the AI decides for people near you, one at a time,
  and everyone else follows their plan and the built-in brain. It also covers
  car trips and lifts, who talks to whom, plans to meet, and reactions to the
  player. On the police side: your wanted level, BUDDY's tickets, chases (an
  escape, a bust and a fine, or BUDDY calling it off when he thinks it over
  every 20 seconds or so) and the mischief he fines people for. Their choices
  stand: the game only stops what can't happen (chases switched off, a
  wrecked police car). Then
  texts from friends, ride hails, the Saturday street party, making friends,
  clicking on people, and saving (version 3; older saves still load, and get
  standing decisions and a "called off" count when they're first saved). It all
  steps aside during events.
- `ui.ts`: the chat box (beside them, by phone, or with them riding in your
  car), the journal (L) and the Packs tab options, including police chases.
- `main.ts`: the main-process side. It downloads and runs the model, queues
  requests, and saves the city.
