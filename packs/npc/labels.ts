// The name floating over each cast member's head, with a face for how they
// feel, and the speech bubble that pops up when they talk. Plain HTML laid
// over the game, moved every frame to sit above their heads.
import * as THREE from 'three';
import type { Emotion } from './cast';

const FACES: Record<Emotion, string> = {
  happy: '😊',
  excited: '🤩',
  calm: '🙂',
  curious: '🤔',
  amused: '😄',
  worried: '😟',
  scared: '😱',
  annoyed: '😠',
  sad: '😢',
  proud: '😎',
};

/** Name tags further away than this are hidden; bubbles carry a little further. */
const TAG_RANGE = 90;
const BUBBLE_RANGE = 120;

export class Label {
  readonly tag: HTMLElement;
  private name: HTMLElement;
  private face: HTMLElement;
  private bubble: HTMLElement;
  private bubbleUntil = 0;
  private point = new THREE.Vector3();
  private friend = false;

  /** `onClick` runs when the name is clicked (to start a chat). */
  constructor(layer: HTMLElement, name: string, style: string, onClick: () => void) {
    this.tag = document.createElement('div');
    this.tag.className = `npc-tag ${style}`;
    this.bubble = document.createElement('div');
    this.bubble.className = 'npc-bubble';
    this.name = document.createElement('span');
    this.name.className = 'npc-name';
    this.name.textContent = name;
    this.name.title = `Talk to ${name}`;
    // Clicks on the name go to the chat, never through to the game (where a click fires weapons).
    this.name.addEventListener('pointerdown', (e) => {
      e.stopPropagation();
      e.preventDefault();
      if (e.button === 0) onClick();
    });
    this.face = document.createElement('span');
    this.face.className = 'npc-face';
    this.tag.append(this.bubble, this.face, this.name);
    layer.append(this.tag);
  }

  setEmotion(emotion: Emotion): void {
    const face = FACES[emotion];
    if (this.face.textContent !== face) this.face.textContent = face;
  }

  /** Marks the people who count you as a friend. */
  setFriend(friend: boolean): void {
    if (friend === this.friend) return;
    this.friend = friend;
    this.tag.classList.toggle('friend', friend);
  }

  /** Shows a line over their head for a while. A thought is shown in italics. */
  say(text: string, seconds: number, now: number, thought = false): void {
    this.bubble.textContent = text;
    this.bubble.classList.toggle('thought', thought);
    this.bubble.classList.add('show');
    this.bubbleUntil = now + seconds;
  }

  /** Takes the bubble away. */
  hush(): void {
    this.bubble.classList.remove('show');
    this.bubbleUntil = 0;
  }

  get talking(): boolean {
    return this.bubble.classList.contains('show');
  }

  /** Moves the label over their head. `showTag` is the name tags setting. */
  place(x: number, y: number, z: number, camera: THREE.Camera, width: number, height: number, now: number, visible: boolean, showTag: boolean): void {
    if (now > this.bubbleUntil) this.bubble.classList.remove('show');
    const distance = camera.position.distanceTo(this.point.set(x, y, z));
    this.point.project(camera);
    const inView = visible && this.point.z < 1 && Math.abs(this.point.x) < 1.2 && Math.abs(this.point.y) < 1.2;
    const tagShown = inView && showTag && distance < TAG_RANGE;
    const bubbleShown = inView && this.talking && distance < BUBBLE_RANGE;
    if (!tagShown && !bubbleShown) {
      this.tag.style.display = 'none';
      return;
    }
    this.tag.style.display = '';
    this.tag.classList.toggle('no-name', !tagShown);
    // Smaller with distance, but always readable.
    const scale = Math.max(0.55, Math.min(1.15, 14 / Math.max(1, distance)));
    const sx = (this.point.x * 0.5 + 0.5) * width;
    const sy = (-this.point.y * 0.5 + 0.5) * height;
    this.tag.style.transform = `translate(${sx.toFixed(1)}px, ${sy.toFixed(1)}px) translate(-50%, -100%) scale(${scale.toFixed(3)})`;
    this.tag.style.zIndex = String(Math.max(1, 1000 - Math.round(distance)));
  }

  dispose(): void {
    this.tag.remove();
  }
}
