//! Rewards on top of the career: driver XP and levels, the prize wheel you
//! spin at every new level, and trophies for milestones.
use crate::career::{CHASSIS, CareerSave, LEVELS, UPGRADES, prestige, upgrade_cap};
use crate::rng;
use crate::text::thousands;

/// How many gold wrenches and free-roam challenge stars there are.
pub const WRENCHES: usize = 68;
pub const ZONE_STARS: usize = 78;

pub fn bump(save: &mut CareerSave, key: &str, amount: f64) {
    *save.stats.entry(key.into()).or_insert(0.0) += amount;
}

pub fn record(save: &mut CareerSave, key: &str, value: f64) {
    let v = save.stats.entry(key.into()).or_insert(0.0);
    if value > *v {
        *v = value;
    }
}

pub fn xp_for_level(level: u32) -> f64 {
    5000.0 + 2500.0 * (level as f64 - 1.0)
}

/// The driver level for an amount of XP: (level, XP into it, XP it needs).
pub fn driver_level(xp: f64) -> (u32, f64, f64) {
    let mut level = 1;
    let mut left = xp;
    while left >= xp_for_level(level) {
        left -= xp_for_level(level);
        level += 1;
    }
    (level, left, xp_for_level(level))
}

/// Adds XP. Each new level earns a prize spin. Returns how many levels were gained.
pub fn add_xp(save: &mut CareerSave, xp: f64) -> u32 {
    let before = driver_level(save.xp).0;
    save.xp += xp.round().max(0.0);
    let gained = driver_level(save.xp).0 - before;
    save.spins += gained;
    gained
}

pub struct Prize {
    pub label: &'static str,
    pub color: &'static str,
    pub weight: f64,
    pub coins: f64,
    pub coin_rush: u32,
    pub upgrade: bool,
}

pub const PRIZES: [Prize; 8] = [
    Prize {
        label: "500",
        color: "#3a4256",
        weight: 22.0,
        coins: 500.0,
        coin_rush: 0,
        upgrade: false,
    },
    Prize {
        label: "Coin rush",
        color: "#7b2cbf",
        weight: 14.0,
        coins: 0.0,
        coin_rush: 1,
        upgrade: false,
    },
    Prize {
        label: "1,500",
        color: "#1b98e0",
        weight: 18.0,
        coins: 1500.0,
        coin_rush: 0,
        upgrade: false,
    },
    Prize {
        label: "Free upgrade",
        color: "#3bb273",
        weight: 12.0,
        coins: 0.0,
        coin_rush: 0,
        upgrade: true,
    },
    Prize {
        label: "3,000",
        color: "#f46036",
        weight: 12.0,
        coins: 3000.0,
        coin_rush: 0,
        upgrade: false,
    },
    Prize {
        label: "2× Coin rush",
        color: "#c62f9e",
        weight: 8.0,
        coins: 0.0,
        coin_rush: 2,
        upgrade: false,
    },
    Prize {
        label: "6,000",
        color: "#d7263d",
        weight: 8.0,
        coins: 6000.0,
        coin_rush: 0,
        upgrade: false,
    },
    Prize {
        label: "JACKPOT",
        color: "#d4af37",
        weight: 3.0,
        coins: 25000.0,
        coin_rush: 0,
        upgrade: false,
    },
];

/// Spins the wheel: returns which prize (an index into PRIZES) and what was won.
pub fn spin_wheel(save: &mut CareerSave) -> (usize, String) {
    let total: f64 = PRIZES.iter().map(|p| p.weight).sum();
    let mut roll = rng::random() * total;
    let mut index = 0;
    while index < PRIZES.len() - 1 && roll >= PRIZES[index].weight {
        roll -= PRIZES[index].weight;
        index += 1;
    }
    let prize = &PRIZES[index];
    save.spins = save.spins.saturating_sub(1);
    bump(save, "spins", 1.0);
    let text = if prize.coins > 0.0 {
        let coins = (prize.coins * prestige(save)).round();
        save.coins += coins;
        save.earned += coins;
        format!(
            "{}+💰 {}",
            if prize.label == "JACKPOT" { "JACKPOT! " } else { "" },
            thousands(coins)
        )
    } else if prize.coin_rush > 0 {
        save.coin_rush += prize.coin_rush;
        if prize.coin_rush == 1 {
            "Coin rush ×1: your next event pays double".to_string()
        } else {
            format!("Coin rush ×{0}: your next {0} events pay double", prize.coin_rush)
        }
    } else {
        let cap = upgrade_cap(save);
        let open: Vec<_> = UPGRADES
            .iter()
            .filter(|u| save.upgrades.get(u.0).copied().unwrap_or(0) < cap)
            .collect();
        if open.is_empty() {
            save.coins += 5000.0;
            "Everything is maxed: +💰 5,000 instead".to_string()
        } else {
            let u = open[rng::index(open.len())];
            let level = save.upgrades.entry(u.0.into()).or_insert(0);
            *level += 1;
            format!("Free upgrade: {} {} → level {}", u.3, u.1, level)
        }
    };
    (index, text)
}

pub struct Trophy {
    pub id: String,
    pub name: String,
    pub about: String,
    pub icon: String,
    pub reward: f64,
    pub value: fn(&CareerSave) -> f64,
    pub need: f64,
}

fn stat(save: &CareerSave, key: &str) -> f64 {
    save.stats.get(key).copied().unwrap_or(0.0)
}

fn tiers(
    out: &mut Vec<Trophy>,
    id: &str,
    name: &str,
    about: &dyn Fn(f64) -> String,
    icon: &str,
    value: fn(&CareerSave) -> f64,
    needs: &[f64],
    rewards: &[f64],
) {
    let medals = ["🎖", "🏅", "🏆"];
    for (i, need) in needs.iter().enumerate() {
        out.push(Trophy {
            id: format!("{id}{}", i + 1),
            name: if needs.len() > 1 {
                format!("{name} {}", ["I", "II", "III"][i])
            } else {
                name.into()
            },
            about: about(*need),
            icon: if needs.len() > 1 {
                format!("{}{icon}", medals[i])
            } else {
                icon.into()
            },
            reward: rewards[i],
            value,
            need: *need,
        });
    }
}

pub fn trophies() -> &'static [Trophy] {
    use std::sync::OnceLock;
    static LIST: OnceLock<Vec<Trophy>> = OnceLock::new();
    LIST.get_or_init(|| {
        let mut t = Vec::new();
        let plural = |n: f64, word: &str| if n == 1.0 { word.to_string() } else { format!("{word}s") };
        tiers(
            &mut t,
            "win",
            "Winner",
            &|n| format!("Win {} {}", n, plural(n, "race")),
            "🏁",
            |s| s.wins as f64,
            &[1.0, 10.0, 50.0],
            &[500.0, 2000.0, 8000.0],
        );
        tiers(
            &mut t,
            "near",
            "Close shave",
            &|n| format!("{n} near misses"),
            "😬",
            |s| stat(s, "nearMiss"),
            &[10.0, 100.0, 500.0],
            &[500.0, 2500.0, 8000.0],
        );
        tiers(
            &mut t,
            "takedown",
            "Wrecking crew",
            &|n| format!("{n} takedowns"),
            "💥",
            |s| stat(s, "takedown"),
            &[10.0, 100.0, 500.0],
            &[500.0, 2500.0, 8000.0],
        );
        tiers(
            &mut t,
            "overtake",
            "Coming through",
            &|n| format!("{n} overtakes"),
            "⏩",
            |s| stat(s, "overtake"),
            &[25.0, 250.0, 1000.0],
            &[500.0, 2500.0, 8000.0],
        );
        tiers(
            &mut t,
            "start",
            "Launch control",
            &|n| format!("{n} perfect starts"),
            "🚀",
            |s| stat(s, "perfectStart"),
            &[5.0, 25.0, 100.0],
            &[500.0, 2000.0, 6000.0],
        );
        tiers(
            &mut t,
            "driftBoost",
            "Mini-turbo",
            &|n| format!("{n} drift boosts"),
            "🌀",
            |s| stat(s, "driftBoost"),
            &[10.0, 100.0, 500.0],
            &[500.0, 2000.0, 6000.0],
        );
        tiers(
            &mut t,
            "draft",
            "Slipstreamer",
            &|n| format!("{n} seconds in a slipstream"),
            "💨",
            |s| stat(s, "draft"),
            &[60.0, 600.0, 3000.0],
            &[500.0, 2000.0, 6000.0],
        );
        tiers(
            &mut t,
            "air",
            "Frequent flyer",
            &|n| format!("{n} seconds in the air"),
            "🎈",
            |s| stat(s, "air"),
            &[30.0, 300.0, 1500.0],
            &[500.0, 2000.0, 6000.0],
        );
        tiers(
            &mut t,
            "chain",
            "Chain reaction",
            &|n| format!("Bank a {}-point skill chain", thousands(n)),
            "⛓",
            |s| stat(s, "bestChain"),
            &[10000.0, 50000.0, 200000.0],
            &[1000.0, 4000.0, 12000.0],
        );
        tiers(
            &mut t,
            "speed",
            "Speed freak",
            &|n| format!("Reach {n} mph"),
            "⚡",
            |s| stat(s, "topSpeed"),
            &[120.0, 200.0, 400.0],
            &[500.0, 2000.0, 6000.0],
        );
        tiers(
            &mut t,
            "boss",
            "Giant slayer",
            &|n| format!("Beat {n} boss{}", if n == 1.0 { "" } else { "es" }),
            "👑",
            |s| s.stars.iter().enumerate().filter(|(i, st)| **st > 0 && (i + 1) % 10 == 0).count() as f64,
            &[1.0, 5.0, 10.0],
            &[1000.0, 5000.0, 15000.0],
        );
        tiers(
            &mut t,
            "perfect",
            "Perfectionist",
            &|n| format!("3 stars on {n} levels"),
            "⭐",
            |s| s.stars.iter().filter(|x| **x == 3).count() as f64,
            &[10.0, 50.0, LEVELS as f64],
            &[1000.0, 6000.0, 25000.0],
        );
        tiers(
            &mut t,
            "cars",
            "Collector",
            &|n| format!("Own {n} cars"),
            "🚗",
            |s| s.owned.len() as f64,
            &[3.0, CHASSIS.len() as f64],
            &[1000.0, 10000.0],
        );
        tiers(
            &mut t,
            "maxed",
            "Fully loaded",
            &|_| "Max out an upgrade".into(),
            "🔧",
            |s| s.upgrades.values().copied().max().unwrap_or(0) as f64,
            &[10.0],
            &[3000.0],
        );
        tiers(
            &mut t,
            "level",
            "Driver level",
            &|n| format!("Reach driver level {n}"),
            "🎖",
            |s| driver_level(s.xp).0 as f64,
            &[5.0, 15.0, 30.0],
            &[1000.0, 5000.0, 15000.0],
        );
        tiers(
            &mut t,
            "found",
            "Treasure hunter",
            &|n| format!("Find {n} gold wrenches in free roam"),
            "🔧",
            |s| s.found.len() as f64,
            &[10.0, 30.0, WRENCHES as f64],
            &[1000.0, 5000.0, 20000.0],
        );
        tiers(
            &mut t,
            "zones",
            "Explorer",
            &|n| format!("{n} stars on free-roam challenges"),
            "🗺",
            |s| s.zones.values().sum(),
            &[10.0, 40.0, ZONE_STARS as f64],
            &[1000.0, 5000.0, 15000.0],
        );
        tiers(
            &mut t,
            "daily",
            "Regular",
            &|n| format!("A {n}-day daily streak"),
            "📅",
            |s| s.streak as f64,
            &[3.0, 7.0, 30.0],
            &[1000.0, 3000.0, 15000.0],
        );
        tiers(
            &mut t,
            "sandbox",
            "No limits",
            &|n| format!("Clear sandbox level {}", 100.0 + n),
            "∞",
            |s| s.sandbox_best.saturating_sub(LEVELS) as f64,
            &[10.0, 50.0, 100.0],
            &[5000.0, 20000.0, 50000.0],
        );
        tiers(
            &mut t,
            "legend",
            "Legend",
            &|_| "Finish the career".into(),
            "🏆",
            |s| if s.stars[LEVELS - 1] > 0 { 1.0 } else { 0.0 },
            &[1.0],
            &[25000.0],
        );
        tiers(
            &mut t,
            "spins",
            "High roller",
            &|n| format!("Spin the prize wheel {n} times"),
            "🎰",
            |s| stat(s, "spins"),
            &[10.0, 50.0],
            &[1000.0, 5000.0],
        );
        t
    })
}

/// Awards any trophies just won. Returns the new ones (indices into trophies()).
pub fn check_trophies(save: &mut CareerSave) -> Vec<usize> {
    let mut won = Vec::new();
    for (i, t) in trophies().iter().enumerate() {
        if save.trophies.contains(&t.id) || (t.value)(save) < t.need {
            continue;
        }
        save.trophies.push(t.id.clone());
        save.coins += t.reward;
        save.earned += t.reward;
        won.push(i);
    }
    won
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_trophies() {
        assert_eq!(driver_level(0.0).0, 1);
        assert_eq!(driver_level(5000.0).0, 2);
        assert_eq!(trophies().len(), 57);
        let mut s = crate::career::new_career(0);
        s.wins = 1;
        let won = check_trophies(&mut s);
        assert_eq!(won.len(), 1);
        assert_eq!(s.coins, 500.0);
    }
}
