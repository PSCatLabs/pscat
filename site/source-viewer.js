// "View source" for the gallery (#461).
//
// Every gallery picture has a link, <a class="view-source" href="source/...">,
// to the PostScript program that drew it, staged in _site/source/ by
// scripts/gallery.py. This script opens that program in the page's one
// <dialog id="source-dialog">: it fetches the file when the dialog opens (so
// the gallery page does not carry every program), shows it with line numbers
// in a scroll area, and lists the library files it loads, with links to them
// on GitHub. Without JavaScript, or in a browser with no <dialog>, the link
// simply goes to the file.
(function () {
  'use strict';
  var dialog = document.getElementById('source-dialog');
  if (!dialog || typeof dialog.showModal !== 'function') return;

  var blob = dialog.dataset.blob;                      // GitHub .../blob/main/
  var nameEl = document.getElementById('source-name');
  var metaEl = document.getElementById('source-meta');
  var githubEl = document.getElementById('source-github');
  var libsEl = document.getElementById('source-libs');
  var bodyEl = document.getElementById('source-body');
  var closeEl = document.getElementById('source-close');
  var root = document.documentElement;

  var opener = null;      // the link that opened the dialog: focus returns to it
  var ticket = 0;         // each open gets one; a late fetch for an old open is dropped
  var cache = {};         // url -> text, so reopening is instant

  function sizeText(bytes) {
    return bytes < 1024 ? bytes + ' bytes' : (bytes / 1024).toFixed(1) + ' KB';
  }

  function message(text, github) {
    bodyEl.textContent = '';
    var p = document.createElement('p');
    p.className = 'source-note';
    p.textContent = text;
    if (github) {
      var a = document.createElement('a');
      a.href = github;
      a.textContent = 'Read it on GitHub';
      p.appendChild(document.createTextNode(' '));
      p.appendChild(a);
    }
    bodyEl.appendChild(p);
  }

  // The same rule scripts/gallery.py uses for the line count: split on \n and
  // ignore the one that ends the file.
  function linesOf(text) {
    return text.replace(/\n$/, '').split('\n');
  }

  function show(text) {
    var lines = linesOf(text);
    var gutter = document.createElement('pre');
    gutter.className = 'source-gutter';
    gutter.setAttribute('aria-hidden', 'true');
    var nums = new Array(lines.length);
    for (var i = 0; i < lines.length; i++) nums[i] = i + 1;
    gutter.textContent = nums.join('\n');
    var pre = document.createElement('pre');
    pre.className = 'source-code';
    var code = document.createElement('code');
    code.textContent = lines.join('\n');
    pre.appendChild(code);
    bodyEl.textContent = '';
    bodyEl.appendChild(gutter);
    bodyEl.appendChild(pre);
  }

  function open(link) {
    var mine = ++ticket;
    opener = link;
    var d = link.dataset;
    nameEl.textContent = d.name;
    metaEl.textContent = d.lines + ' lines, ' + sizeText(Number(d.bytes));
    githubEl.href = d.github;
    libsEl.textContent = '';
    var libs = d.libs ? d.libs.split(' ') : [];
    libsEl.hidden = libs.length === 0;
    if (libs.length) {
      libsEl.appendChild(document.createTextNode('Loads: '));
      libs.forEach(function (path, i) {
        if (i) libsEl.appendChild(document.createTextNode(', '));
        var a = document.createElement('a');
        a.href = blob + path;
        a.textContent = path;
        libsEl.appendChild(a);
      });
    }
    message('Loading...');
    bodyEl.scrollTop = 0;
    bodyEl.scrollLeft = 0;
    root.classList.add('dialog-open');
    dialog.showModal();
    bodyEl.focus();     // arrow keys and Page Down scroll the source at once

    var url = link.getAttribute('href');
    var ready = cache[url] !== undefined ? Promise.resolve(cache[url]) :
      fetch(url).then(function (r) {
        if (!r.ok) throw new Error('HTTP ' + r.status);
        return r.text();
      }).then(function (t) { cache[url] = t; return t; });
    ready.then(function (text) {
      if (mine !== ticket) return;
      show(text);
      bodyEl.scrollTop = 0;
    }, function () {
      if (mine !== ticket) return;
      message('The source could not be loaded.', d.github);
    });
  }

  document.addEventListener('click', function (e) {
    var link = e.target.closest && e.target.closest('a.view-source');
    if (!link || e.defaultPrevented || e.button !== 0 ||
        e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    open(link);
  });

  closeEl.addEventListener('click', function () { dialog.close(); });
  // the dialog has no padding, so a click that lands on the dialog itself is
  // a click on the dimmed backdrop
  dialog.addEventListener('click', function (e) {
    if (e.target === dialog) dialog.close();
  });
  dialog.addEventListener('close', function () {
    if (dialog.open) return;    // reopened before this (asynchronous) event arrived
    ticket++;
    root.classList.remove('dialog-open');
    if (opener && document.contains(opener)) opener.focus();
    opener = null;
  });
})();
