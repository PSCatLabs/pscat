/* The renderer and PS programs come from the generated, local runtime bundle.
 * All artwork is executed by PSCat. JS only configures, steps, and displays it.
 */
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const canvas = $('canvas'), status = $('status');
  let renderer, generation = 0, mode = location.hash === '#press' ? 'press' : 'terrain';
  const controls = ['render', 'watch'];
  const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
  function message(text, error = false) {
    status.textContent = text;
    status.classList.toggle('error', error);
  }
  function word(id) {
    const text = $(id).value.trim().toUpperCase();
    if (!/^[A-Z]{1,10}$/.test(text)) throw new Error('Use one to ten A–Z letters in each active word field.');
    $(id).value = text;
    return text;
  }
  function literal(text) { return '(' + text.replace(/[\\()]/g, '\\$&') + ')'; }
  function configured() {
    if (mode === 'terrain') {
      const words = ['word-a', 'word-b', 'word-c'].map(word);
      const setup = '/TypeStyle ' + Number($('palette').value) + ' def\n' +
        words.map((text, i) => '/Word' + 'ABC'[i] + ' ' + literal(text) + ' def').join('\n');
      return {width:960, height:1200, setup, body:window.PSCAT_STUDIO_BUNDLE.programs.terrain,
        command:"target/ci/pscat --page 960x1200 --png type.png -e\n'" + setup + "\n(examples/functional/type-terrain.ps) run'"};
    }
    const setup = '/Inspect ' + $('inspect').checked + ' def\n/InjectFault ' + $('inject-fault').checked +
      ' def\n/ProofText ' + literal(word('proof-text')) + ' def';
    return {width:1280, height:960, setup, body:window.PSCAT_STUDIO_BUNDLE.programs.press,
      command:"target/ci/pscat --page 1280x960 --png proof.png -e\n'" + setup + "\n(examples/functional/observable-press.ps) run'"};
  }
  function view() {
    const press = mode === 'press';
    $('terrain-fields').hidden = press; $('press-fields').hidden = !press;
    $('terrain-explanation').hidden = press; $('press-explanation').hidden = !press;
    $('work-title').textContent = press ? 'THE OBSERVABLE PRESS' : 'TYPE BECOMES TERRAIN';
    document.querySelectorAll('[data-mode]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.mode === mode)));
  }
  async function render(watch = false) {
    if (!renderer) return;
    const ticket = ++generation;
    let job;
    try { job = configured(); } catch (error) {
      message(error.message, true); $('stop').disabled = true;
      document.documentElement.dataset.renderState = 'invalid'; return;
    }
    $('source').value = job.setup + '\n' + job.body;
    $('native-command').textContent = job.command;
    $('download').disabled = true; $('stop').disabled = false;
    $('stdout').textContent = ''; message(watch ? 'Drawing the score, one execution slice at a time…' : 'Rendering actual PostScript…');
    try {
      const started = performance.now();
      renderer.begin($('source').value, job.width, job.height);
      let more;
      do {
        await frame();
        if (ticket !== generation) return;
        more = renderer.step(watch ? 2500 : 50000);
        if (watch || more !== 1) {
          renderer.paintTo(canvas);
          canvas.hidden = false; $('fallback').hidden = true;
        }
        $('stdout').textContent = renderer.output || 'Executing…';
      } while (more === 1);
      if (more === -1) throw new Error(renderer.error);
      if (renderer.warnings.length) throw new Error(renderer.warnings.join('\n'));
      const elapsed = Math.round(performance.now() - started);
      const label = mode === 'terrain' ? 'Type becomes terrain' : 'The observable press';
      message('Rendered ' + job.width + ' × ' + job.height + ' · ' + elapsed + ' ms browser elapsed.');
      $('caption').textContent = label + ' · actual live PSCat WASM output · ' + job.width + ' × ' + job.height;
      $('download').disabled = false;
      document.documentElement.dataset.renderState = 'complete';
    } catch (error) {
      renderer.paintTo(canvas); canvas.hidden = false; $('fallback').hidden = true;
      message(error.message, true); document.documentElement.dataset.renderState = 'error';
    } finally {
      if (ticket === generation) $('stop').disabled = true;
    }
  }
  function stop() {
    ++generation;
    if (renderer) renderer.paintTo(canvas);
    $('stop').disabled = true;
    message('Stopped. The canvas contains a partial render; render again to restart.');
    document.documentElement.dataset.renderState = 'stopped';
  }
  $('controls').addEventListener('submit', event => {event.preventDefault(); document.documentElement.dataset.renderState = 'running'; render();});
  $('watch').addEventListener('click', () => {document.documentElement.dataset.renderState = 'running'; render(true);});
  $('stop').addEventListener('click', stop);
  document.querySelectorAll('[data-mode]').forEach(button => button.addEventListener('click', () => {
    ++generation; mode = button.dataset.mode; view(); document.documentElement.dataset.renderState = 'running'; render();
  }));
  $('download').addEventListener('click', () => canvas.toBlob(blob => {
    if (!blob) return;
    const url = URL.createObjectURL(blob), link = document.createElement('a');
    link.href = url; link.download = mode === 'terrain' ? 'type-terrain.png' : 'observable-press.png';
    link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
  }, 'image/png'));
  view();
  (async () => {
    try {
      renderer = await window.createPSCatStudio();
      controls.forEach(id => $(id).disabled = false);
      document.documentElement.dataset.renderState = 'running';
      await render();
    } catch (error) {
      message('Live renderer unavailable: ' + error.message + ' Saved examples are available in the introduction.', true);
      document.documentElement.dataset.renderState = 'error';
    }
  })();
})();
