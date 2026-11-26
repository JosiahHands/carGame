//! The people of the city: who they are, how they look, and what they start
//! out like. The main cast is written here; the other residents are made from
//! a fixed seed, so every copy of the game gets the same people (and saved
//! minds from 1.2.x still belong to the right person).
//!
//! They don't stay like this: their traits drift with what happens to them,
//! they pick up habits, change their minds about each other (and about you),
//! and learn which places they enjoy. See mind.rs.
use std::sync::OnceLock;

use game::rng::Seeded;
use serde::{Deserialize, Serialize};

/// Five sides of a personality, each 0..1.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Traits {
    pub kindness: f64,
    pub courage: f64,
    pub curiosity: f64,
    pub humor: f64,
    pub energy: f64,
}

impl Default for Traits {
    fn default() -> Self {
        Traits {
            kindness: 0.5,
            courage: 0.5,
            curiosity: 0.5,
            humor: 0.5,
            energy: 0.5,
        }
    }
}

pub const TRAIT_NAMES: [&str; 5] = ["kindness", "courage", "curiosity", "humor", "energy"];

impl Traits {
    pub fn get(&self, name: &str) -> f64 {
        match name {
            "kindness" => self.kindness,
            "courage" => self.courage,
            "curiosity" => self.curiosity,
            "humor" => self.humor,
            _ => self.energy,
        }
    }

    pub fn get_mut(&mut self, name: &str) -> &mut f64 {
        match name {
            "kindness" => &mut self.kindness,
            "courage" => &mut self.courage,
            "curiosity" => &mut self.curiosity,
            "humor" => &mut self.humor,
            _ => &mut self.energy,
        }
    }
}

/// Kinds of place in the city (see city.rs for where they are).
pub const PLACE_KINDS: [&str; 13] = [
    "coffee", "bank", "diner", "gym", "music", "news", "garage", "park", "stunts", "bakery", "library", "office", "market",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HairStyle {
    Short,
    Long,
    Curly,
    Bun,
    Bald,
    Spiky,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hat {
    None,
    Police,
    Beanie,
    Cap,
}

/// Something they carry or wear that says who they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extra {
    None,
    Badge,
    Apron,
    Guitar,
    Camera,
    Skateboard,
    Tie,
    Paint,
    Wrench,
    Headband,
    Newspaper,
    Spatula,
}

#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub skin: u32,
    pub hair: u32,
    pub hair_style: HairStyle,
    pub shirt: u32,
    pub trousers: u32,
    pub shoes: u32,
    pub hat: Hat,
    pub glasses: bool,
    /// 1 is average; a little broader or slimmer.
    pub build: f64,
    /// Height in meters.
    pub height: f64,
    pub extra: Extra,
}

#[derive(Clone, Debug)]
pub struct CastMember {
    pub id: String,
    /// Shown over their head.
    pub name: String,
    pub role: String,
    /// Who they are, for the AI and the journal.
    pub bio: String,
    /// How they talk.
    pub voice: String,
    pub traits: Traits,
    pub look: Look,
    /// Where they work, if anywhere.
    pub job: Option<&'static str>,
    /// Places they like to spend time.
    pub likes: Vec<&'static str>,
    /// Someone they go everywhere with.
    pub bond: Option<&'static str>,
    /// People they already know and like.
    pub friends: Vec<String>,
    /// One of the main cast (written by hand), rather than a resident.
    pub star: bool,
}

/// Feelings a line of speech can carry (shown as the face on their name tag, and in the journal).
pub const EMOTIONS: [&str; 10] = [
    "happy", "excited", "calm", "curious", "amused", "worried", "scared", "annoyed", "sad", "proud",
];

fn star(
    id: &str,
    role: &str,
    bio: &str,
    voice: &str,
    t: [f64; 5],
    look: Look,
    job: Option<&'static str>,
    likes: &[&'static str],
    bond: Option<&'static str>,
    friends: &[&str],
) -> CastMember {
    CastMember {
        id: id.to_string(),
        name: id.to_uppercase(),
        role: role.to_string(),
        bio: bio.to_string(),
        voice: voice.to_string(),
        traits: Traits {
            kindness: t[0],
            courage: t[1],
            curiosity: t[2],
            humor: t[3],
            energy: t[4],
        },
        look,
        job,
        likes: likes.to_vec(),
        bond,
        friends: friends.iter().map(|s| s.to_string()).collect(),
        star: true,
    }
}

#[allow(clippy::too_many_arguments)]
fn look(
    skin: u32,
    hair: u32,
    style: HairStyle,
    shirt: u32,
    trousers: u32,
    shoes: u32,
    hat: Hat,
    glasses: bool,
    build: f64,
    height: f64,
    extra: Extra,
) -> Look {
    Look {
        skin,
        hair,
        hair_style: style,
        shirt,
        trousers,
        shoes,
        hat,
        glasses,
        build,
        height,
        extra,
    }
}

fn stars() -> Vec<CastMember> {
    use Extra as X;
    use HairStyle as S;
    vec![
        star(
            "guy",
            "bank teller",
            "Works the counter at the city bank. Wakes up cheerful every single day, greets everyone by name and believes every day can be the best one yet. Lately he has started to wonder whether there is more to life than his routine, and he is fascinated by the drivers who tear around the city.",
            "warm, upbeat and earnest; easily amazed; sees the good in everyone",
            [0.95, 0.5, 0.8, 0.65, 0.85],
            look(
                0xf1c9a5,
                0x5a3a22,
                S::Short,
                0x2f74d0,
                0xb9a57d,
                0x5b4636,
                Hat::None,
                false,
                1.0,
                1.82,
                X::None,
            ),
            Some("bank"),
            &["coffee", "park"],
            None,
            &["buddy", "tobias"],
        ),
        star(
            "buddy",
            "police officer",
            "City police officer and Guy's best friend since forever. Easy-going and loyal to a fault: wherever Guy goes, Buddy goes too. Takes traffic safety very seriously and hands out tickets to reckless drivers, but would never let anything happen to Guy.",
            "laid-back, dry and loyal; a little gruff with drivers, soft with friends",
            [0.8, 0.75, 0.4, 0.6, 0.55],
            look(
                0x8d5524,
                0x1b1b1b,
                S::Short,
                0x1f2d4d,
                0x18223a,
                0x111111,
                Hat::Police,
                true,
                1.12,
                1.86,
                X::Badge,
            ),
            None,
            &["coffee", "diner"],
            Some("guy"),
            &["guy", "rosa"],
        ),
        star(
            "tobias",
            "coffee cart owner",
            "Runs the coffee cart and remembers everyone's order. Hears every rumor in the city first, and passes it on with extra sugar.",
            "chatty, gossipy, theatrical",
            [0.7, 0.35, 0.9, 0.8, 0.7],
            look(
                0xe0ac69,
                0x2b1b10,
                S::Curly,
                0xf5f0e6,
                0x3a3a3a,
                0x222222,
                Hat::None,
                true,
                0.95,
                1.74,
                X::Apron,
            ),
            Some("coffee"),
            &["news", "diner"],
            None,
            &["guy", "walt"],
        ),
        star(
            "maya",
            "street artist",
            "Paints murals on the sides of buildings at night and dreams of a city with more color in it. Thinks the fast cars are moving art.",
            "dreamy, poetic, rebellious",
            [0.7, 0.7, 0.85, 0.5, 0.6],
            look(
                0xc68642,
                0x7b2cbf,
                S::Long,
                0x2ec4b6,
                0x1d3557,
                0xf2f2f2,
                Hat::Beanie,
                false,
                0.9,
                1.66,
                X::Paint,
            ),
            None,
            &["park", "music", "stunts"],
            None,
            &["jules", "kenji"],
        ),
        star(
            "priya",
            "fitness trainer",
            "Runs the gym and jogs around the blocks every morning. Competitive about everything, including how fast other people drive.",
            "brisk, motivational, competitive",
            [0.6, 0.8, 0.5, 0.45, 0.98],
            look(
                0xa86b3c,
                0x111111,
                S::Bun,
                0xff4d6d,
                0x222222,
                0xffffff,
                Hat::None,
                false,
                0.95,
                1.7,
                X::Headband,
            ),
            Some("gym"),
            &["park", "coffee"],
            None,
            &["dex"],
        ),
        star(
            "walt",
            "retired bus driver",
            "Drove the number 9 bus for forty years. Sits on the same park bench feeding pigeons and complaining that nobody signals any more. Secretly loves a good jump.",
            "grumbly, nostalgic, secretly sweet",
            [0.65, 0.4, 0.45, 0.7, 0.25],
            look(
                0xffdbac,
                0xd9d9d9,
                S::Bald,
                0x6b4f3a,
                0x55595f,
                0x3a2a1a,
                Hat::Cap,
                true,
                1.05,
                1.72,
                X::Newspaper,
            ),
            None,
            &["park", "news", "diner"],
            None,
            &["tobias", "rosa"],
        ),
        star(
            "dex",
            "skater",
            "Lives at the stunt park on a skateboard. Rates every jump out of ten and wants to be famous on the internet.",
            "hyped, slangy, always rating things",
            [0.55, 0.95, 0.7, 0.85, 0.95],
            look(
                0xf1c27d,
                0xe8c547,
                S::Spiky,
                0xf46036,
                0x4a5d23,
                0x111111,
                Hat::Cap,
                false,
                0.9,
                1.76,
                X::Skateboard,
            ),
            None,
            &["stunts", "park", "music"],
            None,
            &["priya", "kenji"],
        ),
        star(
            "rosa",
            "diner cook",
            "Cooks at the diner and feeds half the city. Mothers everyone, Buddy included, and has strong opinions about loud engines at breakfast time.",
            "warm, bossy, motherly",
            [0.9, 0.6, 0.5, 0.7, 0.65],
            look(
                0xd8a47f,
                0x3b2314,
                S::Bun,
                0xd7263d,
                0x2b3a55,
                0x222222,
                Hat::None,
                false,
                1.1,
                1.62,
                X::Spatula,
            ),
            Some("diner"),
            &["coffee", "park"],
            None,
            &["buddy", "walt"],
        ),
        star(
            "kenji",
            "photographer",
            "Freelance photographer hunting the perfect shot of the city. Follows the action, especially big jumps and close calls.",
            "observant, quietly funny, describes things like photos",
            [0.6, 0.55, 0.95, 0.6, 0.6],
            look(
                0xf0c8a0,
                0x111111,
                S::Short,
                0x3bb273,
                0x2b2b2b,
                0x6b4226,
                Hat::None,
                true,
                0.95,
                1.75,
                X::Camera,
            ),
            None,
            &["stunts", "park", "music"],
            None,
            &["maya", "dex"],
        ),
        star(
            "lena",
            "mechanic",
            "Runs the garage in the parking lot and can tell what is wrong with a car from its sound. Judges every car that drives past.",
            "blunt, technical, proud of good engineering",
            [0.55, 0.7, 0.75, 0.5, 0.7],
            look(
                0xffdbac,
                0xb5651d,
                S::Long,
                0x3a5a78,
                0x3a5a78,
                0x2b2b2b,
                Hat::Cap,
                false,
                1.0,
                1.69,
                X::Wrench,
            ),
            Some("garage"),
            &["diner", "stunts"],
            None,
            &["rosa"],
        ),
        star(
            "marcus",
            "bank manager",
            "Guy's boss at the bank. Always in a hurry, always on the phone, and worried about everything. Wants to relax, has forgotten how.",
            "stressed, formal, clipped",
            [0.4, 0.3, 0.35, 0.3, 0.75],
            look(
                0x6b4226,
                0x1b1b1b,
                S::Short,
                0xffffff,
                0x1b1f2a,
                0x111111,
                Hat::None,
                false,
                1.05,
                1.8,
                X::Tie,
            ),
            Some("bank"),
            &["coffee", "news"],
            None,
            &[],
        ),
        star(
            "jules",
            "street musician",
            "Plays guitar on the corner for spare change and writes songs about the people walking past. Wants to write the song of the city.",
            "gentle, playful, sometimes rhymes",
            [0.85, 0.45, 0.75, 0.75, 0.5],
            look(
                0xc68642,
                0x2b1b10,
                S::Curly,
                0xffd23f,
                0x6b4f3a,
                0x3a2a1a,
                Hat::Beanie,
                false,
                0.95,
                1.78,
                X::Guitar,
            ),
            Some("music"),
            &["park", "coffee"],
            None,
            &["maya"],
        ),
    ]
}

/// How many residents live in the city besides the main cast.
pub const RESIDENTS: usize = 36;

const FIRST_NAMES: [&str; 48] = [
    "AVA", "NOAH", "ZARA", "OMAR", "IVY", "LUCA", "NIA", "FELIX", "MEI", "SAM", "RUBY", "THEO", "ANYA", "DIEGO", "HANA", "ELI", "GRACE",
    "KOFI", "LILA", "MATEO", "NORA", "OSCAR", "PIA", "RAJ", "SOFIA", "TOMAS", "UMA", "VIC", "WREN", "YUSUF", "ZOE", "BEA", "CLEO",
    "DARIUS", "EDIE", "FINN", "GUS", "HUGO", "INES", "JADE", "KAI", "LOU", "MILO", "NED", "OLGA", "PERCY", "QUINN", "REX",
];

/// Jobs at each kind of place, and what people with no job there do with their days.
const JOBS: [(Option<&str>, &[&str], Extra); 12] = [
    (Some("coffee"), &["barista"], Extra::Apron),
    (Some("bank"), &["bank clerk", "loan officer"], Extra::Tie),
    (Some("diner"), &["waiter", "dishwasher", "line cook"], Extra::Apron),
    (Some("gym"), &["yoga teacher", "boxing coach"], Extra::Headband),
    (Some("news"), &["local reporter"], Extra::Newspaper),
    (Some("garage"), &["tire fitter", "apprentice mechanic"], Extra::Wrench),
    (Some("bakery"), &["baker", "pastry chef"], Extra::Apron),
    (Some("library"), &["librarian", "archivist"], Extra::None),
    (
        Some("office"),
        &[
            "accountant",
            "architect",
            "web designer",
            "insurance agent",
            "lawyer",
            "office manager",
        ],
        Extra::Tie,
    ),
    (Some("market"), &["fruit seller", "flower seller", "cheese maker"], Extra::None),
    (Some("park"), &["park gardener", "dog walker"], Extra::None),
    (
        None,
        &[
            "student",
            "retired teacher",
            "novelist",
            "night nurse",
            "delivery rider",
            "out-of-work actor",
            "inventor",
            "painter",
        ],
        Extra::None,
    ),
];

const HOBBIES: [&str; 18] = [
    "collects old postcards of the city",
    "is training for a marathon",
    "grows tomatoes on a tiny balcony",
    "writes poems nobody has read",
    "feeds the pigeons every day",
    "is learning the trumpet badly",
    "builds model cars",
    "does crosswords in pen",
    "bakes for the neighbors",
    "is obsessed with weather forecasts",
    "keeps a diary of every car that drives past",
    "plays chess against strangers",
    "dances when no one is looking",
    "restores old bicycles",
    "knows every bird in the park",
    "tells terrible jokes on purpose",
    "reads three books a week",
    "paints tiny portraits",
];

const QUIRKS: [&str; 16] = [
    "Always early, never on time for anything fun.",
    "Hums when nervous.",
    "Remembers every birthday in the city.",
    "Cannot keep a secret.",
    "Talks to plants.",
    "Gives everyone a nickname.",
    "Apologizes to furniture.",
    "Has a different hat for every mood.",
    "Hates loud engines.",
    "Loves loud engines.",
    "Believes in lucky numbers.",
    "Overthinks every decision.",
    "Laughs at their own jokes first.",
    "Never forgets a favor.",
    "Quotes their grandmother constantly.",
    "Is suspicious of anyone in a hurry.",
];

const DREAMS: [&str; 14] = [
    "opening a little shop of their own",
    "seeing the ocean",
    "being famous for one day",
    "finding true love",
    "learning to drive",
    "writing a book about the city",
    "winning the city bake-off",
    "traveling the world",
    "getting a dog",
    "becoming mayor",
    "fixing up an old convertible",
    "playing on a real stage",
    "making up with an old friend",
    "a quiet life with no surprises",
];

const VOICES: [&str; 12] = [
    "chatty and warm",
    "dry and sarcastic",
    "shy, short sentences",
    "loud and enthusiastic",
    "polite and formal",
    "dreamy and poetic",
    "nosy, asks lots of questions",
    "grumpy but secretly kind",
    "nervous, talks fast",
    "calm and wise",
    "cheeky and teasing",
    "earnest, a bit naive",
];

const LIKES: [&str; 10] = [
    "coffee", "diner", "park", "music", "news", "gym", "bakery", "library", "market", "stunts",
];
const SKINS: [u32; 8] = [0xffdbac, 0xf1c27d, 0xe0ac69, 0xc68642, 0x8d5524, 0x6b4226, 0xf5d0b0, 0xd8a47f];
const HAIRS: [u32; 8] = [0x1b1b1b, 0x3b2314, 0x5a3a22, 0xb5651d, 0xe8c547, 0xd9d9d9, 0x8b3a1a, 0x2b1b10];
const STYLES: [HairStyle; 6] = [
    HairStyle::Short,
    HairStyle::Long,
    HairStyle::Curly,
    HairStyle::Bun,
    HairStyle::Bald,
    HairStyle::Spiky,
];
const SHIRTS: [u32; 12] = [
    0xe63946, 0x2a9d8f, 0xe9c46a, 0xf4a261, 0x264653, 0x8338ec, 0x3a86ff, 0xff006e, 0x06d6a0, 0xffffff, 0x6b705c, 0xb5838d,
];
const TROUSERS: [u32; 7] = [0x22223b, 0x4a4e69, 0x3d405b, 0x6b4f3a, 0x2b2d42, 0x8d99ae, 0x5c4033];

/// Makes the residents, exactly as 1.2.x did (the same seed and the same
/// order of random numbers). `taken` are the main cast's names.
fn make_residents(taken: &[String]) -> Vec<CastMember> {
    let mut rng = Seeded::new(20261123.0);
    let names: Vec<&str> = FIRST_NAMES.iter().copied().filter(|n| !taken.iter().any(|t| t == n)).collect();
    let mut people: Vec<CastMember> = Vec::new();
    fn pick<T: Copy>(rng: &mut Seeded, list: &[T]) -> T {
        list[(rng.next() * list.len() as f64).floor() as usize]
    }
    for (i, name) in names.iter().enumerate().take(RESIDENTS) {
        let (job, roles, extra) = JOBS[i % JOBS.len()];
        let role = pick(&mut rng, roles);
        let mut trait_ = || ((0.1 + rng.next() * 0.85) * 100.0).round() / 100.0;
        let traits = Traits {
            kindness: trait_(),
            courage: trait_(),
            curiosity: trait_(),
            humor: trait_(),
            energy: trait_(),
        };
        let mut likes: Vec<&'static str> = Vec::new();
        for _ in 0..3 {
            let l = pick(&mut rng, &LIKES);
            if !likes.contains(&l) {
                likes.push(l);
            }
        }
        likes.retain(|k| Some(*k) != job);
        likes.truncate(3);
        let count = 1 + (rng.next() * 3.0).floor() as usize;
        let mut friends: Vec<String> = Vec::new();
        for _ in 0..count.min(3) {
            let f = if people.is_empty() {
                String::new()
            } else {
                let p = (rng.next() * people.len() as f64).floor() as usize;
                people[p].id.clone()
            };
            if !friends.contains(&f) {
                friends.push(f);
            }
        }
        friends.retain(|f| !f.is_empty());
        let hobby = pick(&mut rng, &HOBBIES);
        let quirk = pick(&mut rng, &QUIRKS);
        let dream = pick(&mut rng, &DREAMS);
        let skin = pick(&mut rng, &SKINS);
        let hair = pick(&mut rng, &HAIRS);
        let hair_style = pick(&mut rng, &STYLES);
        let shirt = pick(&mut rng, &SHIRTS);
        let trousers = pick(&mut rng, &TROUSERS);
        let shoes = if rng.next() < 0.5 { 0x1b1b1b } else { 0x5b4636 };
        let hat = if rng.next() < 0.15 {
            Hat::Beanie
        } else if rng.next() < 0.12 {
            Hat::Cap
        } else {
            Hat::None
        };
        let glasses = rng.next() < 0.25;
        let build = ((0.88 + rng.next() * 0.26) * 100.0).round() / 100.0;
        let height = ((1.55 + rng.next() * 0.35) * 100.0).round() / 100.0;
        let extra = if extra != Extra::None && rng.next() < 0.6 {
            extra
        } else {
            Extra::None
        };
        people.push(CastMember {
            id: name.to_lowercase(),
            name: name.to_string(),
            role: role.to_string(),
            bio: format!("A {role} who {hobby}. {quirk} Dreams of {dream}."),
            voice: pick(&mut rng, &VOICES).to_string(),
            traits,
            look: Look {
                skin,
                hair,
                hair_style,
                shirt,
                trousers,
                shoes,
                hat,
                glasses,
                build,
                height,
                extra,
            },
            job,
            likes: if likes.is_empty() { vec!["park"] } else { likes },
            bond: None,
            friends,
            star: false,
        });
    }
    people
}

/// Everyone in the city: the main cast, then the residents.
pub fn cast() -> &'static [CastMember] {
    static CAST: OnceLock<Vec<CastMember>> = OnceLock::new();
    CAST.get_or_init(|| {
        let mut all = stars();
        let names: Vec<String> = all.iter().map(|m| m.name.clone()).collect();
        all.extend(make_residents(&names));
        all
    })
}

pub fn member(id: &str) -> Option<&'static CastMember> {
    cast().iter().find(|m| m.id == id)
}

/// A cast member's name, or the id if there's nobody by that id.
pub fn name_of(id: &str) -> String {
    member(id).map_or_else(|| id.to_string(), |m| m.name.clone())
}

#[cfg(test)]
mod tests {
    #[test]
    fn residents_match_the_old_game() {
        let cast = super::cast();
        assert_eq!(cast.len(), 48);
        // Checked against 1.2.x (residents.ts run with node).
        let ava = super::member("ava").unwrap();
        assert_eq!(ava.role, "barista");
        assert_eq!((ava.traits.kindness, ava.traits.energy), (0.82, 0.52));
        assert_eq!(ava.likes, ["music", "bakery"]);
        assert_eq!(ava.voice, "loud and enthusiastic");
        assert_eq!(ava.look.height, 1.67);
        let zara = super::member("zara").unwrap();
        assert_eq!(zara.likes, ["coffee", "gym", "park"]);
        assert_eq!(zara.friends, ["noah"]);
        assert_eq!(zara.look.hat, super::Hat::Beanie);
        let finn = super::member("finn").unwrap();
        assert_eq!(finn.friends, ["uma", "pia"]);
        assert_eq!(finn.voice, "polite and formal");
        assert_eq!(finn.look.height, 1.8);
        assert!(cast.iter().all(|m| !m.likes.is_empty()));
    }
}
