// Activate only fixed, generated controls (tabs and flow connectors); no model code or host bridge is evaluated.
// 仅激活固定生成的控件（页签与流程连线）；不求值模型代码，也不调用宿主桥接。
(() => {
  // Each tab group owns its controls and panels, so activation cannot cross component boundaries.
  // 每个页签组拥有自身控件及面板，因此激活操作不会跨越组件边界。
  for (const group of document.querySelectorAll('.tabs')) {
    // The renderer creates direct-child tab lists and panels with paired IDs.
    // 渲染器创建直接子级页签列表与具有配对标识的面板。
    const tabs = Array.from(group.querySelectorAll('[role=tab]'));
    // Select the supplied local tab and optionally focus it; returns no value.
    // 选择所提供的局部页签，并按需聚焦；不返回值。
    const activate = (selected, focus) => {
      for (const tab of tabs) {
        const active = tab === selected;
        tab.setAttribute('aria-selected', String(active));
        tab.tabIndex = active ? 0 : -1;
        document.getElementById(tab.getAttribute('aria-controls')).hidden = !active;
      }
      if (focus) selected.focus();
    };
    // Mouse and keyboard share the same state transition.
    // 鼠标和键盘共用同一状态转换。
    tabs.forEach((tab, index) => {
      tab.addEventListener('click', () => activate(tab, false));
      tab.addEventListener('keydown', (event) => {
        // Horizontal tabs support arrow navigation and both ends of the group.
        // 水平页签支持方向键导航及页签组首尾定位。
        let next;
        if (event.key === 'ArrowRight') next = (index + 1) % tabs.length;
        else if (event.key === 'ArrowLeft') next = (index - 1 + tabs.length) % tabs.length;
        else if (event.key === 'Home') next = 0;
        else if (event.key === 'End') next = tabs.length - 1;
        else return;
        event.preventDefault();
        activate(tabs[next], true);
      });
    });
  }

  // Draw flow connectors over the rendered node boxes; the edge list stays as the no-script fallback.
  // 在已渲染节点框上绘制流程连线；边列表保留为无脚本时的回退。
  const svgNs = 'http://www.w3.org/2000/svg';
  for (const figure of document.querySelectorAll('.flow')) {
    const ranks = figure.querySelector('.flow-ranks');
    const edges = Array.from(figure.querySelectorAll('.flow-edges li'));
    const vertical = figure.dataset.dir !== 'lr';
    // Redraw every connector from current box geometry; returns no value.
    // 依据当前方框几何重新绘制全部连线；不返回值。
    const draw = () => {
      for (const stale of ranks.querySelectorAll('svg,.edge-label')) stale.remove();
      const origin = ranks.getBoundingClientRect();
      const svg = document.createElementNS(svgNs, 'svg');
      svg.setAttribute('width', ranks.scrollWidth);
      svg.setAttribute('height', ranks.scrollHeight);
      const marker = document.createElementNS(svgNs, 'marker');
      marker.id = figure.id + '-arrow';
      for (const [name, value] of [['viewBox', '0 0 10 10'], ['refX', '9'], ['refY', '5'], ['markerWidth', '7'], ['markerHeight', '7'], ['orient', 'auto-start-reverse']]) marker.setAttribute(name, value);
      const head = document.createElementNS(svgNs, 'path');
      head.setAttribute('d', 'M0 0L10 5L0 10z');
      marker.append(head);
      const defs = document.createElementNS(svgNs, 'defs');
      defs.append(marker);
      svg.append(defs);
      // Box geometry relative to the rank container, independent of page scroll.
      // 相对于层级容器的方框几何，与页面滚动无关。
      const box = (element) => {
        const rect = element.getBoundingClientRect();
        return { x: rect.left - origin.left, y: rect.top - origin.top, w: rect.width, h: rect.height };
      };
      for (const edge of edges) {
        const from = document.getElementById(edge.dataset.from);
        const to = document.getElementById(edge.dataset.to);
        if (!from || !to) continue;
        const a = box(from);
        const b = box(to);
        let start;
        let end;
        let bend;
        if (vertical) {
          const down = b.y >= a.y + a.h;
          start = [a.x + a.w / 2, down ? a.y + a.h : a.y];
          end = [b.x + b.w / 2, down ? b.y : b.y + b.h];
          bend = [[start[0], (start[1] + end[1]) / 2], [end[0], (start[1] + end[1]) / 2]];
          if (!down) bend = [[start[0] + a.w, start[1] - 40], [end[0] + b.w, end[1] + 40]];
        } else {
          const right = b.x >= a.x + a.w;
          start = [right ? a.x + a.w : a.x, a.y + a.h / 2];
          end = [right ? b.x : b.x + b.w, b.y + b.h / 2];
          bend = [[(start[0] + end[0]) / 2, start[1]], [(start[0] + end[0]) / 2, end[1]]];
          if (!right) bend = [[start[0] - 40, start[1] + a.h], [end[0] + 40, end[1] + b.h]];
        }
        const path = document.createElementNS(svgNs, 'path');
        path.setAttribute('d', `M${start}C${bend[0]} ${bend[1]} ${end}`);
        path.setAttribute('class', edge.className);
        path.setAttribute('marker-end', `url(#${marker.id})`);
        svg.append(path);
        const text = edge.querySelector('em');
        if (text) {
          const label = document.createElement('span');
          label.className = 'edge-label';
          label.textContent = text.textContent;
          label.style.left = `${(start[0] + end[0]) / 2}px`;
          label.style.top = `${(start[1] + end[1]) / 2}px`;
          ranks.append(label);
        }
      }
      ranks.prepend(svg);
      figure.classList.add('drawn');
    };
    draw();
    new ResizeObserver(draw).observe(ranks);
  }
})();
