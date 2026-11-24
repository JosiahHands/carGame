# Car Game

A 3D driving game. Race AI rivals through a 100-level career, set up quick races
with any car, or build your car in the garage and drive anywhere in free roam.

## Download

Ready-to-run builds for macOS (Apple Silicon), Windows and Linux are on the
[latest release](https://github.com/JosiahHands/carGame/releases/latest). Unzip and
run it. These builds aren't signed by Apple or Microsoft, so the first time:

- **macOS:** right-click Car Game and pick Open. If it says the app is damaged, run
  `xattr -cr "/Applications/Car Game.app"` in Terminal, then open it again.
- **Windows:** if SmartScreen warns you, click More info, then Run anyway.

## Install and update from source

Runs on macOS, Linux and Windows. It needs [Git](https://git-scm.com) and
[Node.js](https://nodejs.org) 22.12 or newer. On Windows, run the commands in Git Bash.
Installing adds Car Game to your apps: open it from Launchpad or Spotlight on macOS,
the Start Menu or Desktop on Windows, or the app menu on Linux. Press H in the game
to see the controls.

**Install:**

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash
```

**Update:**

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/update.sh | bash
```

## Packs

Packs are optional add-ons. Each one changes a whole part of the game. Packs
need the game installed from source, as above. Each command installs the game
first if it isn't installed yet, then adds the pack.

**🌅 Photo Real** (`graphics`): makes the game look real. It adds a physically
based sky with a real sun and clouds, and light reflected from the sky onto
every surface. Roads get wet and puddles in the rain. Cars get sculpted, rounded
bodies that follow your garage settings, with clearcoat paint, metal flake,
brake discs and calipers. It also adds ambient occlusion, soft bloom, street
lamps that light the road at night, headlights, and a camera lens with
film-style looks and photo-mode depth of field.

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack graphics
```

**🧪 Real Physics** (`physics`): realistic driving. Each of the four tyres has
real grip curves. Weight shifts under braking and cornering. The engine has a
torque curve and gears, and every surface has its own grip. Glancing crashes
spin the car and dent it, and a telemetry panel shows the tyres at work.

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack physics
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

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack npc
```

**All three packs:**

```bash
curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack all
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
gearbox, tyres, brakes, nitro, aero, armour). Each level gives up to three stars,
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
car (stock or fully upgraded), or a showcase build. Rivals get cars as quick as yours.

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

- **City:** streets, parks, a car park and a stunt park full of ramps and cones.
- **Grand Prix circuit:** a twisty 2 km race track with fast sweepers, a
  hairpin, esses and a chicane. It has red and white kerbs, barriers,
  grandstands full of fans, floodlights, and a paddock with pit garages and
  team cars. Grass off the track slows you down.
- **Speedway:** a 3 km oval with turns banked at 26°. Cars lean into the
  banking, and a 500 mph build can stay flat out on the high line. Drive off
  the apron into the infield to find the paddock and a row of stunt ramps.
- **Red Rock Canyon:** fast desert roads between red mesas.
- **Alpine Pass:** snowy mountain roads in falling snow, where the road is slippery.
- **Neon Night:** a tight street circuit through a city at night.
- **Forest Rally:** narrow stages through the woods in the rain, with no barriers.
- **Kart Park:** a short, tight, twisty track with tyre walls.

## Free roam

Pick the map on the garage's Maps tab. Press T for a timed lap on any map. A start
gantry over the line counts you down with five red lights. Each map keeps its own
best lap, and once you have one, a see-through ghost of that lap races you.

Your car has a roof machine gun, a rocket pod and a shockwave blast. Buildings,
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
pavements, leap out of your way (nobody gets hurt), run from explosions, and
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
- **Wheels:** rims, rim and tyre colours, wheel size up to 4× (big wheels turn it
  into a monster truck), tyre width, wheel spacing, camber and a suspension lift of
  up to 3 m.
- **Parts:** spoilers up to a dragster wing, exhausts including truck stacks, roof
  toppers (crown, top hat, cone, Viking horns, shark fin, halo, rubber duck),
  antennas, a bull bar, a light bar and the roof weapons.
- **Lights, sound and effects:** headlights, solid or rainbow underglow, nitro flame
  colour, boost trails (flames, rainbow, sparkles, smoke), coloured tyre smoke, five
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

