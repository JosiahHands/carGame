//! The tires: the "Magic Formula" grip curve, and how temperature and wear
//! change a tire's grip. Cold rubber is hard and slides; overheated rubber
//! goes greasy. Sliding heats it, the air rushing past cools it, and it
//! wears away as it slides.
use game::venues::Weather;

const B: f64 = 10.0;
const C: f64 = 1.9;
const E: f64 = 0.97;
/// Where the curve peaks (slip ratio or tan of slip angle).
pub const PEAK_SLIP: f64 = 0.11;

/// Tire force (as a share of the most it can give) for slip x.
pub fn magic(x: f64) -> f64 {
    let bx = B * x;
    (C * (bx - E * (bx - bx.atan())).atan()).sin()
}

/// The curve's height at its peak, magic(PEAK_SLIP).
pub const PEAK: f64 = 0.970_803_486_419_246_3;

/// The temperature where a tire of this grip grips best, °C.
pub fn best_temperature(grip: f64) -> f64 {
    40.0 + grip * 35.0
}

pub fn temperature_grip(t: f64, best: f64, floor: f64) -> f64 {
    let off = (t - best) / 45.0;
    (1.0 - 0.15 * off * off).max(floor)
}

pub fn wear_grip(wear: f64) -> f64 {
    1.0 - 0.15 * wear.min(1.0)
}

pub fn heat_tire(t: f64, slip_power: f64, speed: f64, ambient: f64, dt: f64) -> f64 {
    let heating = 1.5 * slip_power.max(0.0).powf(0.25);
    let cooling = 0.02 * (t - ambient) * (1.0 + speed / 20.0);
    t + (heating - cooling) * dt
}

pub fn ambient_temperature(weather: Weather, night: bool) -> f64 {
    let base = match weather {
        Weather::Snow => 0.0,
        Weather::Rain => 12.0,
        Weather::Clear => 24.0,
    };
    base - if night { 6.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_curve_peaks_where_it_should() {
        let peak = super::magic(super::PEAK_SLIP);
        assert!((peak - super::PEAK).abs() < 1e-12, "peak {peak}");
        assert!(super::magic(0.5) < super::magic(0.2), "grip falls off past the top");
    }
}
