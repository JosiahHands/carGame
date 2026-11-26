# Car Game

A 3D driving game. Race AI rivals through a 100-level career, set up quick races
with any car, or build your car in the garage and drive anywhere in free roam.

## Download

Pick your computer, download, and play. No setup or terminal needed.

| Computer | Download |
| --- | --- |
| **Mac** with Apple Silicon (M1 or newer) | [Car-Game-macOS-Apple-Silicon.zip](https://github.com/JosiahHands/carGame/releases/latest/download/Car-Game-macOS-Apple-Silicon.zip) |
| **Mac** with Intel | [Car-Game-macOS-Intel.zip](https://github.com/JosiahHands/carGame/releases/latest/download/Car-Game-macOS-Intel.zip) |
| **Windows** (installer) | [Car-Game-Windows-Setup.exe](https://github.com/JosiahHands/carGame/releases/latest/download/Car-Game-Windows-Setup.exe) |
| **Windows** (no install) | [Car-Game-Windows.zip](https://github.com/JosiahHands/carGame/releases/latest/download/Car-Game-Windows.zip) |
| **Linux** | [Car-Game-Linux.AppImage](https://github.com/JosiahHands/carGame/releases/latest/download/Car-Game-Linux.AppImage) or [Car-Game-Linux.zip](https://github.com/JosiahHands/carGame/releases/latest/download/Car-Game-Linux.zip) |

These links always get the newest version. Every version is on the
[releases page](https://github.com/JosiahHands/carGame/releases). The downloads
aren't signed by Apple or Microsoft, so the first time:

- **Mac:** unzip, drag Car Game to Applications, then right-click it and pick Open.
  If it says the app is damaged, run `xattr -cr "/Applications/Car Game.app"` in
  Terminal, then open it again.
- **Windows:** if SmartScreen warns you, click More info, then Run anyway.
- **Linux:** make the AppImage runnable (right-click, Properties, Allow executing,
  or `chmod +x Car-Game-Linux.AppImage`), then open it.

## Install or update

Run these commands in Terminal on macOS or Linux, or in Git Bash on Windows.
They download the ready-built app from the same release as the links above.
Press H in the game to see the controls.

**Install**

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/tools/install.sh | bash
```

**Update**

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/tools/update.sh | bash
```

## Packs

Open **Garage → Packs** to enable any of the three optional packs. Each can
be switched off at any time.

- **Photo Real** adds detailed car bodies, a physical sky and camera effects.
  The car models are by Daniel Zhabotinsky (CC BY 4.0). See the [model credits](packs/photo_real/libs/car-models/CREDITS.md).
- **Real Physics** adds suspension, individual tire grip and temperatures, a
  powertrain model, crash damage and driving aids, with a telemetry panel while you drive.
- **Living City** adds residents who work, meet, drive and remember what happens
  around them, plus messages, a city journal and police pursuits. Press K near
  someone to talk and L to open the journal. The built-in brain works offline.
  An optional Qwen model can be downloaded in the Packs tab and runs on your computer.

## Build from source

Install [Rust with rustup](https://rustup.rs), CMake and a C++ toolchain with
libclang. On macOS install the Xcode command-line tools. On Windows use Visual
Studio Build Tools with Desktop development with C++ and set `LIBCLANG_PATH`
to LLVM’s `bin` folder if it is outside the default search paths. On Debian or
Ubuntu install `libasound2-dev`, `libudev-dev`, `libwayland-dev`,
`libxkbcommon-dev` and `libclang-dev`.

Run `cargo run --release --locked --offline` from the checkout. The program
is built into `builds/release/`. New installs select graphics settings based
on the computer. Garage → Settings lets you change the preset, resolution,
shadows, effects, draw distance and frame-rate cap.

How the code is laid out:

| Folder | What's in it |
| --- | --- |
| `game/` | The game. `backEnd/` has its rules and data (the main loop, physics, worlds, events, career and saves). `frontEnd/` has what you see, hear and touch (graphics, car models, menus, HUD, sound, controls). `shared/` has small things both use. `main.rs` starts it. |
| `packs/` | The three optional packs. Each pack's folder is self-contained, with its `code/` and its own `libs/`. |
| `libs/` | Every library the game uses, stored so that building never downloads anything. |
| `tools/` | Packaging, the smoke tests, and the install and update scripts. |
| `builds/` | Everything built: the program and the downloads (`tools/packaging/package.sh` puts them in `builds/downloads/`). Not kept in git. |

To add or update a library, move `.cargo/config.toml` aside, change
`Cargo.toml`, and store the libraries again with
`cargo vendor --versioned-dirs libs` (or `cargo-vendor-filterer`). A library
only one pack uses then moves from `libs/` into that pack's `libs/`, with a
`[patch.crates-io]` entry for it in the top-level `Cargo.toml`.

On first launch the game imports 1.2.x career progress, garage builds, settings,
best times, ghosts and city memories from the old app’s data folder.

## Career

100 levels in ten chapters, from the Rookie Cup to the Legends, each with its own
venue and a boss to beat at the end (the last one is The Phantom). Every level
pays coins: spend them on six cars (Hot Hatch, Muscle Car, Rally Truck, Sports GT,
Supercar, Hypercar), each with its own perk, and on seven upgrades (engine,
gearbox, tires, brakes, nitro, aero, armor). A career starts with no weapons:
buy a machine gun from level 6 (800 coins), a rocket pod from level 11 (3,500)
and a shockwave from level 31 (10,000), then upgrade each one four times for
more damage and a cooler gun or faster reloads. Your career car carries only
what you've bought, in events with weapons and in free roam. Each level gives up to three stars,
and a chapter's boss only lets you race once you have 18 stars from that chapter,
so it pays to go back for three stars. A daily challenge pays double, more for a
streak of days in a row.

Nine kinds of event:

- **Race:** finish in the top three. Some are combat races, with weapons on.
- **Elimination:** the car in last place is knocked out every few seconds.
- **Boss duel:** one on one against a chapter's boss.
- **Time attack:** beat target times set by a pace car.
- **Takedown:** wreck rivals with guns, rockets and rams while they shoot back.
- **Drift:** score drift points against the clock.
- **Speed trap:** hit the speed cameras as fast as you can.
- **Destruction:** do as much damage to the city as you can.
- **Stunt show:** as much air time as you can in the stunt park.

Rivals drive with the same physics as you. They brake for corners, take the
racing line, overtake, make mistakes, and get better level by level. After level 100,
pick **Sandbox** (endless levels that keep getting harder, with upgrades up to
level 20) or **Reset** (start again from level 1, with coins paying 25% more for
each finished career).

**Quick race** lets you pick any event, venue, track, number of laps and rivals,
and a difficulty from Easy to Insane. Drive any car: your garage build, any career
car (stock or fully upgraded), or a showcase build. Rivals get cars as quick as yours,
or pick their top speed yourself, from 60 to 1,000 mph.

## Skills, XP and rewards

Stylish driving builds a **skill chain**: drifts, near misses, slipstreaming,
jumps, overtakes, takedowns, smashes, flat-out speed and perfect starts all add
to it. Mix different skills for a bigger multiplier (up to ×10). The chain banks
a few seconds after your last skill, but a hard crash loses it all.

- **Drift boosts:** hold a drift for blue, then orange, then purple sparks, and
  let go for a turbo boost.
- **Perfect starts:** press the throttle just as the countdown ends for a boost.
  Press it too early and the wheels spin.
- **Slipstream:** tuck in behind a rival for more top speed.
- **Nemesis:** a rival who beats you twice becomes your nemesis. Finish ahead of
  them for revenge and bonus XP.

Banked chains and events earn XP. Every driver level gives a spin of the **prize
wheel**: coins, free upgrades, coin rushes (double coins from your next events)
or the jackpot. There are 57 **trophies**, each paying coins.

Fireworks, an orbiting camera, synth music that builds with your speed, speed
lines and an announcer finish it off. Each can be turned off in Settings.

## Venues

Eight venues, each with its own main track. Any other track number makes a new
generated track, so there are endless tracks for the career and quick races:

- **City:** streets, parks, a parking lot and a stunt park full of ramps and cones.
- **Grand Prix circuit:** a twisty 2 km race track with fast sweepers, a
  hairpin, esses and a chicane. It has red and white curbs, barriers,
  grandstands full of fans, floodlights, and a paddock with pit garages and
  team cars. Grass off the track slows you down.
- **Speedway:** a 3 km oval with turns banked at 26°. Cars lean into the
  banking, and a 500 mph build can stay flat out on the high line. Drive off
  the apron into the infield to find the paddock and a row of stunt ramps.
- **Red Rock Canyon:** fast desert roads between red mesas.
- **Alpine Pass:** snowy mountain roads in falling snow, where the road is slippery.
- **Neon Night:** a tight street circuit through a city at night.
- **Forest Rally:** narrow stages through the woods in the rain, with no barriers.
- **Kart Park:** a short, tight, twisty track with tire walls.

## Free roam

Pick the map on the garage's Maps tab. Press T for a timed lap on any map. A start
gantry over the line counts you down with five red lights. Each map keeps its own
best lap, and once you have one, a see-through ghost of that lap races you.

Out of the career, your car has a roof machine gun, a rocket pod and a
shockwave blast whenever weapons are on (Settings → Weapons in free roam, and in
events with weapons). Guns only show on the roof when they can fire. Buildings,
parked cars, street lamps and trees can all be destroyed, on every map: buildings
collapse into rubble you can drive over, and everything is rebuilt once you drive
away. There is also a nitro boost, slow motion, a minimap and a photo mode for
saving screenshots.

The gearbox is automatic, or pick Manual in Settings and change gear yourself with
X and Z (D-pad right and left on a gamepad). A rev counter turns into a flashing
red shift light at the redline; hold a gear too long and you hit the rev limiter,
change up too early and the engine bogs down.

The city is alive in free roam: **traffic** drives the streets, keeps to its lane,
takes turns at the crossings, queues, and honks if you block the road. Ram it or
blow it up for points, or skim past for a near miss. **Pedestrians** walk the
sidewalks, leap out of your way (nobody gets hurt), run from explosions, and
cheer your big jumps. Set the traffic from Off to Busy, or turn pedestrians off,
in Settings.

Every map hides **gold wrenches** (68 in all; look for the beams of light, and
some need a jump) and has **challenges**: speed cameras, drift zones and jump
zones, each with three stars. Challenge stars count when you pick **Free roam in
your career car** from the main menu.

## The garage

The garage lets you build almost anything. In the career, your paint and parts go
on whichever car you are racing:

- **Size and shape:** six body styles. Scale the whole car from toy-sized to over
  100 ft tall, and stretch its length, width and height. A car about 10 ft tall or
  more flattens buildings, cars, lamps and trees in its way.
- **Paint:** eight finishes (including carbon, glow-in-the-dark and animated rainbow),
  patterns (two-tone, flames, checker, camo, tiger, dots), stripes, race numbers
  and window tint.
- **Wheels:** rims, rim and tire colors, wheel size up to 4× (big wheels turn it
  into a monster truck), tire width, wheel spacing, camber and a suspension lift of
  up to 3 m.
- **Parts:** spoilers up to a dragster wing, exhausts including truck stacks, roof
  toppers (crown, top hat, cone, Viking horns, shark fin, halo, rubber duck),
  antennas, a bull bar, a light bar and the roof weapons.
- **Lights, sound and effects:** headlights, solid or rainbow underglow, nitro flame
  color, boost trails (flames, rainbow, sparkles, smoke), colored tire smoke, five
  horns (V) and six engine sounds.
- **Engine and physics:** 50 to 10,000 hp, a top speed of up to 1,000 mph, nitro
  power, bottle size or infinite nitro, downforce, grip, brakes, steering lock,
  drift, a jump (J), air control, gravity from the Moon to beyond Jupiter, and wall
  bounce.

The Builds tab has ten ready-made showcase cars (a 100 ft Kaiju, a 500 mph record
car, a monster truck, a moon buggy and more), lets you save your own builds, and
gives every car a share code you can copy to a friend. Your car is saved between
sessions.

If the game runs slowly, press Esc for the settings: pick a quality preset (Low to
Ultra), or set the resolution, shadows, view distance, effects and a frame rate
limit yourself, and turn on the FPS counter to see the difference.

## Releasing a new version

Set the workspace version in `Cargo.toml` and include the matching version
(for example, `v1.3.0`) in the merge commit on `main`. The Native release
workflow builds the six packages and publishes them together on GitHub Releases.
The v1.3.0 feature pull request also builds the packages as workflow artifacts.
