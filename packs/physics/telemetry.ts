// The telemetry panel: a g-force dot inside the grip circle, each tyre's
// grip at work (green, then amber at the limit, red when sliding) and load,
// and lights for the driving aids when they step in.
import type { CarPhysics } from '../../game/carPhysics';
import type { CarState } from './model';

const W = 230;
const H = 132;

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
    this.ctx = this.canvas.getContext('2d')!;
    this.ctx.scale(ratio, ratio);
    parent.append(this.canvas);
  }

  setVisible(visible: boolean): void {
    this.canvas.classList.toggle('hidden', !visible);
  }

  draw(s: CarState, car: CarPhysics, grip: string): void {
    const ctx = this.ctx;
    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = 'rgba(8, 12, 20, 0.62)';
    roundRect(ctx, 0, 0, W, H, 12);
    ctx.fill();

    // The g-force circle: the outer ring is the most the tyres can give.
    const cx = 62;
    const cy = 64;
    const R = 46;
    const limit = Math.max(1, car.cornerGrip(Math.abs(car.forwardSpeed)) / 0.92);
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
    const g = Math.hypot(s.ax, s.ay) / 9.81;
    ctx.fillText(`${g.toFixed(2)} g`, cx, cy + R + 13);

    // The four tyres, seen from above: colour is grip used, the bar is the weight on it.
    const tyreW = 18;
    const tyreH = 30;
    const left = 136;
    const top = 16;
    for (let i = 0; i < 4; i++) {
      const x = left + (i % 2) * 44;
      const y = top + (i < 2 ? 0 : 50);
      const use = s.usage[i];
      ctx.fillStyle = use < 0.8 ? '#3bb273' : use < 1.15 ? '#ffb627' : '#e63946';
      roundRect(ctx, x, y, tyreW, tyreH, 4);
      ctx.fill();
      // Weight bar beside the tyre.
      const load = Math.min(2, s.load[i]) / 2;
      ctx.fillStyle = 'rgba(255,255,255,0.18)';
      ctx.fillRect(x + tyreW + 4, y, 4, tyreH);
      ctx.fillStyle = 'rgba(255,255,255,0.8)';
      ctx.fillRect(x + tyreW + 4, y + tyreH * (1 - load), 4, tyreH * load);
    }
    // Driving aid lights.
    const lights: [string, boolean][] = [['TC', s.tractionControl], ['ABS', s.abs], ['ESC', s.stability]];
    ctx.font = '700 10px system-ui, sans-serif';
    lights.forEach(([label, on], i) => {
      const x = 136 + i * 30;
      ctx.fillStyle = on ? '#ffb627' : 'rgba(255,255,255,0.18)';
      roundRect(ctx, x, 104, 26, 16, 4);
      ctx.fill();
      ctx.fillStyle = on ? '#111' : 'rgba(255,255,255,0.6)';
      ctx.fillText(label, x + 13, 115.5);
    });
    ctx.textAlign = 'left';
    ctx.fillStyle = 'rgba(255,255,255,0.5)';
    ctx.font = '600 9px system-ui, sans-serif';
    ctx.fillText(`${grip.toUpperCase()} TYRES`, 10, 14);
  }

  dispose(): void {
    this.canvas.remove();
  }
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number): void {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
}
