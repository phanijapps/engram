/* engram pages — no-framework behavior (job 004, plan DEC-P03).
 * Progressive enhancement only: without this script the pages remain plain,
 * readable, selectable HTML in the light polarity.
 *  - theme toggle: data-theme on <html>, aria-pressed, localStorage, light default
 *  - copy affordance on code blocks (added by script; text stays selectable without it)
 *  - diagram polarity swap (DEC-P004 fallback: data-light-src / data-dark-src)
 *  - inert ?probe=1 diagnostics reporter for headless verification
 */
(function () {
  'use strict';

  var ROOT = document.documentElement;
  var STORE_KEY = 'engram-theme';
  var params = new URLSearchParams(window.location.search);
  var PROBE = params.get('probe') === '1';
  var FORCED = params.get('polarity') === 'dark' || params.get('polarity') === 'light'
    ? params.get('polarity') : null;

  /* ---------- theme ---------- */

  function currentTheme() {
    return ROOT.getAttribute('data-theme') === 'dark' ? 'dark' : 'light';
  }

  function applyTheme(theme, opts) {
    ROOT.setAttribute('data-theme', theme);
    try {
      if (opts && opts.persist) {
        window.localStorage.setItem(STORE_KEY, theme);
      }
      /* Forced polarity (?polarity=) is session-only by design: it never
       * writes and never wipes the stored preference (FIND-CODE-003 /
       * FIND-UX-004 — a harness parameter must not delete a visitor's
       * saved theme). */
    } catch (e) { /* storage unavailable — polarity still applies in-session */ }
    var button = document.querySelector('[data-theme-toggle]');
    if (button) {
      button.setAttribute('aria-pressed', theme === 'dark' ? 'true' : 'false');
      var label = button.querySelector('[data-theme-label]');
      if (label) {
        label.textContent = theme === 'dark' ? 'light' : 'dark';
      }
    }
    swapDiagrams(theme);
    return theme;
  }

  function swapDiagrams(theme) {
    var imgs = document.querySelectorAll('img[data-light-src][data-dark-src]');
    for (var i = 0; i < imgs.length; i++) {
      var img = imgs[i];
      img.src = theme === 'dark' ? img.getAttribute('data-dark-src')
                                 : img.getAttribute('data-light-src');
    }
  }

  function initTheme() {
    var stored = null;
    try { stored = window.localStorage.getItem(STORE_KEY); } catch (e) { /* ignore */ }
    var initial = FORCED || (stored === 'dark' ? 'dark' : 'light');
    applyTheme(initial);
    var button = document.querySelector('[data-theme-toggle]');
    if (button) {
      button.addEventListener('click', function () {
        applyTheme(currentTheme() === 'dark' ? 'light' : 'dark', { persist: true });
      });
    }
  }

  /* ---------- code copy affordance (progressive) ---------- */

  function initCopy() {
    var pres = document.querySelectorAll('main pre');
    for (var i = 0; i < pres.length; i++) {
      (function (pre) {
        if (pre.id === 'engram-probe') { return; }
        var wrapper = document.createElement('div');
        wrapper.className = 'code-block';
        pre.parentNode.insertBefore(wrapper, pre);
        wrapper.appendChild(pre);
        var button = document.createElement('button');
        button.type = 'button';
        button.className = 'code-copy';
        button.setAttribute('aria-label', 'Copy code to clipboard');
        button.textContent = 'copy';
        button.addEventListener('click', function () {
          var text = pre.innerText;
          var done = function () {
            button.textContent = 'copied';
            window.setTimeout(function () { button.textContent = 'copy'; }, 1600);
          };
          if (navigator.clipboard && navigator.clipboard.writeText) {
            navigator.clipboard.writeText(text).then(done, function () { fallback(); });
          } else {
            fallback();
          }
          function fallback() {
            var range = document.createRange();
            range.selectNodeContents(pre);
            var sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(range);
            done();
          }
        });
        wrapper.appendChild(button);
      })(pres[i]);
    }
  }

  /* ---------- inert probe reporter ---------- */

  function luminance(channel) {
    var c = channel / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  }

  function relativeLuminance(rgb) {
    var m = rgb.match(/^rgba?\((\d+),\s*(\d+),\s*(\d+)/);
    if (!m) { return null; }
    return 0.2126 * luminance(+m[1]) + 0.7152 * luminance(+m[2]) + 0.0722 * luminance(+m[3]);
  }

  function contrast(fg, bg) {
    var a = relativeLuminance(fg);
    var b = relativeLuminance(bg);
    if (a === null || b === null) { return null; }
    var hi = Math.max(a, b);
    var lo = Math.min(a, b);
    return Math.round(((hi + 0.05) / (lo + 0.05)) * 100) / 100;
  }

  function probeTokens() {
    var cs = window.getComputedStyle(ROOT);
    var names = ['--paper', '--ink', '--muted', '--line', '--accent', '--link', '--pass',
                 '--card', '--code-bg'];
    var out = {};
    for (var i = 0; i < names.length; i++) {
      out[names[i]] = cs.getPropertyValue(names[i]).trim();
    }
    return out;
  }

  function probeContrast(tokens) {
    /* Component pairs mirroring check-contrast.py's witnessed matrix: the
     * browser computes these from the same tokens so render-probe asserts
     * AA independently of the static gate (FIND-CODE-002). */
    function rgb(name) { return 'rgb(' + hexToRgb(tokens[name]).join(',') + ')'; }
    var paper = rgb('--paper'), ink = rgb('--ink'), accent = rgb('--accent'),
        link = rgb('--link'), card = rgb('--card'), pass = rgb('--pass');
    return {
      'body-ink-on-paper': contrast(ink, paper),
      'muted-on-paper': contrast(rgb('--muted'), paper),
      'accent-on-paper': contrast(accent, paper),
      'link-on-paper': contrast(link, paper),
      'tag-paper-on-ink': contrast(paper, ink),
      'tag-acc-paper-on-accent': contrast(paper, accent),
      'tag-pass-card-on-pass': contrast(card, pass),
      'line-boundary-on-paper': contrast(rgb('--line'), paper)
    };
  }

  function hexToRgb(hex) {
    var h = hex.replace('#', '');
    if (h.length === 3) {
      h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
    }
    return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
  }

  function runProbe() {
    var tokens = probeTokens();
    var entries = window.performance.getEntriesByType('resource') || [];
    var origins = {};
    for (var i = 0; i < entries.length; i++) {
      try {
        origins[new URL(entries[i].name, window.location.href).origin] = true;
      } catch (e) { origins['unparseable:' + entries[i].name] = true; }
    }
    var button = document.querySelector('[data-theme-toggle]');
    var diagrams = [];
    var imgs = document.querySelectorAll('img[data-light-src][data-dark-src]');
    for (var j = 0; j < imgs.length; j++) {
      diagrams.push({
        light: imgs[j].getAttribute('data-light-src'),
        dark: imgs[j].getAttribute('data-dark-src'),
        current: imgs[j].currentSrc || imgs[j].src
      });
    }
    var probe = {
      url: window.location.pathname,
      polarity: currentTheme(),
      forced: FORCED,
      forcedStoragePreserved: (function () {
        try { return window.localStorage.getItem(STORE_KEY) === 'dark'; } catch (e) { return null; }
      })(),
      toggle: {
        ariaPressed: button ? button.getAttribute('aria-pressed') : null,
        stored: (function () { try { return window.localStorage.getItem(STORE_KEY); } catch (e) { return null; } })()
      },
      tokens: tokens,
      contrast: probeContrast(tokens),
      fonts: {
        sourceSerif4: document.fonts.check('600 24px "Source Serif 4"'),
        inter400: document.fonts.check('400 16px Inter'),
        inter600: document.fonts.check('600 16px Inter'),
        jetbrainsMono400: document.fonts.check('400 13px "JetBrains Mono"'),
        jetbrainsMono700: document.fonts.check('700 13px "JetBrains Mono"')
      },
      overflow: {
        documentScrollWidth: document.documentElement.scrollWidth,
        documentClientWidth: document.documentElement.clientWidth,
        bodyScrollWidth: document.body ? document.body.scrollWidth : null,
        hasHorizontalOverflow: document.documentElement.scrollWidth >
          document.documentElement.clientWidth
      },
      resourceOrigins: Object.keys(origins),
      sameOriginOnly: Object.keys(origins).every(function (o) { return o === window.location.origin; }),
      copyButtons: document.querySelectorAll('button.code-copy').length,
      diagrams: diagrams
    };
    var pre = document.createElement('pre');
    pre.id = 'engram-probe';
    pre.textContent = 'ENGRAM-PROBE ' + JSON.stringify(probe);
    document.body.appendChild(pre);
  }

  function boot() {
    if (PROBE && FORCED) {
      /* Regression sentinel for the forced-polarity storage rule: seed a
       * stored preference BEFORE init, then report whether it survived —
       * the old behavior (removeItem under ?polarity=) reports false. */
      try { window.localStorage.setItem(STORE_KEY, 'dark'); } catch (e) { /* ignore */ }
    }
    initTheme();
    initCopy();
    if (PROBE) {
      if (document.fonts && document.fonts.ready && document.fonts.ready.then) {
        document.fonts.ready.then(function () {
          window.setTimeout(runProbe, 350);
        });
      } else {
        window.setTimeout(runProbe, 600);
      }
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
