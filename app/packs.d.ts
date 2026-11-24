// The installed packs, made by the build (see vite.config.mts).
declare module 'virtual:installed-packs' {
  import type { Pack } from '../game/packs';
  const packs: Pack[];
  export default packs;
  /** Changes every time the game is built. */
  export const buildId: string;
}
