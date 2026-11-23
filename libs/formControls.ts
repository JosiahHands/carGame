// Small DOM form controls (button groups, sliders, colour swatches, text boxes)
// built from plain elements. Generic: nothing in here knows about the car game.

export interface Choice<T extends string> {
  value: T;
  label: string;
}

/** Creates an element with an optional class name and text. */
export function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

/** A labelled row holding one control. */
export function row(label: string, control: HTMLElement): HTMLDivElement {
  const r = el('div', 'row');
  r.append(el('label', undefined, label), control);
  return r;
}

/** A row of buttons where exactly one is selected. */
export function segmented<T extends string>(options: Choice<T>[], get: () => T, set: (v: T) => void): HTMLDivElement {
  const group = el('div', 'segmented');
  const buttons: [T, HTMLButtonElement][] = [];
  const sync = () => {
    for (const [v, b] of buttons) b.classList.toggle('active', v === get());
  };
  for (const o of options) {
    const b = el('button', undefined, o.label);
    b.type = 'button';
    b.addEventListener('click', () => {
      set(o.value);
      sync();
    });
    buttons.push([o.value, b]);
    group.append(b);
  }
  sync();
  return group;
}

/** A range slider with its formatted value beside it. Values snap to `step`. */
export function slider(
  min: number,
  max: number,
  step: number,
  get: () => number,
  set: (v: number) => void,
  fmt: (v: number) => string,
): HTMLDivElement {
  const wrap = el('div', 'slider');
  const input = el('input');
  input.type = 'range';
  input.min = String(min);
  input.max = String(max);
  input.step = String(step);
  input.value = String(get());
  const out = el('span', 'value', fmt(get()));
  input.addEventListener('input', () => {
    const v = Number((Math.round(Number(input.value) / step) * step).toFixed(2));
    set(v);
    out.textContent = fmt(v);
  });
  wrap.append(input, out);
  return wrap;
}

/** Colour swatches plus a picker for any other colour. Colours are #rrggbb. */
export function swatches(colors: string[], get: () => string, set: (v: string) => void): HTMLDivElement {
  const wrap = el('div', 'swatches');
  const picker = el('input');
  picker.type = 'color';
  picker.title = 'Pick any colour';
  const sync = () => {
    for (const b of wrap.querySelectorAll<HTMLButtonElement>('button')) b.classList.toggle('active', b.dataset.color === get());
    picker.value = get();
  };
  for (const c of colors) {
    const b = el('button', 'swatch');
    b.type = 'button';
    b.style.background = c;
    b.dataset.color = c;
    b.title = c;
    b.addEventListener('click', () => {
      set(c);
      sync();
    });
    wrap.append(b);
  }
  picker.addEventListener('input', () => {
    set(picker.value.toLowerCase());
    sync();
  });
  wrap.append(picker);
  sync();
  return wrap;
}

/** A one-line text box. `clean` tidies what is typed (for example upper-casing it). */
export function textInput(maxLength: number, get: () => string, set: (v: string) => void, clean: (v: string) => string = (v) => v): HTMLInputElement {
  const input = el('input', 'text');
  input.type = 'text';
  input.maxLength = maxLength;
  input.value = get();
  input.spellcheck = false;
  input.addEventListener('input', () => {
    const v = clean(input.value);
    if (v !== input.value) input.value = v;
    set(v);
  });
  return input;
}
