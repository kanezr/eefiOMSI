// The launcher's icons: Google's Material Symbols Rounded, filled (see symbols.js), drawn in
// the text's colour.

import { html } from '../vendor/preact-htm.js';
import { SYMBOLS } from './symbols.js';

export function Icon({ name, size }) {
  const s = size || 20;
  return html`<svg class="icon" viewBox="0 -960 960 960" width=${s} height=${s} fill="currentColor" aria-hidden="true"><path d=${SYMBOLS[name] || SYMBOLS.info} /></svg>`;
}
