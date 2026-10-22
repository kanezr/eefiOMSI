// Tutorials: OMSI 2's four lessons, each started in its own situation.

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Button, Card, Empty, PageHead, Spinner } from '../ui.js';
import { start, cached } from '../data.js';
import { useStore } from '../store.js';

export function TutorialsPage() {
  const s = useStore();
  const list = Array.isArray(s.cache.tutorials) ? s.cache.tutorials : s.cache.tutorials ? [] : null;
  useEffect(() => {
    cached('tutorials', 'tutorials');
  }, []);
  return html`
    <${PageHead} title="Tutorials" text="OMSI 2's lessons: driving the bus, the stops, the ticket printer and the timetable." />
    ${list == null && html`<div class="row muted"><${Spinner} /> ${t('Reading the tutorials…')}</div>`}
    ${list?.length === 0 && html`<${Card}><${Empty} icon="book" title="No tutorial found" text="The tutorials come with OMSI 2 (its Tutorials folder)." /></${Card}>`}
    <div class="grid-2">
      ${(list || []).map((x) => html`<${Card} title=${`${x.number}. ${x.title}`}>
        <div class="stack">
          <p class="dim" style="white-space:pre-line;max-height:180px;overflow:auto">${x.text}</p>
          <div><${Button} kind="primary" icon="play" label="Start the tutorial" onClick=${() => start({ tutorial: x.number })} /></div>
        </div>
      </${Card}>`)}
    </div>
  `;
}
