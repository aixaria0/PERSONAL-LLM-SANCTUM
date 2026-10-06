const $ = (id) => document.getElementById(id);

async function jsonFetch(url, options = {}) {
  const res = await fetch(url, options);
  const body = await res.json();
  if (!res.ok) throw new Error(body.error || `HTTP ${res.status}`);
  return body;
}

async function refreshHealth() {
  try {
    const health = await jsonFetch('/health');
    $('status').textContent = `ONLINE · ${health.models.length} models`;
    $('status').classList.add('ok');
    $('models').textContent = health.models.join('\n');
  } catch (err) {
    $('status').textContent = `OFFLINE · ${err.message}`;
    $('status').classList.remove('ok');
  }
}

$('send').addEventListener('click', async () => {
  const input = $('prompt').value.trim();
  if (!input) return;
  $('send').disabled = true;
  $('answer').textContent = 'Executing…';
  $('runMeta').textContent = '';
  $('traceBox').classList.add('hidden');
  try {
    const data = await jsonFetch('/v1/chat', {
      method: 'POST',
      headers: {'content-type': 'application/json'},
      body: JSON.stringify({input, mode: $('mode').value})
    });
    $('answer').textContent = data.final_answer;
    $('runMeta').textContent = `${data.path.toUpperCase()} · ${data.intent} · ${data.elapsed_ms} ms · ${data.run_id}`;
    $('trace').textContent = JSON.stringify({
      memory_hits: data.memory_hits,
      evidence: data.evidence,
      critique: data.critique,
      verification: data.verification
    }, null, 2);
    $('traceBox').classList.remove('hidden');
  } catch (err) {
    $('answer').textContent = `ERROR: ${err.message}`;
  } finally {
    $('send').disabled = false;
  }
});

$('saveMemory').addEventListener('click', async () => {
  const content = $('memory').value.trim();
  if (!content) return;
  try {
    const data = await jsonFetch('/v1/memory', {
      method: 'POST',
      headers: {'content-type': 'application/json'},
      body: JSON.stringify({content, tags: $('tags').value.trim()})
    });
    $('memoryStatus').textContent = `Saved memory #${data.id}`;
    $('memory').value = '';
  } catch (err) {
    $('memoryStatus').textContent = `ERROR: ${err.message}`;
  }
});

if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js').catch(() => {});
refreshHealth();
