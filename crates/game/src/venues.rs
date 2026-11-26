//! The places you can drive: the city and the race track venues. Each venue
//! has a look (sky, ground, trees, weather, scenery) and a race track style.
//!
//! A venue's main track (layout 0) is the one you drive in free roam. Any
//! other layout number makes a different track with `generate_layout`, so
//! career levels and sandbox races can use endless tracks.
use crate::track::{LayoutOptions, TrackStep, TrackStyle, generate_layout};

pub const MAP_NAMES: &[&str] = &["city", "circuit", "oval", "canyon", "alpine", "neon", "forest", "kart"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    Grass,
    Sand,
    Snow,
    Dirt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trees {
    Round,
    Pine,
    Cactus,
    SnowPine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
    Clear,
    Rain,
    Snow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenery {
    Paddock,
    Mesas,
    Mountains,
    City,
    Woods,
}

/// What a venue looks like.
#[derive(Clone, Debug)]
pub struct Theme {
    /// Sky gradient, top to bottom, in the daytime.
    pub sky: [&'static str; 2],
    pub fog: u32,
    pub ground: Ground,
    pub hills: u32,
    pub trees: Trees,
    pub night: bool,
    pub weather: Weather,
    /// How grippy the road is: 1 is dry asphalt.
    pub grip: f64,
    pub scenery: Scenery,
    pub banner: &'static str,
}

pub struct Venue {
    pub value: &'static str,
    pub label: &'static str,
    pub about: &'static str,
    pub theme: Theme,
    pub style: Option<TrackStyle>,
    pub layout: Option<Vec<TrackStep>>,
    pub generate: Option<LayoutOptions>,
    pub seed: Option<f64>,
}

const DAY: Theme = Theme {
    sky: ["#5fa8e8", "#cfe5f5"],
    fog: 0xc4dcef,
    ground: Ground::Grass,
    hills: 0x55734a,
    trees: Trees::Round,
    night: false,
    weather: Weather::Clear,
    grip: 1.0,
    scenery: Scenery::Paddock,
    banner: "GRAND PRIX",
};

/// The speedway's turn radius.
pub const OVAL_RADIUS: f64 = 220.0;

fn circuit() -> Vec<TrackStep> {
    use TrackStep::*;
    vec![
        Straight(238.42),
        Corner(110.0, 40.0),
        Straight(120.0),
        Corner(-50.0, 80.0),
        Straight(90.0),
        Corner(120.0, 50.0),
        Straight(120.0),
        Corner(90.0, 40.0),
        Straight(130.0),
        Corner(-180.0, 32.0),
        Straight(40.0),
        Corner(90.0, 35.0),
        Straight(60.0),
        Corner(-40.0, 50.0),
        Corner(40.0, 50.0),
        Straight(60.0),
        Corner(90.0, 45.0),
        Straight(145.14),
        Corner(30.0, 40.0),
        Corner(-30.0, 40.0),
        Straight(50.0),
        Corner(90.0, 35.0),
        Straight(200.0),
    ]
}

fn style(
    width: f64,
    bank: [f64; 2],
    curbs: bool,
    outside: f64,
    inside: Option<f64>,
    apron: f64,
    wall: f64,
    fence: f64,
    colors: [&'static str; 2],
) -> TrackStyle {
    TrackStyle {
        width,
        bank,
        curbs,
        outside_runoff: outside,
        inside_runoff: inside,
        no_walls: false,
        apron,
        wall_height: wall,
        fence,
        wall_colors: colors,
    }
}

fn layout_options(length: f64, radius: [f64; 2], corners: [f64; 2], clearance: f64) -> LayoutOptions {
    LayoutOptions {
        length,
        radius,
        corners,
        left_only: false,
        clearance,
    }
}

pub fn venues() -> &'static [Venue] {
    use std::sync::OnceLock;
    static VENUES: OnceLock<Vec<Venue>> = OnceLock::new();
    VENUES.get_or_init(|| {
        use TrackStep::*;
        vec![
            Venue {
                value: "city",
                label: "City",
                about: "Streets, parks, a parking lot and a stunt park, all destroyable.",
                theme: Theme {
                    scenery: Scenery::City,
                    ..DAY
                },
                style: None,
                layout: None,
                generate: None,
                seed: None,
            },
            Venue {
                value: "circuit",
                label: "Grand Prix circuit",
                about: "A twisty 2 km track: fast sweepers, a hairpin, esses and a chicane, with curbs and grandstands.",
                theme: DAY,
                style: Some(style(16.0, [0.0, 0.0], true, 9.0, Some(9.0), 0.0, 1.1, 0.0, ["#d7263d", "#f2f2f2"])),
                layout: Some(circuit()),
                generate: Some(layout_options(2100.0, [30.0, 110.0], [8.0, 12.0], 64.0)),
                seed: None,
            },
            Venue {
                value: "oval",
                label: "Speedway",
                about: "A 3 km oval with turns banked at 26°. Built for 500 mph.",
                theme: Theme { banner: "SPEEDWAY", ..DAY },
                style: Some(style(24.0, [5.0, 26.0], false, 0.6, None, 10.0, 1.2, 5.0, ["#f4f4f4", "#2b6fd6"])),
                layout: Some(vec![
                    Straight(400.0),
                    Corner(180.0, OVAL_RADIUS),
                    Straight(800.0),
                    Corner(180.0, OVAL_RADIUS),
                    Straight(400.0),
                ]),
                generate: Some(LayoutOptions {
                    left_only: true,
                    ..layout_options(2700.0, [150.0, 320.0], [4.0, 6.0], 120.0)
                }),
                seed: None,
            },
            Venue {
                value: "canyon",
                label: "Red Rock Canyon",
                about: "A long, fast desert track between towering red mesas. Sand everywhere off the road.",
                theme: Theme {
                    sky: ["#3d8fd6", "#f6d7a8"],
                    fog: 0xf0cf9e,
                    ground: Ground::Sand,
                    hills: 0xb5552b,
                    trees: Trees::Cactus,
                    night: false,
                    weather: Weather::Clear,
                    grip: 1.0,
                    scenery: Scenery::Mesas,
                    banner: "CANYON RUN",
                },
                style: Some(style(14.0, [0.0, 4.0], true, 7.0, Some(7.0), 0.0, 1.2, 0.0, ["#a0522d", "#e8c39e"])),
                layout: None,
                generate: Some(layout_options(2600.0, [28.0, 140.0], [9.0, 13.0], 56.0)),
                seed: Some(3.0),
            },
            Venue {
                value: "alpine",
                label: "Alpine Pass",
                about: "Snowy mountain roads in falling snow. The road is slippery, so brake early.",
                theme: Theme {
                    sky: ["#8fb4d9", "#e9f1f8"],
                    fog: 0xe4edf5,
                    ground: Ground::Snow,
                    hills: 0xdfe7ef,
                    trees: Trees::SnowPine,
                    night: false,
                    weather: Weather::Snow,
                    grip: 0.8,
                    scenery: Scenery::Mountains,
                    banner: "ALPINE PASS",
                },
                style: Some(style(13.0, [0.0, 6.0], true, 6.0, Some(6.0), 0.0, 1.0, 0.0, ["#2b6fd6", "#f2f2f2"])),
                layout: None,
                generate: Some(layout_options(2200.0, [25.0, 100.0], [10.0, 14.0], 55.0)),
                seed: Some(6.0),
            },
            Venue {
                value: "neon",
                label: "Neon Night",
                about: "A tight street circuit through a city that never sleeps. Concrete walls, no room for mistakes.",
                theme: Theme {
                    sky: ["#0a0f24", "#2a1a48"],
                    fog: 0x1a1430,
                    ground: Ground::Dirt,
                    hills: 0x1b1d2b,
                    trees: Trees::Round,
                    night: true,
                    weather: Weather::Clear,
                    grip: 1.0,
                    scenery: Scenery::City,
                    banner: "NEON NIGHT",
                },
                style: Some(style(14.0, [0.0, 0.0], true, 2.5, Some(2.5), 0.0, 1.3, 2.5, ["#ff3bd0", "#2b2e33"])),
                layout: None,
                generate: Some(layout_options(1900.0, [18.0, 60.0], [10.0, 14.0], 52.0)),
                seed: Some(7.0),
            },
            Venue {
                value: "forest",
                label: "Forest Rally",
                about: "A narrow rally stage through the woods, in the rain. No barriers, just trees.",
                theme: Theme {
                    sky: ["#6f7f8c", "#b9c4cc"],
                    fog: 0xaab6bf,
                    ground: Ground::Grass,
                    hills: 0x3d5a35,
                    trees: Trees::Pine,
                    night: false,
                    weather: Weather::Rain,
                    grip: 0.85,
                    scenery: Scenery::Woods,
                    banner: "FOREST RALLY",
                },
                style: Some(TrackStyle {
                    no_walls: true,
                    ..style(11.0, [0.0, 3.0], false, 0.0, Some(0.0), 0.0, 1.0, 0.0, ["#6b4a2f", "#d9c9a3"])
                }),
                layout: None,
                generate: Some(layout_options(2100.0, [22.0, 90.0], [10.0, 15.0], 50.0)),
                seed: Some(5.0),
            },
            Venue {
                value: "kart",
                label: "Kart Park",
                about: "A short, tight, twisty track with tire walls. Corners come thick and fast.",
                theme: Theme {
                    banner: "KART PARK",
                    ..DAY
                },
                style: Some(style(9.0, [0.0, 0.0], true, 3.0, Some(3.0), 0.0, 0.8, 0.0, ["#1a1a1a", "#f2f2f2"])),
                layout: None,
                generate: Some(layout_options(950.0, [12.0, 35.0], [10.0, 16.0], 34.0)),
                seed: Some(3.0),
            },
        ]
    })
}

/// The venue with this name (the city if there's none).
pub fn venue(map: &str) -> &'static Venue {
    venues().iter().find(|v| v.value == map).unwrap_or(&venues()[0])
}

/// The track for a venue's layout number: its main track for 0, otherwise a generated one.
pub fn track_layout(v: &Venue, layout: u32) -> Vec<TrackStep> {
    if layout == 0
        && let Some(l) = &v.layout
    {
        return l.clone();
    }
    let index = venues().iter().position(|o| std::ptr::eq(o, v)).unwrap_or(0) as f64;
    let seed = if layout == 0 {
        v.seed.unwrap_or(1.0)
    } else {
        1000.0 + layout as f64 * 31.0 + index * 100003.0
    };
    generate_layout(seed, v.generate.as_ref().expect("a race venue"))
}
