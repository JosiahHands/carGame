//! Skill chains: stylish driving scores points. Drifts, near misses,
//! slipstreaming, jumps, overtakes, takedowns and boosts all add to a chain,
//! and the more different skills in it, the bigger its multiplier (up to
//! ×10). The chain is banked a few seconds after the last skill; a hard crash
//! before then loses it all.
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkillKind {
    Drift,
    NearMiss,
    Draft,
    Air,
    Overtake,
    Takedown,
    PerfectStart,
    DriftBoost,
    Smash,
    Speed,
    Revenge,
    Collect,
}

impl SkillKind {
    pub fn label(self) -> &'static str {
        match self {
            SkillKind::Drift => "Drift",
            SkillKind::NearMiss => "Near miss",
            SkillKind::Draft => "Slipstream",
            SkillKind::Air => "Air",
            SkillKind::Overtake => "Overtake",
            SkillKind::Takedown => "Takedown",
            SkillKind::PerfectStart => "Perfect start",
            SkillKind::DriftBoost => "Drift boost",
            SkillKind::Smash => "Smash",
            SkillKind::Speed => "Speed demon",
            SkillKind::Revenge => "Revenge",
            SkillKind::Collect => "Collectible",
        }
    }

    /// The color it's shown in (sRGB hex).
    pub fn color(self) -> u32 {
        match self {
            SkillKind::Drift => 0xffd23f,
            SkillKind::NearMiss => 0xff70a6,
            SkillKind::Draft => 0x7af0ff,
            SkillKind::Air => 0x9b8cff,
            SkillKind::Overtake => 0x3bb273,
            SkillKind::Takedown => 0xff5a36,
            SkillKind::PerfectStart => 0xffffff,
            SkillKind::DriftBoost => 0x4fb3ff,
            SkillKind::Smash => 0xff9f40,
            SkillKind::Speed => 0xff3b3b,
            SkillKind::Revenge | SkillKind::Collect => 0xd4af37,
        }
    }

    /// The name the 1.2.x game saved its stats under.
    pub fn stat(self) -> &'static str {
        match self {
            SkillKind::NearMiss => "nearMiss",
            SkillKind::Takedown => "takedown",
            SkillKind::Overtake => "overtake",
            SkillKind::PerfectStart => "perfectStart",
            SkillKind::DriftBoost => "driftBoost",
            _ => "",
        }
    }
}

/// Seconds after the last skill before the chain is banked.
pub const CHAIN_TIME: f64 = 4.0;
const MAX_MULTIPLIER: f64 = 10.0;

#[derive(Clone, Debug)]
pub struct FeedLine {
    pub kind: SkillKind,
    pub points: f64,
    pub label: String,
}

/// What happened to a chain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChainEnd {
    Banked(f64),
    Broken(f64),
}

#[derive(Default, Clone, Debug)]
pub struct SkillChain {
    pub points: f64,
    pub count: usize,
    kinds: BTreeSet<SkillKind>,
    pub timer: f64,
    pub feed: Vec<FeedLine>,
}

impl SkillChain {
    /// ×1 for one kind of skill, +1 for each other kind, +0.5 for every 5 skills, up to ×10.
    pub fn multiplier(&self) -> f64 {
        if self.count == 0 {
            return 1.0;
        }
        (self.kinds.len() as f64 + (self.count / 5) as f64 * 0.5).min(MAX_MULTIPLIER)
    }

    pub fn total(&self) -> f64 {
        (self.points * self.multiplier()).round()
    }

    pub fn add(&mut self, kind: SkillKind, points: f64, label: Option<&str>) {
        let points = points.round();
        if points <= 0.0 {
            return;
        }
        let label = label.unwrap_or(kind.label()).to_string();
        self.points += points;
        self.count += 1;
        self.kinds.insert(kind);
        self.timer = CHAIN_TIME;
        match self.feed.first_mut() {
            Some(top) if top.kind == kind && top.label == label => top.points += points,
            _ => self.feed.insert(0, FeedLine { kind, points, label }),
        }
        self.feed.truncate(5);
    }

    pub fn update(&mut self, dt: f64) -> Option<ChainEnd> {
        if self.count == 0 {
            return None;
        }
        self.timer -= dt;
        if self.timer <= 0.0 { self.bank() } else { None }
    }

    pub fn bank(&mut self) -> Option<ChainEnd> {
        if self.count == 0 {
            return None;
        }
        let total = self.total();
        self.clear();
        Some(ChainEnd::Banked(total))
    }

    pub fn break_chain(&mut self) -> Option<ChainEnd> {
        if self.count == 0 {
            return None;
        }
        let lost = self.total();
        self.clear();
        Some(ChainEnd::Broken(lost))
    }

    pub fn clear(&mut self) {
        *self = SkillChain::default();
    }
}
