//! What an event is: its kind, venue, rivals, targets and rules. Career
//! levels, the daily challenge and quick races all make these (career.rs);
//! event.rs runs them.
use crate::config::CarConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventKind {
    Race,
    Elimination,
    Duel,
    TimeAttack,
    Takedown,
    Drift,
    SpeedTrap,
    Destruction,
    Stunt,
}

impl EventKind {
    pub const ALL: [EventKind; 9] = [
        EventKind::Race,
        EventKind::Elimination,
        EventKind::Duel,
        EventKind::TimeAttack,
        EventKind::Takedown,
        EventKind::Drift,
        EventKind::SpeedTrap,
        EventKind::Destruction,
        EventKind::Stunt,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EventKind::Race => "Race",
            EventKind::Elimination => "Elimination",
            EventKind::Duel => "Boss duel",
            EventKind::TimeAttack => "Time attack",
            EventKind::Takedown => "Takedown",
            EventKind::Drift => "Drift",
            EventKind::SpeedTrap => "Speed trap",
            EventKind::Destruction => "Destruction",
            EventKind::Stunt => "Stunt show",
        }
    }

    pub fn rules(self) -> &'static str {
        match self {
            EventKind::Race => "Finish in the top 3. 1st place for 3 stars.",
            EventKind::Elimination => "The car in last place is knocked out every few seconds. Be the last one left.",
            EventKind::Duel => "One on one against the boss. Beat them to move on.",
            EventKind::TimeAttack => "Just you and the clock. Beat the target times.",
            EventKind::Takedown => "Weapons on. Wreck rivals with guns, rockets and rams. They shoot back!",
            EventKind::Drift => "Score drift points before the time runs out. Crashing loses the drift.",
            EventKind::SpeedTrap => "Hit the speed cameras as fast as you can. Your speeds are added up.",
            EventKind::Destruction => "Weapons on. Do as much damage to the city as you can.",
            EventKind::Stunt => "Use the ramps. Every second in the air counts.",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            EventKind::Race => "🏁",
            EventKind::Elimination => "⏳",
            EventKind::Duel => "👑",
            EventKind::TimeAttack => "⏱",
            EventKind::Takedown => "💥",
            EventKind::Drift => "🌀",
            EventKind::SpeedTrap => "📸",
            EventKind::Destruction => "🏚",
            EventKind::Stunt => "🚀",
        }
    }

    /// The name the 1.2.x game used (for old quick race settings).
    pub fn id(self) -> &'static str {
        match self {
            EventKind::Race => "race",
            EventKind::Elimination => "elimination",
            EventKind::Duel => "duel",
            EventKind::TimeAttack => "timeAttack",
            EventKind::Takedown => "takedown",
            EventKind::Drift => "drift",
            EventKind::SpeedTrap => "speedTrap",
            EventKind::Destruction => "destruction",
            EventKind::Stunt => "stunt",
        }
    }

    pub fn from_id(id: &str) -> Option<EventKind> {
        EventKind::ALL.iter().copied().find(|k| k.id() == id)
    }
}

/// Who a rival is, and what they drive.
#[derive(Clone, Debug)]
pub struct RivalSpec {
    pub name: String,
    pub config: CarConfig,
    /// How close to the limit they drive: 0.75 (gentle) to 1.05 (flat out, no mistakes).
    pub skill: f64,
    /// Never makes mistakes: the pace car that sets target times.
    pub steady: bool,
}

#[derive(Clone, Debug)]
pub struct EventSpec {
    pub kind: EventKind,
    pub title: String,
    pub map: String,
    pub layout: u32,
    pub laps: u32,
    /// Seconds: the time limit, or for an elimination, how often someone is knocked out.
    pub time_limit: f64,
    pub rivals: Vec<RivalSpec>,
    /// The car and skill that set the target times and speeds for time attacks and speed traps.
    pub pace: Option<(CarConfig, f64)>,
    /// What 1, 2 and 3 stars need.
    pub targets: [f64; 3],
    pub night: bool,
    pub weapons: bool,
    pub no_nitro: bool,
    pub catch_up: f64,
    pub armor: f64,
    pub rival_armor: f64,
    pub rival_damage: f64,
    pub boss: Option<String>,
}
