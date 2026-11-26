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

## Install and update from source

Runs on macOS, Linux and Windows. It needs [Git](https://git-scm.com) and
[Node.js](https://nodejs.org) 22.12 or newer. Each command comes three ways: pick the
one for the terminal you use. The Windows PowerShell and Command Prompt versions run
the same script through the bash that comes with Git (if you installed Git somewhere
other than Program Files, change the path).
Installing adds Car Game to your apps: open it from Launchpad or Spotlight on macOS,
the Start Menu or Desktop on Windows, or the app menu on Linux. Press H in the game
to see the controls.

**Install:**

*macOS, Linux, or Git Bash on Windows:*

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash
```

*Windows PowerShell:*

```powershell
& "$env:ProgramFiles\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash"
```

*Windows Command Prompt:*

```bat
"%ProgramFiles%\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash"
```

**Update:**

*macOS, Linux, or Git Bash on Windows:*

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/update.sh | bash
```

*Windows PowerShell:*

```powershell
& "$env:ProgramFiles\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/update.sh | bash"
```

*Windows Command Prompt:*

```bat
"%ProgramFiles%\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/update.sh | bash"
```

## Building the Rust version

Car Game 1.3.0 is being rebuilt in [Rust](https://www.rust-lang.org) on the
[Bevy](https://bevyengine.org) game engine, on the `features/v1.3.0` branch. Until
it's finished, the commands above install the current version.

To build it, install Rust with [rustup](https://rustup.rs). The `rust-toolchain.toml`
file picks the exact Rust version, and rustup installs it the first time. Then:

```bash
cargo run --release
```

Every library the game uses is stored in `vendor/`, so building never downloads
anything. Only the platforms the game ships for are kept (macOS on Apple Silicon and
Intel, Windows x64, Linux x64). To add or update a library, change `Cargo.toml`, then
refresh `vendor/` with
[cargo-vendor-filterer](https://github.com/coreos/cargo-vendor-filterer):

```bash
cargo-vendor-filterer --versioned-dirs \
  --platform aarch64-apple-darwin --platform x86_64-apple-darwin \
  --platform x86_64-pc-windows-msvc --platform x86_64-unknown-linux-gnu \
  --exclude-crate-path '*#tests' --exclude-crate-path '*#benches' vendor
```

(Move `.cargo/config.toml` aside while it runs, since it switches downloads off.)

To check the game draws without watching it, set `CAR_GAME_SCREENSHOT` to a file
name: it saves a picture once the game has settled, then quits.

## Packs

Packs are optional add-ons. Each one changes a whole part of the game. Packs
need the game installed from source, as above. Each command installs the game
first if it isn't installed yet, then adds the pack.

**🌅 Photo Real** (`graphics`): makes the game look real. It adds a physically
based sky with a real sun and clouds, and light reflected from the sky onto
every surface. Roads get wet and puddles in the rain. Cars get real, detailed
car bodies with no badges or logos (by Daniel Zhabotinsky, CC BY 4.0), shaped
and painted by your garage settings, with clearcoat paint, metal flake, brake
discs and calipers; the Packs tab can switch them back to sculpted bodies. It
also adds ambient occlusion, soft bloom, street lamps that light the road at
night, headlights that shine beams through the night air and throw shadows,
raindrops on the lens, and a camera lens with film-style looks and photo-mode
depth of field.

*macOS, Linux, or Git Bash on Windows:*

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack graphics
```

*Windows PowerShell:*

```powershell
& "$env:ProgramFiles\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack graphics"
```

*Windows Command Prompt:*

```bat
"%ProgramFiles%\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack graphics"
```

**🧪 Real Physics** (`physics`): realistic driving. The body rides on real
springs, dampers and anti-roll bars, so it dives under braking, leans in
corners and bounces over curbs, and the weight on each tire moves with it.
Each of the four tires has real grip curves, warms up as you push and wears
as it slides. Each kind of engine has its own torque curve, and turbos lag.
Gear changes take a moment, and limited-slip differentials put the power
down. Hard-used brakes fade, and every surface has its own grip. Crashes spin
the car, dent it, and can hurt the engine or bend a wheel and the steering.
There are realism levels from Friendly to Simulation. A telemetry panel shows
it all at work, and a controller rumbles as the tires slide.

*macOS, Linux, or Git Bash on Windows:*

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack physics
```

*Windows PowerShell:*

```powershell
& "$env:ProgramFiles\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack physics"
```

*Windows Command Prompt:*

```bat
"%ProgramFiles%\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack physics"
```

**🧠 Living City** (`npc`): a whole city of 48 people, each with a name over
their head, a job, a home, friends and a personality of their own. They live
through the city's days: they get up, grab a coffee, go to work, have lunch,
meet friends in the evening and go home to sleep. They stop to talk when they
run into each other, gossip about each other and about you, and make plans to
meet again, which they keep. Over time they change: they form beliefs, pick up
habits, make friends and hold grudges, and write their own life stories. Among
them are GUY, the cheerful bank teller in the blue shirt, and his best friend
BUDDY, a police officer who goes everywhere with him and writes you tickets.

They're completely free. Each has a goal and a plan for the day, and private
thoughts (💭) you can read in the journal. They choose for themselves what to
do next, whether that's lunch, a drive across town in their car, giving a
friend a lift, getting up to mischief, texting you, or anything else they can
put into words, like painting a mural in the park or dancing in the street.
They can make lasting decisions about their own lives ("I'm going to quit my
job and paint"), which they keep, and the journal shows, until they change
their mind. The game never overrules them.

BUDDY drives a police car with lights and a siren, and GUY rides along. What
to do about you is always BUDDY's call: let it go, write a ticket, or come
after you. In a chase he rams you and fires the roof gun, and a wanted level
shows how much trouble you're in. Lose him to escape, or pull over and pay the
fine. Every so often he thinks the chase over (how battered his car is,
whether you've calmed down, what GUY says) and may call it off. He can even
decide he's done chasing you, and stick to it. Everyone remembers the chase.
People also wave you down for lifts and tip you when you drop them off. On
Saturday nights the whole city throws a street party in the park. You can
turn chases off in the Packs tab.

Their conversations and thoughts are written by a free, open-source AI model
(Qwen3.5, Apache 2.0) that runs on your own computer through
[llama.cpp](https://github.com/ggml-org/llama.cpp). Nothing is sent anywhere
and it costs nothing. The first time the pack runs, it downloads the model that
suits your computer, once: Qwen3.5 4B (2.7 GB) with 16 GB of memory or more,
otherwise Qwen3.5 2B (1.3 GB). You can pick a smaller or larger model, or run
it on the processor instead of the graphics chip, in the Packs tab. Until the
model is ready, a simpler built-in brain keeps the city going.

Click on anyone (or pull up next to them and press K) to talk to them. Talk to
people often and treat them well, and they become your friends: they wave when
you drive by, greet you by name and remember what you talked about. Everyone's
memories, friendships and life stories are saved in the game's data folder,
which updates leave alone, so they last as long as the game is installed. L
opens the city journal: what everyone is doing, what they've been saying to each
other, their plans, and your friends (city free roam).

*macOS, Linux, or Git Bash on Windows:*

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack npc
```

*Windows PowerShell:*

```powershell
& "$env:ProgramFiles\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack npc"
```

*Windows Command Prompt:*

```bat
"%ProgramFiles%\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack npc"
```

**All three packs:**

*macOS, Linux, or Git Bash on Windows:*

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack all
```

*Windows PowerShell:*

```powershell
& "$env:ProgramFiles\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack all"
```

*Windows Command Prompt:*

```bat
"%ProgramFiles%\Git\bin\bash.exe" -c "curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack all"
```

The update command above also updates your packs. To switch a pack off, change
its options or remove it, open the garage (G) and go to the **Packs** tab. A
removed pack stops straight away and is left out the next time the game
updates. To make a pack of your own, see [packs/README.md](packs/README.md).

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

Put the version in the commit message on `main` (or in the pull request's title),
like `v1.2.0: more maps`. GitHub Actions then builds the game for macOS, Windows
and Linux, tags the commit, and publishes a release with the three downloads (see
`.github/workflows/release.yml`). A commit without a version, or with one that's
already out, doesn't release anything. Every pull request is also type-checked
and built (`check.yml`).

