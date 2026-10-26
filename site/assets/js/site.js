/* engram pages — no-framework behavior (job 004 DEC-P03, re-derived for the
 * D2 dark-only register in job 008, DEC-P01).
 * Progressive enhancement only: without this script the pages remain plain,
 * readable, selectable HTML in the single dark polarity.
 *  - copy affordance on code blocks (added by script; text stays selectable without it)
 *  - inert ?probe=1 diagnostics reporter for headless verification
 * The theme toggle, polarity forcing, diagram pair swap and localStorage
 * handling are gone: D2 ships dark-only with one SVG per diagram.
 */
(function () {
  'use strict';

  var ROOT = document.documentElement;
  var params = new URLSearchParams(window.location.search);
  var PROBE = params.get('probe') === '1';

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

  /* ---------- inert probe reporter (D2, single polarity) ---------- */

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
    var names = ['--paper', '--panel', '--chip', '--card', '--plate', '--ink',
                 '--muted', '--accent', '--link', '--pass', '--block',
                 '--code-bg', '--code-ink', '--line', '--line-ui'];
    var out = {};
    for (var i = 0; i < names.length; i++) {
      out[names[i]] = cs.getPropertyValue(names[i]).trim();
    }
    return out;
  }

  function probeContrast(tokens) {
    /* Component pairs mirroring check-contrast.py's witnessed DEC-P01 matrix:
     * the browser computes these from the same tokens so render-probe asserts
     * AA independently of the static gate (FIND-CODE-002 pattern, job 004).
     * Worst body pair block-on-plate (6.19:1) is included on purpose. */
    function rgb(name) { return 'rgb(' + hexToRgb(tokens[name]).join(',') + ')'; }
    return {
      'ink-on-paper': contrast(rgb('--ink'), rgb('--paper')),
      'ink-on-card': contrast(rgb('--ink'), rgb('--card')),
      'ink-on-plate': contrast(rgb('--ink'), rgb('--plate')),
      'muted-on-panel': contrast(rgb('--muted'), rgb('--panel')),
      'accent-on-paper': contrast(rgb('--accent'), rgb('--paper')),
      'accent-on-card': contrast(rgb('--accent'), rgb('--card')),
      'link-on-paper': contrast(rgb('--link'), rgb('--paper')),
      'link-on-card': contrast(rgb('--link'), rgb('--card')),
      'pass-on-paper': contrast(rgb('--pass'), rgb('--paper')),
      'block-on-card': contrast(rgb('--block'), rgb('--card')),
      'block-on-plate': contrast(rgb('--block'), rgb('--plate')),
      'paper-on-accent': contrast(rgb('--paper'), rgb('--accent')),
      'paper-on-pass': contrast(rgb('--paper'), rgb('--pass')),
      'paper-on-block': contrast(rgb('--paper'), rgb('--block')),
      'accent-on-chip': contrast(rgb('--accent'), rgb('--chip')),
      'code-ink-on-code-bg': contrast(rgb('--code-ink'), rgb('--code-bg')),
      'line-ui-on-paper': contrast(rgb('--line-ui'), rgb('--paper')),
      'line-ui-on-panel': contrast(rgb('--line-ui'), rgb('--panel'))
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
    var probe = {
      url: window.location.pathname,
      tokens: tokens,
      contrast: probeContrast(tokens),
      fonts: {
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
      copyButtons: document.querySelectorAll('button.code-copy').length
    };
    var pre = document.createElement('pre');
    pre.id = 'engram-probe';
    pre.textContent = 'ENGRAM-PROBE ' + JSON.stringify(probe);
    document.body.appendChild(pre);
  }

  function boot() {
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
