// The telemetry panel:
// - a g-force dot inside the grip circle;
// - each tyre seen from above: its colour is the grip at work (green, then
//   amber at the limit, red when sliding), with bars for the weight on it and
//   how far its spring is squashed, and its temperature (blue cold, green in
//   its window, red overheated) and wear;
// - the engine's revs and turbo boost, the gear and the brake temperature;
// - lights for the driving aids when they step in, and for fading brakes;
// - any crash damage.
import type { CarPhysics } from '../../game/carPhysics';
import { CORNER_MARGIN, type CarState, type Realism } from './model';
import { bestTemperature } from './tyres';

const W = 300;
const H = 172;
const GRIP = { road: 0.95, sport: 1.2, race: 1.45 } as Record<string, number>;
const REALISM: Record<Realism, string> = { friendly: 'FRIENDLY', real: 'REAL', sim: 'SIMULATION' };

export class Telemetry {
  private canvas = document.createElement('canvas');
  private ctx: CanvasRenderingContext2D;
  /** The last half-second of g-force dots, for a fading trail. */
  private trail: { x: number; y: number }[] = [];

  constructor(parent: HTMLElement) {
    const ratio = Math.min(2, window.devicePixelRatio || 1);
    this.canvas.width = W * ratio;
    this.canvas.height = H * ratio;
    this.canvas.className = 'physics-telemetry hidden';
    // (The stylesheet sizes the panel for the first version; this one is bigger.)
    this.canvas.style.width = `${W}px`;
    this.canvas.style.height = `${H}px`;
    this.ctx = this.canvas.getContext('2d')!;
    this.ctx.scale(ratio, ratio);
    parent.append(this.canvas);
  }

  setVisible(visible: boolean): void {
    this.canvas.classList.toggle('hidden', !visible);
  }

  draw(s: CarState, car: CarPhysics, grip: string, realism: Realism): void {
    const ctx = this.ctx;
    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = 'rgba(8, 12, 20, 0.62)';
    roundRect(ctx, 0, 0, W, H, 12);
    ctx.fill();

    ctx.textAlign = 'left';
    ctx.fillStyle = 'rgba(255,255,255,0.5)';
    ctx.font = '600 9px system-ui, sans-serif';
    ctx.fillText(`${grip.toUpperCase()} TYRES · ${REALISM[realism]}`, 10, 14);

    // ---------- the g-force circle: the outer ring is the most the tyres can give ----------
    const cx = 60;
    const cy = 72;
    const R = 44;
    const limit = Math.max(1, car.cornerGrip(Math.abs(car.forwardSpeed)) / CORNER_MARGIN);
    ctx.strokeStyle = 'rgba(255,255,255,0.25)';
    ctx.lineWidth = 1;
    for (const f of [0.5, 1]) {
      ctx.beginPath();
      ctx.arc(cx, cy, R * f, 0, Math.PI * 2);
      ctx.stroke();
    }
    ctx.beginPath();
    ctx.moveTo(cx - R, cy);
    ctx.lineTo(cx + R, cy);
    ctx.moveTo(cx, cy - R);
    ctx.lineTo(cx, cy + R);
    ctx.stroke();
    // Braking pushes the dot up, accelerating down; turning left pushes it right (you feel it sideways).
    const dot = { x: cx - (s.ay / limit) * R, y: cy + (s.ax / limit) * R };
    this.trail.push(dot);
    if (this.trail.length > 30) this.trail.shift();
    this.trail.forEach((p, i) => {
      ctx.fillStyle = `rgba(120, 200, 255, ${(i / this.trail.length) * 0.5})`;
      ctx.beginPath();
      ctx.arc(p.x, p.y, 2, 0, Math.PI * 2);
      ctx.fill();
    });
    ctx.fillStyle = '#7fd4ff';
    ctx.beginPath();
    ctx.arc(dot.x, dot.y, 4.5, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = 'rgba(255,255,255,0.75)';
    ctx.font = '600 10px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText(`${(Math.hypot(s.ax, s.ay) / 9.81).toFixed(2)} g`, cx, cy + R + 13);

    // ---------- the four tyres, seen from above ----------
    const best = bestTemperature(GRIP[grip] ?? 1.2);
    const tyreW = 16;
    const tyreH = 28;
    const travel = s.susp.compression;
    const full = Math.max(1e-6, Math.max(...travel, 0.05) * 1.6);
    for (let i = 0; i < 4; i++) {
      const x = 124 + (i % 2) * 56;
      const y = 22 + (i < 2 ? 0 : 56);
      const use = s.usage[i];
      ctx.fillStyle = use < 0.8 ? '#3bb273' : use < 1.15 ? '#ffb627' : '#e63946';
      roundRect(ctx, x, y, tyreW, tyreH, 4);
      ctx.fill();
      // Worn rubber: the tyre goes darker from the top down.
      if (s.wear[i] > 0.01) {
        ctx.fillStyle = 'rgba(0,0,0,0.45)';
        ctx.fillRect(x, y, tyreW, tyreH * Math.min(1, s.wear[i]));
      }
      // Bars beside it: the weight on the tyre, and how far its spring is squashed.
      bar(ctx, x + tyreW + 4, y, tyreH, Math.min(2, s.load[i]) / 2, 'rgba(255,255,255,0.8)');
      bar(ctx, x + tyreW + 10, y, tyreH, Math.max(0, travel[i]) / full, '#9b8cff');
      // Temperature.
      const t = s.temp[i];
      ctx.font = '600 9px system-ui, sans-serif';
      ctx.textAlign = 'left';
      ctx.fillStyle = Number.isNaN(t) ? 'rgba(255,255,255,0.4)' : temperatureColour(t - best);
      ctx.fillText(Number.isNaN(t) ? '--°' : `${Math.round(t)}°`, x, y + tyreH + 11);
    }

    // ---------- the engine: revs, turbo boost, gear, and the brakes' temperature ----------
    const ex = 244;
    const rpm = Math.min(1.05, Math.max(0, s.rpm));
    bar(ctx, ex, 22, 84, rpm, rpm > 0.95 ? '#e63946' : rpm > 0.85 ? '#ffb627' : '#7fd4ff', 8);
    bar(ctx, ex + 12, 22, 84, s.boost, '#ffb627', 6);
    ctx.textAlign = 'center';
    ctx.fillStyle = 'rgba(255,255,255,0.5)';
    ctx.font = '600 8px system-ui, sans-serif';
    ctx.fillText('RPM', ex + 4, 116);
    ctx.fillText('BOOST', ex + 15, 125);
    ctx.fillStyle = 'rgba(255,255,255,0.9)';
    ctx.font = '700 22px system-ui, sans-serif';
    ctx.fillText(car.forwardSpeed < -0.5 ? 'R' : String(car.gear), ex + 38, 44);
    ctx.font = '600 8px system-ui, sans-serif';
    ctx.fillStyle = 'rgba(255,255,255,0.5)';
    ctx.fillText('BRAKES', ex + 38, 70);
    ctx.font = '700 10px system-ui, sans-serif';
    const brake = s.brakeTemp;
    ctx.fillStyle = Number.isNaN(brake) ? 'rgba(255,255,255,0.4)' : brake > 550 ? '#e63946' : brake > 400 ? '#ffb627' : 'rgba(255,255,255,0.8)';
    ctx.fillText(Number.isNaN(brake) ? '--' : `${Math.round(brake)}°`, ex + 38, 82);

    // ---------- lights: the driving aids, and fading brakes ----------
    const lights: [string, boolean][] = [['TC', s.tractionControl], ['ABS', s.abs], ['ESC', s.stability], ['FADE', s.fade]];
    ctx.font = '700 10px system-ui, sans-serif';
    ctx.textAlign = 'center';
    lights.forEach(([label, on], i) => {
      const x = 12 + i * 36;
      ctx.fillStyle = on ? (label === 'FADE' ? '#e63946' : '#ffb627') : 'rgba(255,255,255,0.18)';
      roundRect(ctx, x, 140, 32, 16, 4);
      ctx.fill();
      ctx.fillStyle = on ? '#111' : 'rgba(255,255,255,0.6)';
      ctx.fillText(label, x + 16, 151.5);
    });

    // ---------- crash damage ----------
    const d = s.damage;
    const notes: string[] = [];
    if (d.engine > 0.01) notes.push(`ENGINE ${Math.round((1 - d.engine) * 100)}%`);
    if (Math.abs(d.toe) > 0.002) notes.push(`STEERING PULLS ${d.toe > 0 ? 'LEFT' : 'RIGHT'}`);
    const bent = d.wheel.filter((w) => w > 0.01).length;
    if (bent > 0) notes.push(`${bent} WHEEL${bent > 1 ? 'S' : ''} BENT`);
    ctx.textAlign = 'left';
    ctx.font = '700 8px system-ui, sans-serif';
    ctx.fillStyle = notes.length ? '#ff8a8a' : 'rgba(255,255,255,0.35)';
    ctx.fillText(notes.length ? notes.join(' · ') : 'NO DAMAGE', 160, 151.5, W - 168);
  }

  dispose(): void {
    this.canvas.remove();
  }
}

/** A vertical bar filled from the bottom (0..1). */
function bar(ctx: CanvasRenderingContext2D, x: number, y: number, h: number, fill: number, colour: string, w = 4): void {
  const f = Math.max(0, Math.min(1, fill));
  ctx.fillStyle = 'rgba(255,255,255,0.18)';
  ctx.fillRect(x, y, w, h);
  ctx.fillStyle = colour;
  ctx.fillRect(x, y + h * (1 - f), w, h * f);
}

/** Blue when cold, green in the tyre's window, red when overheated. */
function temperatureColour(off: number): string {
  if (off < -25) return '#6fb8ff';
  if (off < -10) return '#a8e0ff';
  if (off <= 15) return '#3bd18a';
  if (off <= 30) return '#ffb627';
  return '#e63946';
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number): void {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
}
