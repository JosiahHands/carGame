// The people of the city: who they are, how they look, and what they start
// out like. The main cast is written here; the other residents are made in
// residents.ts. Just data (no 3D, no browser), because the main process uses
// it too, to check who the AI is asked about.
//
// They don't stay like this: their traits drift with what happens to them,
// they pick up habits, change their minds about each other (and about you),
// and learn which places they enjoy. See mind.ts.

import { makeResidents } from './residents';

/** Five sides of a personality, each 0..1. */
export interface Traits {
  kindness: number;
  courage: number;
  curiosity: number;
  humour: number;
  energy: number;
}
export const TRAIT_NAMES = ['kindness', 'courage', 'curiosity', 'humour', 'energy'] as const;
export type TraitName = (typeof TRAIT_NAMES)[number];
/** What each trait is called on screen. The names above are kept as they are: saves and the AI's answers use them. */
export const TRAIT_LABELS: Record<TraitName, string> = { kindness: 'kindness', courage: 'courage', curiosity: 'curiosity', humour: 'humor', energy: 'energy' };

/** Kinds of place in the city (see city.ts for where they are). */
export const PLACE_KINDS = ['coffee', 'bank', 'diner', 'gym', 'music', 'news', 'garage', 'park', 'stunts', 'bakery', 'library', 'office', 'market'] as const;
export type PlaceKind = (typeof PLACE_KINDS)[number];

export interface Look {
  skin: number;
  hair: number;
  hairStyle: 'short' | 'long' | 'curly' | 'bun' | 'bald' | 'spiky';
  shirt: number;
  trousers: number;
  shoes: number;
  hat: 'none' | 'police' | 'beanie' | 'cap';
  glasses: boolean;
  /** 1 is average; a little broader or slimmer. */
  build: number;
  /** Height in meters. */
  height: number;
  /** Something they carry or wear that says who they are. */
  extra: 'none' | 'badge' | 'apron' | 'guitar' | 'camera' | 'skateboard' | 'tie' | 'paint' | 'wrench' | 'headband' | 'newspaper' | 'spatula';
}

export interface CastMember {
  id: string;
  /** Shown over their head. */
  name: string;
  role: string;
  /** Who they are, for the AI and the journal. */
  bio: string;
  /** How they talk. */
  voice: string;
  traits: Traits;
  look: Look;
  /** Where they work, if anywhere. */
  job: PlaceKind | null;
  /** Places they like to spend time. */
  likes: PlaceKind[];
  /** Someone they go everywhere with. */
  bond?: string;
  /** People they already know and like. */
  friends: string[];
  /** One of the main cast (written by hand), rather than a resident. */
  star?: boolean;
}

/** The main cast. */
const STARS: CastMember[] = [
  {
    id: 'guy',
    name: 'GUY',
    role: 'bank teller',
    bio:
      'Works the counter at the city bank. Wakes up cheerful every single day, greets everyone by name and believes every day can be the best one yet. ' +
      'Lately he has started to wonder whether there is more to life than his routine, and he is fascinated by the drivers who tear round the city.',
    voice: 'warm, upbeat and earnest; easily amazed; sees the good in everyone',
    traits: { kindness: 0.95, courage: 0.5, curiosity: 0.8, humour: 0.65, energy: 0.85 },
    look: { skin: 0xf1c9a5, hair: 0x5a3a22, hairStyle: 'short', shirt: 0x2f74d0, trousers: 0xb9a57d, shoes: 0x5b4636, hat: 'none', glasses: false, build: 1, height: 1.82, extra: 'none' },
    job: 'bank',
    likes: ['coffee', 'park'],
    friends: ['buddy', 'tobias'],
  },
  {
    id: 'buddy',
    name: 'BUDDY',
    role: 'police officer',
    bio:
      "City police officer and Guy's best friend since forever. Easy-going and loyal to a fault: wherever Guy goes, Buddy goes too. " +
      'Takes traffic safety very seriously and hands out tickets to reckless drivers, but would never let anything happen to Guy.',
    voice: 'laid-back, dry and loyal; a little gruff with drivers, soft with friends',
    traits: { kindness: 0.8, courage: 0.75, curiosity: 0.4, humour: 0.6, energy: 0.55 },
    look: { skin: 0x8d5524, hair: 0x1b1b1b, hairStyle: 'short', shirt: 0x1f2d4d, trousers: 0x18223a, shoes: 0x111111, hat: 'police', glasses: true, build: 1.12, height: 1.86, extra: 'badge' },
    job: null,
    likes: ['coffee', 'diner'],
    bond: 'guy',
    friends: ['guy', 'rosa'],
  },
  {
    id: 'tobias',
    name: 'TOBIAS',
    role: 'coffee cart owner',
    bio: 'Runs the coffee cart and remembers everyone\'s order. Hears every rumor in the city first, and passes it on with extra sugar.',
    voice: 'chatty, gossipy, theatrical',
    traits: { kindness: 0.7, courage: 0.35, curiosity: 0.9, humour: 0.8, energy: 0.7 },
    look: { skin: 0xe0ac69, hair: 0x2b1b10, hairStyle: 'curly', shirt: 0xf5f0e6, trousers: 0x3a3a3a, shoes: 0x222222, hat: 'none', glasses: true, build: 0.95, height: 1.74, extra: 'apron' },
    job: 'coffee',
    likes: ['news', 'diner'],
    friends: ['guy', 'walt'],
  },
  {
    id: 'maya',
    name: 'MAYA',
    role: 'street artist',
    bio: 'Paints murals on the sides of buildings at night and dreams of a city with more color in it. Thinks the fast cars are moving art.',
    voice: 'dreamy, poetic, rebellious',
    traits: { kindness: 0.7, courage: 0.7, curiosity: 0.85, humour: 0.5, energy: 0.6 },
    look: { skin: 0xc68642, hair: 0x7b2cbf, hairStyle: 'long', shirt: 0x2ec4b6, trousers: 0x1d3557, shoes: 0xf2f2f2, hat: 'beanie', glasses: false, build: 0.9, height: 1.66, extra: 'paint' },
    job: null,
    likes: ['park', 'music', 'stunts'],
    friends: ['jules', 'kenji'],
  },
  {
    id: 'priya',
    name: 'PRIYA',
    role: 'fitness trainer',
    bio: 'Runs the gym and jogs round the blocks every morning. Competitive about everything, including how fast other people drive.',
    voice: 'brisk, motivational, competitive',
    traits: { kindness: 0.6, courage: 0.8, curiosity: 0.5, humour: 0.45, energy: 0.98 },
    look: { skin: 0xa86b3c, hair: 0x111111, hairStyle: 'bun', shirt: 0xff4d6d, trousers: 0x222222, shoes: 0xffffff, hat: 'none', glasses: false, build: 0.95, height: 1.7, extra: 'headband' },
    job: 'gym',
    likes: ['park', 'coffee'],
    friends: ['dex'],
  },
  {
    id: 'walt',
    name: 'WALT',
    role: 'retired bus driver',
    bio: 'Drove the number 9 bus for forty years. Sits on the same park bench feeding pigeons and complaining that nobody signals any more. Secretly loves a good jump.',
    voice: 'grumbly, nostalgic, secretly sweet',
    traits: { kindness: 0.65, courage: 0.4, curiosity: 0.45, humour: 0.7, energy: 0.25 },
    look: { skin: 0xffdbac, hair: 0xd9d9d9, hairStyle: 'bald', shirt: 0x6b4f3a, trousers: 0x55595f, shoes: 0x3a2a1a, hat: 'cap', glasses: true, build: 1.05, height: 1.72, extra: 'newspaper' },
    job: null,
    likes: ['park', 'news', 'diner'],
    friends: ['tobias', 'rosa'],
  },
  {
    id: 'dex',
    name: 'DEX',
    role: 'skater',
    bio: 'Lives at the stunt park on a skateboard. Rates every jump out of ten and wants to be famous on the internet.',
    voice: 'hyped, slangy, always rating things',
    traits: { kindness: 0.55, courage: 0.95, curiosity: 0.7, humour: 0.85, energy: 0.95 },
    look: { skin: 0xf1c27d, hair: 0xe8c547, hairStyle: 'spiky', shirt: 0xf46036, trousers: 0x4a5d23, shoes: 0x111111, hat: 'cap', glasses: false, build: 0.9, height: 1.76, extra: 'skateboard' },
    job: null,
    likes: ['stunts', 'park', 'music'],
    friends: ['priya', 'kenji'],
  },
  {
    id: 'rosa',
    name: 'ROSA',
    role: 'diner cook',
    bio: 'Cooks at the diner and feeds half the city. Mothers everyone, Buddy included, and has strong opinions about loud engines at breakfast time.',
    voice: 'warm, bossy, motherly',
    traits: { kindness: 0.9, courage: 0.6, curiosity: 0.5, humour: 0.7, energy: 0.65 },
    look: { skin: 0xd8a47f, hair: 0x3b2314, hairStyle: 'bun', shirt: 0xd7263d, trousers: 0x2b3a55, shoes: 0x222222, hat: 'none', glasses: false, build: 1.1, height: 1.62, extra: 'spatula' },
    job: 'diner',
    likes: ['coffee', 'park'],
    friends: ['buddy', 'walt'],
  },
  {
    id: 'kenji',
    name: 'KENJI',
    role: 'photographer',
    bio: 'Freelance photographer hunting the perfect shot of the city. Follows the action, especially big jumps and close calls.',
    voice: 'observant, quietly funny, describes things like photos',
    traits: { kindness: 0.6, courage: 0.55, curiosity: 0.95, humour: 0.6, energy: 0.6 },
    look: { skin: 0xf0c8a0, hair: 0x111111, hairStyle: 'short', shirt: 0x3bb273, trousers: 0x2b2b2b, shoes: 0x6b4226, hat: 'none', glasses: true, build: 0.95, height: 1.75, extra: 'camera' },
    job: null,
    likes: ['stunts', 'park', 'music'],
    friends: ['maya', 'dex'],
  },
  {
    id: 'lena',
    name: 'LENA',
    role: 'mechanic',
    bio: 'Runs the garage in the parking lot and can tell what is wrong with a car from its sound. Judges every car that drives past.',
    voice: 'blunt, technical, proud of good engineering',
    traits: { kindness: 0.55, courage: 0.7, curiosity: 0.75, humour: 0.5, energy: 0.7 },
    look: { skin: 0xffdbac, hair: 0xb5651d, hairStyle: 'long', shirt: 0x3a5a78, trousers: 0x3a5a78, shoes: 0x2b2b2b, hat: 'cap', glasses: false, build: 1, height: 1.69, extra: 'wrench' },
    job: 'garage',
    likes: ['diner', 'stunts'],
    friends: ['rosa'],
  },
  {
    id: 'marcus',
    name: 'MARCUS',
    role: 'bank manager',
    bio: "Guy's boss at the bank. Always in a hurry, always on the phone, and worried about everything. Wants to relax, has forgotten how.",
    voice: 'stressed, formal, clipped',
    traits: { kindness: 0.4, courage: 0.3, curiosity: 0.35, humour: 0.3, energy: 0.75 },
    look: { skin: 0x6b4226, hair: 0x1b1b1b, hairStyle: 'short', shirt: 0xffffff, trousers: 0x1b1f2a, shoes: 0x111111, hat: 'none', glasses: false, build: 1.05, height: 1.8, extra: 'tie' },
    job: 'bank',
    likes: ['coffee', 'news'],
    friends: [],
  },
  {
    id: 'jules',
    name: 'JULES',
    role: 'street musician',
    bio: 'Plays guitar on the corner for spare change and writes songs about the people walking past. Wants to write the song of the city.',
    voice: 'gentle, playful, sometimes rhymes',
    traits: { kindness: 0.85, courage: 0.45, curiosity: 0.75, humour: 0.75, energy: 0.5 },
    look: { skin: 0xc68642, hair: 0x2b1b10, hairStyle: 'curly', shirt: 0xffd23f, trousers: 0x6b4f3a, shoes: 0x3a2a1a, hat: 'beanie', glasses: false, build: 0.95, height: 1.78, extra: 'guitar' },
    job: 'music',
    likes: ['park', 'coffee'],
    friends: ['maya'],
  },
];

/** Everyone in the city: the main cast, then the residents. */
export const CAST: CastMember[] = [
  ...STARS.map((member) => ({ ...member, star: true })),
  ...makeResidents(STARS.map((member) => member.name)),
];

const BY_ID = new Map(CAST.map((member) => [member.id, member]));

export function castMember(id: string): CastMember | undefined {
  return BY_ID.get(id);
}

/** Feelings a line of speech can carry (shown as the mood face, and in the journal). */
export const EMOTIONS = ['happy', 'excited', 'calm', 'curious', 'amused', 'worried', 'scared', 'annoyed', 'sad', 'proud'] as const;
export type Emotion = (typeof EMOTIONS)[number];
