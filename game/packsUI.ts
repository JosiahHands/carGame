// The Packs tab in the garage: each installed pack, with a switch to turn it
// on or off, its own options, and a button to remove it.
import { el } from '../libs/dom';
import type { PackHost, PanelUI } from './packs';

const ON_OFF = [
  { value: 'on' as const, label: 'On' },
  { value: 'off' as const, label: 'Off' },
];

export function showPacksTab(host: PackHost, panel: PanelUI): void {
  const packs = host.installed;
  if (packs.length === 0) {
    panel.heading('Packs');
    panel.hint(
      'No packs are installed. Packs add to the game: lifelike graphics, realistic physics and city people with minds of their own. ' +
        'Install one with its command from the README (packs need the game installed from source), then open the game again.',
    );
    return;
  }
  panel.hint('Packs installed in this game. Switch one off to try the game without it, or remove it.');
  for (const pack of packs) {
    panel.heading(`${pack.icon} ${pack.name}`);
    panel.hint(pack.about);
    panel.choice('Pack', ON_OFF, () => (host.isOn(pack.id) ? 'on' : 'off'), (v) => {
      host.setOn(pack.id, v === 'on');
      panel.refresh();
    });
    if (host.isOn(pack.id)) host.settings(pack.id, panel);
    if (host.removable) {
      // Two clicks, so it can't be removed by accident.
      const remove = el('button', 'btn danger', 'Remove pack');
      let sure = false;
      remove.addEventListener('click', () => {
        if (!sure) {
          sure = true;
          remove.textContent = 'Click again to remove it';
          return;
        }
        remove.disabled = true;
        void host.remove(pack.id).then((done) => {
          if (!done) remove.textContent = "Couldn't remove it";
          else panel.refresh();
        });
      });
      panel.row('Remove', remove);
    } else {
      panel.hint('To remove this pack, take its name out of packs/installed.json in the game folder and update the game.');
    }
  }
  panel.hint('A removed pack stops straight away, and is left out of the game the next time it updates.');
}
