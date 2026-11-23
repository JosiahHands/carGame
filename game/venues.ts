// The places you can drive: the city and the race track venues. Each venue
// has a look (sky, ground, trees, weather, scenery) and a race track style.
//
// A venue's main track (layout 0) is the one you drive in free roam. Any other
// layout number makes a different track at the same venue with
// generateLayout, so career levels and sandbox races can use endless tracks.
import { generateLayout, type LayoutOptions, type TrackStep, type TrackStyle } from './raceTrack';

export type MapName = 'city' | 'circuit' | 'oval' | 'canyon' | 'alpine' | 'neon' | 'forest' | 'kart';

/** What a venue looks like. */
export interface Theme {
  /** Sky gradient, top to bottom, in the daytime. */
  sky: [string, string];
  /** Fog colour in the daytime (it should match the bottom of the sky). */
  fog: number;
  ground: 'grass' | 'sand' | 'snow' | 'dirt';
  /** Colour of the hills on the horizon. */
  hills: number;
  trees: 'round' | 'pine' | 'cactus' | 'snowPine';
  /** Always night here (the neon city). */
  night: boolean;
  weather: 'clear' | 'rain' | 'snow';
  /** How grippy the road is: 1 is dry tarmac; rain and snow are slippery. */
  grip: number;
  /** Extra things round the track. */
  scenery: 'paddock' | 'mesas' | 'mountains' | 'city' | 'woods';
  /** Lap names for the start gantry's banner. */
  banner: string;
}

export interface Venue {
  value: MapName;
  label: string;
  about: string;
  theme: Theme;
  /** Race track look and barriers (not for the city). */
  style?: TrackStyle;
  /** Hand-drawn main track; the others are made by generateLayout. */
  layout?: TrackStep[];
  /** What generated tracks here are like. */
  generate?: LayoutOptions;
  /** The seed for the main track, when it's generated. */
  seed?: number;
}

const DAY: Theme = {
  sky: ['#5fa8e8', '#cfe5f5'], fog: 0xc4dcef, ground: 'grass', hills: 0x55734a, trees: 'round',
  night: false, weather: 'clear', grip: 1, scenery: 'paddock', banner: 'GRAND PRIX',
};

/**
 * The Grand Prix circuit, drawn as straights (metres) and corners (degrees to
 * turn, positive left, and the corner's radius). It starts on the start line
 * heading along the main straight.
 */
const CIRCUIT: TrackStep[] = [
  [238.42], // main straight, first half
  [110, 40], // turn 1: a long left
  [120],
  [-50, 80], // fast right kink
  [90],
  [120, 50], // turn 3: tight left onto the top straight
  [120],
  [90, 40], // left, diving into the infield
  [130],
  [-180, 32], // the hairpin
  [40],
  [90, 35], // left, back out
  [60],
  [-40, 50], // esses
  [40, 50],
  [60],
  [90, 45], // left onto the west straight
  [145.14],
  [30, 40], // chicane
  [-30, 40],
  [50],
  [90, 35], // last corner, onto the main straight
  [200], // main straight, second half, up to the start line
];

/** The speedway: two 800 m straights joined by two banked half circles. */
export const OVAL_RADIUS = 220;
const OVAL: TrackStep[] = [[400], [180, OVAL_RADIUS], [800], [180, OVAL_RADIUS], [400]];

export const VENUES: Venue[] = [
  {
    value: 'city',
    label: 'City',
    about: 'Streets, parks, a car park and a stunt park, all destroyable.',
    theme: { ...DAY, scenery: 'city' },
  },
  {
    value: 'circuit',
    label: 'Grand Prix circuit',
    about: 'A twisty 2 km track: fast sweepers, a hairpin, esses and a chicane, with kerbs and grandstands.',
    theme: DAY,
    style: { width: 16, bank: [0, 0], kerbs: true, outsideRunoff: 9, insideRunoff: 9, apron: 0, wallHeight: 1.1, fence: 0, wallColors: ['#d7263d', '#f2f2f2'] },
    layout: CIRCUIT,
    generate: { length: 2100, radius: [30, 110], corners: [8, 12], clearance: 64 },
  },
  {
    value: 'oval',
    label: 'Speedway',
    about: 'A 3 km oval with turns banked at 26°. Built for 500 mph.',
    theme: { ...DAY, banner: 'SPEEDWAY' },
    style: { width: 24, bank: [5, 26], kerbs: false, outsideRunoff: 0.6, insideRunoff: null, apron: 10, wallHeight: 1.2, fence: 5, wallColors: ['#f4f4f4', '#2b6fd6'] },
    layout: OVAL,
    generate: { length: 2700, radius: [150, 320], corners: [4, 6], leftOnly: true, clearance: 120 },
  },
  {
    value: 'canyon',
    label: 'Red Rock Canyon',
    about: 'A long, fast desert track between towering red mesas. Sand everywhere off the road.',
    theme: {
      sky: ['#3d8fd6', '#f6d7a8'], fog: 0xf0cf9e, ground: 'sand', hills: 0xb5552b, trees: 'cactus',
      night: false, weather: 'clear', grip: 1, scenery: 'mesas', banner: 'CANYON RUN',
    },
    style: { width: 14, bank: [0, 4], kerbs: true, outsideRunoff: 7, insideRunoff: 7, apron: 0, wallHeight: 1.2, fence: 0, wallColors: ['#a0522d', '#e8c39e'] },
    generate: { length: 2600, radius: [28, 140], corners: [9, 13], clearance: 56 },
    seed: 3,
  },
  {
    value: 'alpine',
    label: 'Alpine Pass',
    about: 'Snowy mountain roads in falling snow. The road is slippery, so brake early.',
    theme: {
      sky: ['#8fb4d9', '#e9f1f8'], fog: 0xe4edf5, ground: 'snow', hills: 0xdfe7ef, trees: 'snowPine',
      night: false, weather: 'snow', grip: 0.8, scenery: 'mountains', banner: 'ALPINE PASS',
    },
    style: { width: 13, bank: [0, 6], kerbs: true, outsideRunoff: 6, insideRunoff: 6, apron: 0, wallHeight: 1, fence: 0, wallColors: ['#2b6fd6', '#f2f2f2'] },
    generate: { length: 2200, radius: [25, 100], corners: [10, 14], clearance: 55 },
    seed: 6,
  },
  {
    value: 'neon',
    label: 'Neon Night',
    about: 'A tight street circuit through a city that never sleeps. Concrete walls, no room for mistakes.',
    theme: {
      sky: ['#0a0f24', '#2a1a48'], fog: 0x1a1430, ground: 'dirt', hills: 0x1b1d2b, trees: 'round',
      night: true, weather: 'clear', grip: 1, scenery: 'city', banner: 'NEON NIGHT',
    },
    style: { width: 14, bank: [0, 0], kerbs: true, outsideRunoff: 2.5, insideRunoff: 2.5, apron: 0, wallHeight: 1.3, fence: 2.5, wallColors: ['#ff3bd0', '#2b2e33'] },
    generate: { length: 1900, radius: [18, 60], corners: [10, 14], clearance: 52 },
    seed: 7,
  },
  {
    value: 'forest',
    label: 'Forest Rally',
    about: 'A narrow rally stage through the woods, in the rain. No barriers, just trees.',
    theme: {
      sky: ['#6f7f8c', '#b9c4cc'], fog: 0xaab6bf, ground: 'grass', hills: 0x3d5a35, trees: 'pine',
      night: false, weather: 'rain', grip: 0.85, scenery: 'woods', banner: 'FOREST RALLY',
    },
    style: { width: 11, bank: [0, 3], kerbs: false, outsideRunoff: 0, insideRunoff: 0, apron: 0, wallHeight: 1, fence: 0, wallColors: ['#6b4a2f', '#d9c9a3'], noWalls: true },
    generate: { length: 2100, radius: [22, 90], corners: [10, 15], clearance: 50 },
    seed: 5,
  },
  {
    value: 'kart',
    label: 'Kart Park',
    about: 'A short, tight, twisty track with tyre walls. Corners come thick and fast.',
    theme: { ...DAY, banner: 'KART PARK' },
    style: { width: 9, bank: [0, 0], kerbs: true, outsideRunoff: 3, insideRunoff: 3, apron: 0, wallHeight: 0.8, fence: 0, wallColors: ['#1a1a1a', '#f2f2f2'] },
    generate: { length: 950, radius: [12, 35], corners: [10, 16], clearance: 34 },
    seed: 3,
  },
];

/** The venue with this name. */
export function venue(map: MapName): Venue {
  return VENUES.find((v) => v.value === map) ?? VENUES[0];
}

/** Every map name, for checking saved settings. */
export const MAP_NAMES = VENUES.map((v) => v.value);

/**
 * The track for a venue's layout number: its main track for 0, otherwise a
 * generated one. Every venue and layout number gives a different seed.
 */
export function trackLayout(v: Venue, layout: number): TrackStep[] {
  if (layout === 0 && v.layout) return v.layout;
  const venueIndex = VENUES.indexOf(v);
  return generateLayout(layout === 0 ? (v.seed ?? 1) : 1000 + layout * 31 + venueIndex * 100003, v.generate!);
}
