// Activate only the fixed, generated tab controls; no model code or host bridge is evaluated.
// 仅激活固定生成的页签控件；不求值模型代码，也不调用宿主桥接。
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
})();
