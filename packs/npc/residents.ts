// The rest of the city: residents made from a fixed seed, so every copy of
// the game (and the main process, which checks the names it's sent) gets the
// same people. Each one has a job or a way of passing the day, a home, hobbies,
// a quirk, a dream, a way of talking and a personality of their own. Like the
// main cast, they change as they live (see mind.ts).
import type { CastMember, Look, PlaceKind, Traits } from './cast';

/** How many residents live in the city besides the main cast. */
export const RESIDENTS = 36;

const FIRST_NAMES = [
  'AVA', 'NOAH', 'ZARA', 'OMAR', 'IVY', 'LUCA', 'NIA', 'FELIX', 'MEI', 'SAM', 'RUBY', 'THEO', 'ANYA', 'DIEGO', 'HANA', 'ELI',
  'GRACE', 'KOFI', 'LILA', 'MATEO', 'NORA', 'OSCAR', 'PIA', 'RAJ', 'SOFIA', 'TOMAS', 'UMA', 'VIC', 'WREN', 'YUSUF', 'ZOE', 'BEA',
  'CLEO', 'DARIUS', 'EDIE', 'FINN', 'GUS', 'HUGO', 'INES', 'JADE', 'KAI', 'LOU', 'MILO', 'NED', 'OLGA', 'PERCY', 'QUINN', 'REX',
];

/** Jobs at each kind of place, and what people with no job there do with their days. */
const JOBS: { job: PlaceKind | null; roles: string[]; extra?: Look['extra'] }[] = [
  { job: 'coffee', roles: ['barista'], extra: 'apron' },
  { job: 'bank', roles: ['bank clerk', 'loan officer'], extra: 'tie' },
  { job: 'diner', roles: ['waiter', 'dishwasher', 'line cook'], extra: 'apron' },
  { job: 'gym', roles: ['yoga teacher', 'boxing coach'], extra: 'headband' },
  { job: 'news', roles: ['local reporter'], extra: 'newspaper' },
  { job: 'garage', roles: ['tire fitter', 'apprentice mechanic'], extra: 'wrench' },
  { job: 'bakery', roles: ['baker', 'pastry chef'], extra: 'apron' },
  { job: 'library', roles: ['librarian', 'archivist'] },
  { job: 'office', roles: ['accountant', 'architect', 'web designer', 'insurance agent', 'lawyer', 'office manager'], extra: 'tie' },
  { job: 'market', roles: ['fruit seller', 'flower seller', 'cheese maker'] },
  { job: 'park', roles: ['park gardener', 'dog walker'] },
  { job: null, roles: ['student', 'retired teacher', 'novelist', 'night nurse', 'delivery rider', 'out-of-work actor', 'inventor', 'painter'] },
];

const HOBBIES = [
  'collects old postcards of the city', 'is training for a marathon', 'grows tomatoes on a tiny balcony', 'writes poems nobody has read',
  'feeds the pigeons every day', 'is learning the trumpet badly', 'builds model cars', 'does crosswords in pen', 'bakes for the neighbors',
  'is obsessed with weather forecasts', 'keeps a diary of every car that drives past', 'plays chess against strangers', 'dances when no one is looking',
  'restores old bicycles', 'knows every bird in the park', 'tells terrible jokes on purpose', 'reads three books a week', 'paints tiny portraits',
];

const QUIRKS = [
  'Always early, never on time for anything fun.', 'Hums when nervous.', 'Remembers every birthday in the city.', 'Cannot keep a secret.',
  'Talks to plants.', 'Gives everyone a nickname.', 'Apologizes to furniture.', 'Has a different hat for every mood.', 'Hates loud engines.',
  'Loves loud engines.', 'Believes in lucky numbers.', 'Overthinks every decision.', 'Laughs at their own jokes first.', 'Never forgets a favor.',
  'Quotes their grandmother constantly.', 'Is suspicious of anyone in a hurry.',
];

const DREAMS = [
  'opening a little shop of their own', 'seeing the ocean', 'being famous for one day', 'finding true love', 'learning to drive',
  'writing a book about the city', 'winning the city bake-off', 'traveling the world', 'getting a dog', 'becoming mayor',
  'fixing up an old convertible', 'playing on a real stage', 'making up with an old friend', 'a quiet life with no surprises',
];

const VOICES = [
  'chatty and warm', 'dry and sarcastic', 'shy, short sentences', 'loud and enthusiastic', 'polite and formal', 'dreamy and poetic',
  'nosy, asks lots of questions', 'grumpy but secretly kind', 'nervous, talks fast', 'calm and wise', 'cheeky and teasing', 'earnest, a bit naive',
];

const LIKES: PlaceKind[] = ['coffee', 'diner', 'park', 'music', 'news', 'gym', 'bakery', 'library', 'market', 'stunts'];
const SKINS = [0xffdbac, 0xf1c27d, 0xe0ac69, 0xc68642, 0x8d5524, 0x6b4226, 0xf5d0b0, 0xd8a47f];
const HAIRS = [0x1b1b1b, 0x3b2314, 0x5a3a22, 0xb5651d, 0xe8c547, 0xd9d9d9, 0x8b3a1a, 0x2b1b10];
const STYLES: Look['hairStyle'][] = ['short', 'long', 'curly', 'bun', 'bald', 'spiky'];
const SHIRTS = [0xe63946, 0x2a9d8f, 0xe9c46a, 0xf4a261, 0x264653, 0x8338ec, 0x3a86ff, 0xff006e, 0x06d6a0, 0xffffff, 0x6b705c, 0xb5838d];
const TROUSERS = [0x22223b, 0x4a4e69, 0x3d405b, 0x6b4f3a, 0x2b2d42, 0x8d99ae, 0x5c4033];

/** A small seeded random generator (mulberry32), so the residents are the same everywhere. */
function seeded(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Makes the residents. `taken` are the main cast's names, which aren't used again. */
export function makeResidents(taken: string[]): CastMember[] {
  const random = seeded(20261123);
  const pick = <T,>(list: readonly T[]): T => list[Math.floor(random() * list.length)];
  const names = FIRST_NAMES.filter((n) => !taken.includes(n));
  const people: CastMember[] = [];
  for (let i = 0; i < RESIDENTS && i < names.length; i++) {
    const name = names[i];
    // Spread the jobs round the places, with a few people of no fixed job.
    const work = JOBS[i % JOBS.length];
    const role = pick(work.roles);
    const trait = () => Math.round((0.1 + random() * 0.85) * 100) / 100;
    const traits: Traits = { kindness: trait(), courage: trait(), curiosity: trait(), humour: trait(), energy: trait() };
    const likes = [...new Set([pick(LIKES), pick(LIKES), pick(LIKES)])].filter((k) => k !== work.job).slice(0, 3);
    const friends = [...new Set([0, 1, 2].slice(0, 1 + Math.floor(random() * 3)).map(() => people.length ? pick(people).id : ''))].filter(Boolean);
    const hobby = pick(HOBBIES);
    const quirk = pick(QUIRKS);
    const dream = pick(DREAMS);
    const look: Look = {
      skin: pick(SKINS),
      hair: pick(HAIRS),
      hairStyle: pick(STYLES),
      shirt: pick(SHIRTS),
      trousers: pick(TROUSERS),
      shoes: random() < 0.5 ? 0x1b1b1b : 0x5b4636,
      hat: random() < 0.15 ? 'beanie' : random() < 0.12 ? 'cap' : 'none',
      glasses: random() < 0.25,
      build: Math.round((0.88 + random() * 0.26) * 100) / 100,
      height: Math.round((1.55 + random() * 0.35) * 100) / 100,
      extra: work.extra && random() < 0.6 ? work.extra : 'none',
    };
    people.push({
      id: name.toLowerCase(),
      name,
      role,
      bio: `A ${role} who ${hobby}. ${quirk} Dreams of ${dream}.`,
      voice: pick(VOICES),
      traits,
      look,
      job: work.job,
      likes: likes.length ? likes : ['park'],
      friends,
    });
  }
  return people;
}
