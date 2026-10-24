// Drag-to-reorder for a list of rows, driven by touch/pointer events.
//
// Arguments: the container's id, the CSS class of its rows, the CSS class of
// a drag handle inside each row (or null: then a long press on the row starts
// the drag), and the long-press time in ms. When a row is dropped somewhere
// else the page sends `[from, to]` (row positions) back to Rust through
// `dioxus.send`; Rust reorders its list and re-renders. The helper only moves
// rows visually while dragging and puts everything back on drop.
function (containerId, rowClass, handleClass, longPressMs) {
  function bind(tries) {
    var c = document.getElementById(containerId);
    if (!c) {
      if (tries < 60) setTimeout(function () { bind(tries + 1); }, 50);
      return;
    }
    if (c.__reorderBound) return;
    c.__reorderBound = true;

    var st = null;            // the drag in progress
    var pending = null;       // a long press that has not fired yet
    var timer = null;
    var ticker = null;
    var suppressClicksUntil = 0;

    function rowsOf() {
      return Array.prototype.slice.call(c.querySelectorAll('.' + rowClass));
    }

    function scrollParentOf(el) {
      var p = el.parentElement;
      while (p) {
        var o = getComputedStyle(p).overflowY;
        if ((o === 'auto' || o === 'scroll') && p.scrollHeight > p.clientHeight) return p;
        p = p.parentElement;
      }
      return document.scrollingElement || document.documentElement;
    }

    function start(row, clientY, pointerId) {
      var rows = rowsOf();
      var idx = rows.indexOf(row);
      if (idx < 0) return;
      var sp = scrollParentOf(c);
      var rects = rows.map(function (r) {
        var b = r.getBoundingClientRect();
        return { top: b.top, h: b.height };
      });
      st = {
        row: row, idx: idx, rows: rows, rects: rects, sp: sp,
        startY: clientY, lastY: clientY, startScroll: sp.scrollTop, target: idx,
        // Space between two neighbouring rows, to shift the others by one slot.
        space: rects.length > 1 ? Math.max(0, (rects[1].top - rects[0].top) - rects[0].h) : 0
      };
      row.classList.add('reorder-dragging');
      c.classList.add('reorder-active');
      try { row.setPointerCapture(pointerId); } catch (e) {}
      if (navigator.vibrate) { try { navigator.vibrate(15); } catch (e) {} }
      ticker = setInterval(tick, 16);
    }

    function tick() {
      if (!st) return;
      // Scroll when the finger is near the top or bottom edge.
      var vh = window.innerHeight;
      if (st.lastY < 110) st.sp.scrollTop -= 14;
      else if (st.lastY > vh - 170) st.sp.scrollTop += 14;

      var dy = (st.lastY - st.startY) + (st.sp.scrollTop - st.startScroll);
      st.row.style.transform = 'translateY(' + dy + 'px)';

      var h = st.rects[st.idx].h;
      var centre = st.rects[st.idx].top + h / 2 + dy;
      var target = 0;
      for (var i = 0; i < st.rects.length; i++) {
        if (i === st.idx) continue;
        if (st.rects[i].top + st.rects[i].h / 2 < centre) target++;
      }
      st.target = target;

      var slot = h + st.space;
      for (var j = 0; j < st.rows.length; j++) {
        if (j === st.idx) continue;
        var shift = 0;
        if (st.idx < st.target && j > st.idx && j <= st.target) shift = -slot;
        else if (st.idx > st.target && j >= st.target && j < st.idx) shift = slot;
        st.rows[j].style.transform = shift ? 'translateY(' + shift + 'px)' : '';
      }
    }

    function finish(send) {
      clearTimeout(timer);
      pending = null;
      if (!st) return;
      clearInterval(ticker);
      var from = st.idx, to = st.target;
      st.rows.forEach(function (r) {
        r.style.transition = 'none';
        r.style.transform = '';
        r.classList.remove('reorder-dragging');
      });
      c.classList.remove('reorder-active');
      var rows = st.rows;
      st = null;
      // Let the reset paint without an animation, then re-enable transitions.
      requestAnimationFrame(function () {
        rows.forEach(function (r) { r.style.transition = ''; });
      });
      suppressClicksUntil = Date.now() + 400;
      if (send && from !== to) dioxus.send([from, to]);
    }

    c.addEventListener('pointerdown', function (e) {
      if (st) return;
      var row = e.target.closest('.' + rowClass);
      if (!row || !c.contains(row)) return;
      if (handleClass) {
        if (!e.target.closest('.' + handleClass)) return;
        e.preventDefault();
        start(row, e.clientY, e.pointerId);
        return;
      }
      if (e.target.closest('button, input, label, a, select, textarea')) return;
      pending = { x: e.clientX, y: e.clientY, row: row, id: e.pointerId };
      clearTimeout(timer);
      timer = setTimeout(function () {
        if (!pending) return;
        var p = pending;
        pending = null;
        start(p.row, p.y, p.id);
      }, longPressMs);
    });

    c.addEventListener('pointermove', function (e) {
      if (st) {
        st.lastY = e.clientY;
        e.preventDefault();
      } else if (pending) {
        if (Math.abs(e.clientX - pending.x) + Math.abs(e.clientY - pending.y) > 10) {
          clearTimeout(timer);
          pending = null;
        }
      }
    });

    c.addEventListener('pointerup', function () { finish(true); });
    c.addEventListener('pointercancel', function () { finish(false); });

    // Once a drag has started the page must not scroll under the finger.
    c.addEventListener('touchmove', function (e) {
      if (st) e.preventDefault();
    }, { passive: false });

    // A drag ends with a click on the row: swallow it so it does not open the row.
    c.addEventListener('click', function (e) {
      if (Date.now() < suppressClicksUntil) {
        e.stopPropagation();
        e.preventDefault();
      }
    }, true);

    // No text-selection / context menu popup on a long press.
    c.addEventListener('contextmenu', function (e) {
      if (st || pending) e.preventDefault();
    });
  }
  bind(0);
}
