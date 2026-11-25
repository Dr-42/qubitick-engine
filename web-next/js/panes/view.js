/**
 * panes/view.js — View pane: the camera, the tiles, the theme and the field.
 *
 * This is the pane that answers "why does the canvas look like that": the zoom, the
 * frame rate and how many tiles are live/frozen/carded/culled; what a new tile does; and
 * which theme and blueprint the workspace is painted from.
 *
 * In:  {}   Out: {}   Call: `refresh`
 *
 * Every control is a call into `ctx.view`, one namespace for one subsystem — the camera,
 * the tile defaults, the theme board, the field's blueprint spaces and the banner stamp.
 * Grouping them is not cosmetic: a console pane is a list of controls over somebody
 * else's state, and the namespace is that statement. Nothing here owns a value; the
 * readouts are read back from the app on every paint, so the numbers on screen are the
 * numbers in force.
 *
 * The camera's vitals (`zoom`, `fps`, `lod`) update several times a second from the frame
 * loop, which calls the pane's `refresh` rather than re-mounting it — a pane re-mounted at
 * 2.5Hz would drop whatever the pointer was doing.
 */

import { callPort } from '../core/ports.js';
import { ctlRow, ctlSec, ctlBtn, ctlPill, ctlCheck, ctlGroup } from '../app/ctl.js';

export default {
  kind: 'view',
  title: 'View',
  ports: { in: {}, out: {}, call: { refresh: 'call' } },
  create(ctx) {
    const view = () => (ctx.view ? ctx.view : null);
    let zoom = null, fps = null, lod = null, themeId = null;
    let cLod = null, cSnap = null, cNewPin = null, cNewSel = null, cSyntax = null, bannerBtn = null, fieldBtns = null;
    let hostEl = null;

    /** Read every readout back from the app: the pane shows what is in force. */
    function paint() {
      const v = view();
      if (!v) return;
      if (zoom) zoom.textContent = v.zoomLabel();
      if (fps) fps.textContent = v.fps();
      if (lod) lod.textContent = v.lodLine();
      if (themeId) themeId.textContent = v.themeName();
      if (cLod) cLod.checked = !!v.tiles.lod();
      if (cSnap) cSnap.checked = v.tiles.snap() !== false;
      if (cNewPin) cNewPin.checked = !!v.tiles.newTilePin();
      if (cNewSel) cNewSel.checked = v.tiles.newTileSel() !== false;
      if (cSyntax) cSyntax.checked = v.tiles.syntax() !== false;
      if (bannerBtn) bannerBtn.textContent = v.banner.get().border ? '⊞' : '⊡';
      // the field buttons show which blueprint is pressed
      if (fieldBtns) {
        const cur = v.field.current();
        for (const b of fieldBtns.querySelectorAll('button[data-field]')) {
          b.setAttribute('aria-pressed', String(b.dataset.field === cur));
        }
      }
    }

    const callRefresh = callPort(() => { paint(); return { ok: true }; });

    /** A button that runs an app action and then re-reads the state it changed. */
    const act = (el, fn) => { el.addEventListener('click', () => { fn(); paint(); }); return el; };

    return {
      mount(host) {
        hostEl = host;
        const v = view();
        if (!v) { host.innerHTML = '<div class="empty">no view services</div>'; return; }

        const out = ctlBtn('b-zoom-out', '−', 'zoom out  (−)');
        const back = ctlBtn('h-zoom2', '100%', 'zoom — click to go back to 100%  (1)');
        back.className = 'hb-num';
        const zin = ctlBtn('b-zoom-in', '+', 'zoom in  (+)');
        const fit = ctlBtn('b-fit', 'fit all', 'frame every tile  (0)');
        const tidy = ctlBtn('b-tidy', 'tidy', 're-tile every cluster from its kinds');
        act(out, () => v.zoomNudge(1 / 1.25));
        act(zin, () => v.zoomNudge(1.25));
        act(back, () => v.zoom100());
        act(fit, () => v.frameAll());
        act(tidy, () => v.tidy());

        zoom = ctlPill('h-zoom', '100%');
        fps = ctlPill('h-perf', '–');
        lod = ctlPill('h-lod', '–');
        themeId = ctlPill('s-theme', v.themeName());
        const themeBtn = ctlBtn('b-themes', 'themes…', 'the theme board: every theme, as its own field');
        act(themeBtn, () => v.themeBoard());

        // the field: one pressed button per blueprint space
        const group = ctlGroup();
        for (const space of v.field.spaces()) {
          const b = ctlBtn('f-' + space.id, String(space.name || space.id).toLowerCase(), space.note || ('blueprint: ' + space.name));
          b.className = 'seg';
          b.dataset.field = space.id;
          b.addEventListener('click', () => { v.field.set(space.id); paint(); });
          group.appendChild(b);
        }
        fieldBtns = group;

        const banner = v.banner.get();
        const bRotL = ctlBtn('b-banner-ccw', '⟲', 'turn the banner 5° anticlockwise');
        const bRotR = ctlBtn('b-banner-cw', '⟳', 'turn the banner 5° clockwise');
        bannerBtn = ctlBtn('b-banner-border', banner.border ? '⊞' : '⊡', 'rule the banner: a plate and corner ticks, or no frame');
        act(bRotL, () => v.banner.turn(-5));
        act(bRotR, () => v.banner.turn(5));
        act(bannerBtn, () => v.banner.rule());

        cLod = ctlCheck('c-lod', 'on', !!v.tiles.lod());
        cSnap = ctlCheck('c-snap', 'on', v.tiles.snap() !== false);
        cNewPin = ctlCheck('c-newpin', 'pinned to its own file', !!v.tiles.newTilePin());
        cNewSel = ctlCheck('c-newsel', 'shares the selection', v.tiles.newTileSel() !== false);
        cSyntax = ctlCheck('c-syntax', 'syntax highlighting on', v.tiles.syntax() !== false);

        cLod.addEventListener('change', (e) => { v.tiles.lod(e.target.checked); paint(); });
        cSnap.addEventListener('change', (e) => { v.tiles.snap(e.target.checked); paint(); });
        cNewPin.addEventListener('change', (e) => { v.tiles.newTilePin(e.target.checked); paint(); });
        cNewSel.addEventListener('change', (e) => { v.tiles.newTileSel(e.target.checked); paint(); });
        cSyntax.addEventListener('change', (e) => { v.tiles.syntax(e.target.checked); paint(); });

        const sections = [
          ctlSec('camera', [ctlRow('zoom', ctlGroup(out, back, zin)), ctlRow('everything', ctlGroup(fit, tidy))]),
          ctlSec('vitals', [ctlRow('zoom', zoom), ctlRow('frames / fps', fps), ctlRow('live · frozen · card · culled', lod)]),
          ctlSec('tiles', [
            ctlRow('LOD', cLod),
            ctlRow('snap tiles', cSnap),
            ctlRow('new text tile', cNewPin),
            ctlRow('new tree / graph', cNewSel),
            ctlRow('new tile', cSyntax),
          ]),
          ctlSec('theme', [
            ctlRow('now', ctlGroup(themeId, themeBtn)),
            ctlRow('field', fieldBtns),
            ctlRow('banner', ctlGroup(bRotL, bRotR, bannerBtn)),
          ]),
        ];
        if (ctx.controlColumns) ctx.controlColumns(host, sections);
        else host.append(...sections);
        paint();
      },
      unmount() {
        zoom = fps = lod = themeId = null;
        cLod = cSnap = cNewPin = cNewSel = cSyntax = bannerBtn = fieldBtns = null;
        hostEl = null;
      },
      refresh: paint,
      in: {}, out: {},
      call: { refresh: callRefresh },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};