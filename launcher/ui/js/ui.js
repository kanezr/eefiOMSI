// The page's controls. Every text passed in is translated here (`t`), so callers write
// English, as the game's tables want it.

import { html, useEffect, useRef, useState } from '../vendor/preact-htm.js';
import { Icon } from './icons.js';
import { t } from './i18n.js';

export { html };

export function Button({ kind = '', icon, children, label, onClick, disabled, title, type = 'button', big, wide, small }) {
  const cls = ['btn', kind, big && 'big', wide && 'wide', small && 'small', icon && !children && !label && 'icon'].filter(Boolean).join(' ');
  return html`<button type=${type} class=${cls} onClick=${onClick} disabled=${disabled} title=${title ? t(title) : undefined}>
    ${icon && html`<${Icon} name=${icon} />`}${label ? t(label) : children}
  </button>`;
}

export function Card({ title, hint, actions, children, class: cls = '', body = true }) {
  return html`<section class=${'card ' + cls}>
    ${(title || actions) && html`<div class="card-head"><div><h2>${t(title)}</h2>${hint && html`<div class="hint">${t(hint)}</div>`}</div>${actions && html`<div class="row">${actions}</div>`}</div>`}
    ${body ? html`<div class="card-body">${children}</div>` : children}
  </section>`;
}

export function Field({ label, help, children }) {
  return html`<div class="field">${label && html`<label>${t(label)}</label>`}${children}${help && html`<div class="help">${t(help)}</div>`}</div>`;
}

/** A select of `options`: [value, label] pairs (labels translated unless `raw`). */
export function Select({ value, options, onChange, disabled, raw, placeholder }) {
  const has = options.some(([v]) => String(v) === String(value));
  return html`<select class="select" value=${String(value ?? '')} disabled=${disabled} onChange=${(e) => onChange(e.currentTarget.value)}>
    ${!has && html`<option value=${String(value ?? '')}>${value === '' || value == null ? t(placeholder || 'Choose…') : String(value)}</option>`}
    ${options.map(([v, l]) => html`<option value=${String(v)}>${raw ? l : t(l)}</option>`)}
  </select>`;
}

export function Input({ value, onInput, placeholder, type = 'text', ...rest }) {
  return html`<input class="input" type=${type} value=${value ?? ''} placeholder=${placeholder ? t(placeholder) : ''} onInput=${(e) => onInput(e.currentTarget.value)} spellcheck="false" ...${rest} />`;
}

export function Search({ value, onInput, placeholder = 'Search', autofocus }) {
  const ref = useRef();
  useEffect(() => {
    if (autofocus) ref.current?.focus();
  }, []);
  return html`<div class="search"><${Icon} name="search" /><input ref=${ref} class="input" value=${value} placeholder=${t(placeholder)} onInput=${(e) => onInput(e.currentTarget.value)} spellcheck="false" /></div>`;
}

export function Toggle({ checked, onChange, label, help, disabled }) {
  return html`<label class=${'toggle' + (disabled ? ' disabled' : '')}>
    <input type="checkbox" checked=${!!checked} disabled=${disabled} onChange=${(e) => onChange(e.currentTarget.checked)} />
    <span class="track"></span>
    ${(label || help) && html`<span class="text">${label && html`<b>${t(label)}</b>`}${help && html`<span>${t(help)}</span>`}</span>`}
  </label>`;
}

export function Segmented({ value, options, onChange, full }) {
  return html`<div class=${'segmented' + (full ? ' full' : '')}>
    ${options.map(([v, l]) => html`<button class=${v === value ? 'on' : ''} onClick=${() => onChange(v)}>${t(l)}</button>`)}
  </div>`;
}

export function Tabs({ value, options, onChange }) {
  return html`<div class="tabs">${options.map(([v, l]) => html`<button class=${v === value ? 'on' : ''} onClick=${() => onChange(v)}>${t(l)}</button>`)}</div>`;
}

/** A slider; `format` turns the value into what is shown beside the label. */
export function Slider({ label, value, min, max, step, onChange, format }) {
  const v = Number(value ?? min);
  const fill = ((v - min) / (max - min || 1)) * 100;
  return html`<div class="slider">
    <div class="top"><span>${t(label)}</span><span>${format ? format(v) : v}</span></div>
    <input type="range" min=${min} max=${max} step=${step} value=${v} style=${`--fill:${fill}%`} onInput=${(e) => onChange(Number(e.currentTarget.value))} />
  </div>`;
}

export function Spinner() {
  return html`<span class="spinner"></span>`;
}

export function Empty({ icon = 'info', title, text, children }) {
  return html`<div class="empty"><${Icon} name=${icon} />${title && html`<b>${t(title)}</b>`}${text && html`<div>${t(text)}</div>`}${children}</div>`;
}

export function Notice({ kind = '', icon, children }) {
  return html`<div class=${'notice ' + kind}><${Icon} name=${icon || (kind ? 'alert' : 'info')} /><div class="grow">${children}</div></div>`;
}

export function Tag({ kind = '', children }) {
  return html`<span class=${'tag ' + kind}>${children}</span>`;
}

/** A dialog over the page; Escape and a click beside it close it. */
export function Dialog({ title, onClose, children, footer, wide }) {
  useEffect(() => {
    const k = (e) => e.key === 'Escape' && onClose?.();
    window.addEventListener('keydown', k);
    return () => window.removeEventListener('keydown', k);
  }, [onClose]);
  return html`<div class="overlay" onMouseDown=${(e) => e.target === e.currentTarget && onClose?.()}>
    <div class=${'dialog' + (wide ? ' wide' : '')} role="dialog">
      <div class="dialog-head"><h2>${t(title)}</h2><${Button} kind="ghost" icon="x" title="Close" onClick=${onClose} /></div>
      <div class="dialog-body">${children}</div>
      ${footer && html`<div class="dialog-foot">${footer}</div>`}
    </div>
  </div>`;
}

/** A question with two answers; resolves true for the first. */
export function useConfirm() {
  const [q, setQ] = useState(null);
  const ask = (opts) => new Promise((resolve) => setQ({ ...opts, resolve }));
  const node = q && html`<${Dialog} title=${q.title} onClose=${() => (q.resolve(false), setQ(null))} footer=${html`
      <${Button} label="Cancel" onClick=${() => (q.resolve(false), setQ(null))} />
      <${Button} kind=${q.danger ? 'danger' : 'primary'} label=${q.yes || 'OK'} onClick=${() => (q.resolve(true), setQ(null))} />`}>
    <p class="dim">${t(q.text)}</p>
  </${Dialog}>`;
  return [ask, node];
}

/** A picture with a placeholder icon while there is none (or it failed). */
export function Picture({ src, icon = 'map', class: cls = '', children, style = '' }) {
  const [ok, setOk] = useState(true);
  useEffect(() => setOk(true), [src]);
  if (!src || !ok) return html`<div class=${'pic placeholder ' + cls} style=${style}><${Icon} name=${icon} />${children}</div>`;
  return html`<div class=${'pic ' + cls} style=${`background-image:url("${src}");${style}`}>
    <img src=${src} style="display:none" onError=${() => setOk(false)} />${children}
  </div>`;
}

export function PageHead({ title, text, actions }) {
  return html`<header class="page-head"><div><h1>${t(title)}</h1>${text && html`<p>${t(text)}</p>`}</div>${actions && html`<div class="actions">${actions}</div>`}</header>`;
}
