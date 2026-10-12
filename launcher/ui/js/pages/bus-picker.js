// The bus picker: the buses in a list (search, makers, favourites), the chosen one turning in
// a 3D picture beside it with its paints and description.

import { html, useState, useMemo } from '../../vendor/preact-htm.js';
import { BusPreview } from '../preview.js';
import { S, useStore, set } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Dialog, Search, Select, Segmented, Empty, Spinner, Button, Tag, Notice, Field } from '../ui.js';
import { choose } from '../data.js';

const favKey = (file) => file.replace(/\\/g, '/').toLowerCase();

export function toggleFavourite(file) {
  const k = favKey(file);
  const list = S.favourites.includes(k) ? S.favourites.filter((x) => x !== k) : [...S.favourites, k];
  set({ favourites: list });
  call('favourites_save', { list }).catch(() => {});
}

export function BusPicker({ onClose }) {
  const s = useStore();
  const [q, setQ] = useState('');
  const [maker, setMaker] = useState('');
  const [show, setShow] = useState('all');
  const [sel, setSel] = useState(s.choice.bus);
  const [paint, setPaint] = useState(s.choice.bus === sel ? s.choice.paint : '');
  const all = s.vehicles || [];
  const makers = useMemo(() => [...new Set(all.map((v) => v.manufacturer).filter(Boolean))].sort(), [all]);
  const ql = q.trim().toLowerCase();
  const list = all
    .filter((v) => !maker || v.manufacturer === maker)
    .filter((v) => show !== 'fav' || s.favourites.includes(favKey(v.file)))
    .filter((v) => show !== 'mods' || v.installed)
    .filter((v) => !ql || `${v.name} ${v.manufacturer} ${v.type_name} ${v.folder}`.toLowerCase().includes(ql))
    .sort((a, b) => (a.manufacturer || '').localeCompare(b.manufacturer || '') || a.name.localeCompare(b.name));
  const v = all.find((x) => x.file === sel);
  const take = () => {
    choose({ bus: sel, paint, number: sel === s.choice.bus ? s.choice.number : '', hof_manual: sel === s.choice.bus ? s.choice.hof_manual : false });
    onClose();
  };
  return html`<${Dialog} wide title="Choose a bus" onClose=${onClose} footer=${html`
      <span class="muted small" style="margin-right:auto">${t('{n} buses', { n: list.length })}</span>
      <${Button} label="Cancel" onClick=${onClose} />
      <${Button} kind="primary" icon="check" label="Take this bus" disabled=${!v} onClick=${take} />`}>
    <div class="picker" style="margin:-18px -20px;height:calc(100% + 36px)">
      <div class="left">
        <div class="tools">
          <${Search} value=${q} onInput=${setQ} placeholder="Search buses" autofocus />
          <div class="row">
            <div class="grow"><${Select} raw value=${maker} options=${[['', t('All makers')], ...makers.map((m) => [m, m])]} onChange=${setMaker} /></div>
            <${Segmented} value=${show} options=${[['all', 'All'], ['fav', 'Starred'], ['mods', 'Mods']]} onChange=${setShow} />
          </div>
        </div>
        <div class="list">
          ${list.length === 0 && html`<${Empty} icon="bus" title=${show === 'fav' ? 'No starred bus' : 'No bus found'} text=${show === 'fav' ? 'Star a bus to find it here.' : null} />`}
          ${list.map((x) => html`<div class=${'list-item' + (x.file === sel ? ' on' : '')} onClick=${() => (setSel(x.file), setPaint(x.file === s.choice.bus ? s.choice.paint : ''))} onDblClick=${() => (setSel(x.file), setTimeout(take))}>
            <div class="grow" style="min-width:0">
              <div class="title ellipsis">${x.name}</div>
              <div class="sub ellipsis">${[x.manufacturer, x.paints.length ? t('{n} paints', { n: x.paints.length + 1 }) : null].filter(Boolean).join(' - ')}</div>
            </div>
            ${x.missing_packs?.length > 0 && html`<span title=${t('Parts missing')}><${Icon} name="alert" size=${16} /></span>`}
            <button class=${'star' + (s.favourites.includes(favKey(x.file)) ? ' on' : '')} title=${t('Star')} onClick=${(e) => (e.stopPropagation(), toggleFavourite(x.file))}>
              <${Icon} name="star" fill=${s.favourites.includes(favKey(x.file))} />
            </button>
          </div>`)}
        </div>
      </div>
      <div class="right">
        ${v ? html`<${BusPreview} look=${{ bus: v.file, paint, map: s.choice.map, weather: s.choice.weather, time: s.choice.time, date: s.choice.date }} />
          <div class="card-body stack">
            <div class="row">
              <div class="grow"><h2 style="font-size:20px">${v.name}</h2><div class="muted">${[v.manufacturer, v.type_name].filter(Boolean).join(' - ')}</div></div>
              ${v.installed && html`<${Tag}>${t('Mod')}</${Tag}>`}
            </div>
            ${v.missing_packs?.length > 0 && html`<${Notice} kind="warn">${t('Parts of this bus come from packs that are not installed: {packs}', { packs: v.missing_packs.join(', ') })}</${Notice}>`}
            <${Field} label="Paint">
              <${Select} raw value=${paint} options=${[['', v.default_paint ? `${t('Standard')} (${v.default_paint})` : t('Standard')], ...v.paints.map((p) => [p, p])]} onChange=${setPaint} />
            </${Field}>
            ${v.description && html`<p class="dim selectable" style="white-space:pre-line;max-height:220px;overflow:auto">${v.description}</p>`}
            <div class="muted small selectable">${v.file}</div>
          </div>`
          : html`<${Empty} icon="bus" title="Choose a bus on the left" />`}
      </div>
    </div>
  </${Dialog}>`;
}

