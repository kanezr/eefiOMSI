// Mods: installing a bus, a map or a whole folder into the content folder (the original
// OMSI 2 folder is never written to), the installs under way, and the installed mods.

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { useStore, toast } from '../store.js';
import { t, bytes, ago } from '../i18n.js';
import { call, onDrop } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Card, Empty, PageHead, Search, Select, Spinner, Tag, Toggle, Notice, useConfirm } from '../ui.js';
import { pollNow, loadContent, cached } from '../data.js';

const KIND = { bus: ['bus', 'Bus'], map: ['map', 'Map'], archive: ['folder', 'Archive'], other: ['puzzle', 'Other'] };
const ACTIVE = (j) => !['done', 'failed', 'cancelled'].includes(j.state);

export function ModsPage() {
  const s = useStore();
  const status = s.cache.mods ?? null;
  const [mode, setMode] = useState('auto');
  const [over, setOver] = useState(false);
  const [q, setQ] = useState('');
  const [open, setOpen] = useState(null);
  const [ask, confirm] = useConfirm();
  const read = () => cached('mods', 'mods_status');
  useEffect(() => {
    read();
  }, []);
  const install = async (path) => {
    try {
      await call('mod_install', { path, mode });
      toast(t('Installing {name}…', { name: path.split(/[\\/]/).pop() }));
      pollNow();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  useEffect(() => onDrop((paths) => paths.forEach(install), setOver), [mode]);
  const pick = async (archive) => {
    const p = await call('pick_mod', { archive }).catch(() => null);
    if (p) install(p);
  };
  const jobs = s.poll.jobs || [];
  const mods = (status?.installed || []).filter((m) => !q || `${m.name} ${m.paths.join(' ')}`.toLowerCase().includes(q.toLowerCase()));
  const toggle = async (m, on) => {
    try {
      await call('mod_set_enabled', { id: m.id, on });
      toast(t(on ? '{name} switched on' : '{name} switched off', { name: m.name }), 'ok');
      read();
      loadContent();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const remove = async (m) => {
    if (!(await ask({ title: 'Delete this mod?', text: t('{name} and its files are deleted from the content folder. Switch it off instead to keep it.', { name: m.name }), yes: 'Delete', danger: true }))) return;
    try {
      await call('mod_remove', { id: m.id });
      toast(t('{name} deleted', { name: m.name }), 'ok');
      read();
      loadContent();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  return html`
    <${PageHead} title="Mods" text="A bus, a map, scenery, a whole OMSI folder - as a folder or a .zip, .7z or .rar. The original OMSI 2 folder is never written to." />
    <div class="drive" style="grid-template-columns:minmax(0,1fr) 360px">
      <div class="stack">
        ${status?.error && html`<${Notice} kind="danger">${status.error}</${Notice}>`}
        <${Card} title="Installed mods" actions=${html`<div style="width:240px"><${Search} value=${q} onInput=${setQ} placeholder="Search mods..." /></div>`}>
          ${!status
            ? html`<div class="row muted"><${Spinner} /> ${t('Reading the content folder...')}</div>`
            : mods.length === 0
              ? html`<${Empty} icon="puzzle" text=${q ? 'No mod matches.' : 'No mods yet. Choose a folder or an archive on the right, or drop one onto the window.'} />`
              : html`<div class="list" style="margin:0 -8px">
                  ${mods.map((m) => html`<div>
                    <div class="list-item" onClick=${() => setOpen(open === m.id ? null : m.id)}>
                      <${Icon} name=${KIND[m.kind]?.[0] || 'puzzle'} />
                      <div class="grow" style="min-width:0">
                        <div class="title ellipsis" style=${m.enabled ? '' : 'opacity:.55'}>${m.name}</div>
                        <div class="sub">${t(KIND[m.kind]?.[1] || 'Other')} - ${bytes(m.bytes)}${m.installed ? ` - ${ago(m.installed)}` : ''}</div>
                      </div>
                      <div onClick=${(e) => e.stopPropagation()}><${Toggle} checked=${m.enabled} onChange=${(on) => toggle(m, on)} /></div>
                      <${Button} small kind="ghost" icon="trash" title="Delete this mod (asks first; switch it off instead to keep it)" onClick=${(e) => (e.stopPropagation(), remove(m))} />
                    </div>
                    ${open === m.id && html`<div style="padding:4px 12px 12px 44px" class="muted small selectable">${m.paths.map((p) => html`<div>${p}</div>`)}</div>`}
                  </div>`)}
                </div>`}
        </${Card}>
        ${status?.waiting?.length > 0 && html`<${Card} title="Waiting for their bus" hint="Packs installed before the bus they belong to: they go in once it is there.">
          <div class="row wrap">${status.waiting.map((w) => html`<${Tag}>${w}</${Tag}>`)}</div>
        </${Card}>`}
        ${status?.archives?.length > 0 && html`<${Card} title="Archives" hint="Read in place by the game, without unpacking.">
          <table class="table"><tbody>${status.archives.map(([n, b]) => html`<tr><td>${n}</td><td class="num muted" style="text-align:right">${bytes(b)}</td></tr>`)}</tbody></table>
        </${Card}>`}
      </div>
      <div class="side">
        <${Card} title="Install a mod">
          <div class="stack">
            <div class="grid-2" style="gap:8px">
              <${Button} icon="folder" label="Choose a folder" onClick=${() => pick(false)} />
              <${Button} icon="download" label="Choose archive" onClick=${() => pick(true)} />
            </div>
            <div class="field"><span class="label">${t('Archive install mode')}</span>
              <${Select} value=${mode} options=${[['auto', 'Auto'], ['extract', 'Unpacked'], ['inplace', 'Used in place']]} onChange=${setMode} />
            </div>
            <div class=${'drop-zone' + (over ? ' over' : '')}><${Icon} name="download" size=${26} /><div style="margin-top:6px">${t('Drop a folder or an archive here')}</div></div>
            ${status && html`<div class="muted small">${t('Free space: {n}', { n: bytes(status.free_bytes) })}</div>`}
          </div>
        </${Card}>
        <${Card} title="Installs">
          ${jobs.length === 0
            ? html`<p class="muted small">${t('Nothing installed since the launcher started. Big archives are checked against the free disk space before anything is unpacked; a cancelled or failed install leaves nothing behind.')}</p>`
            : html`<div class="stack">${jobs.slice().reverse().map((j) => html`<${Job} j=${j} />`)}</div>`}
        </${Card}>
        ${status && html`<${Card} title="The Mods folder">
          <div class="stack" style="gap:10px">
            <p class="muted small">${t('Anything put into {} is installed by itself once it has finished copying.', [t('this folder')])}</p>
            <div class="row"><code class="small selectable grow ellipsis">${status.inbox}</code><${Button} small icon="folder" title="Open" onClick=${() => call('open_path', { path: status.inbox })} /></div>
            ${status.inbox_items?.length > 0 && html`<div class="muted small">${t('In it now: {}', [status.inbox_items.join(', ')])}</div>`}
          </div>
        </${Card}>`}
      </div>
    </div>
    ${confirm}
  `;
}

function Job({ j }) {
  const pct = j.bytes_total ? (j.bytes_done / j.bytes_total) * 100 : j.files_total ? (j.files_done / j.files_total) * 100 : 0;
  const active = ACTIVE(j);
  return html`<div class="stack" style="gap:6px">
    <div class="row">
      ${active ? html`<${Spinner} />` : html`<span class=${'dot ' + (j.state === 'done' ? 'ok' : 'danger')}></span>`}
      <b class="grow ellipsis">${j.name}</b>
      <span class="muted small">${t(j.state)}</span>
    </div>
    ${active && html`<div class="progress"><div style=${`width:${pct.toFixed(1)}%`}></div></div>`}
    ${j.message && html`<div class=${'small ' + (j.state === 'failed' ? '' : 'muted')} style=${j.state === 'failed' ? 'color:var(--danger)' : ''}>${j.message}</div>`}
    ${j.warnings?.length > 0 && html`<div class="small" style="color:var(--warn)">${j.warnings.join(' - ')}</div>`}
  </div>`;
}
