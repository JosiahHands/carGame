// The tires: the "Magic Formula" grip curve, and how temperature and wear
// change a tire's grip.
//
// A tire grips best in a window of temperatures. Cold, the rubber is hard
// and slides; overheated (from sliding around), it goes greasy. Sliding and
// spinning heat it fast, the air rushing past cools it. Rubber wears away
// as it slides, and a worn tire grips a little less.

/** The Magic Formula's shape (B stiffness, C shape, E curvature), for slip normalized so the peak is at 1. */
const B = 10;
const C = 1.9;
const E = 0.97;
/** Where the curve peaks (slip ratio or tan of slip angle), and its height there. */
export const PEAK_SLIP = 0.11;

/** y = sin(C·atan(Bx − E(Bx − atan Bx))): tire force (as a share of the most it can give) for slip x. */
export function magic(x: number): number {
  const bx = B * x;
  return Math.sin(C * Math.atan(bx - E * (bx - Math.atan(bx))));
}
export const PEAK = magic(PEAK_SLIP);

/** The temperature where a tire of this grip grips best, °C: a road tire works cool, a racing slick hot. */
export function bestTemperature(grip: number): number {
  return 40 + grip * 35; // road 73 °C, sport 82 °C, race 91 °C
}

/**
 * Share of its best grip a tire has at temperature `t`: full in its window,
 * less either side, never below `floor`.
 */
export function temperatureGrip(t: number, best: number, floor: number): number {
  const off = (t - best) / 45;
  return Math.max(floor, 1 - 0.15 * off * off);
}

/** Share of its grip a tire has left at this wear (0 new, 1 bald). */
export function wearGrip(wear: number): number {
  return 1 - 0.15 * Math.min(1, wear);
}

/**
 * The tire's temperature one step on. `slipPower` is how hard it's being
 * worked (sliding force × sliding speed, per kg), `speed` how fast the air
 * goes past, `ambient` the air temperature.
 *
 * Tuned so a tire settles about 25 °C over the air cruising, near its best
 * window driven hard, and around 120 °C sliding flat out (understeering
 * around a skidpad). The heat grows more slowly than the work, as the hot
 * rubber sheds more of it.
 */
export function heatTire(t: number, slipPower: number, speed: number, ambient: number, dt: number): number {
  const heating = 1.5 * Math.max(0, slipPower) ** 0.25;
  const cooling = 0.02 * (t - ambient) * (1 + speed / 20);
  return t + (heating - cooling) * dt;
}

/** The air temperature for the weather, °C. */
export function ambientTemperature(weather: string, night: boolean): number {
  const base = weather === 'snow' ? 0 : weather === 'rain' ? 12 : 24;
  return base - (night ? 6 : 0);
}
