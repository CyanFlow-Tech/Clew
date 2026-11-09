// 代码配色主题：只换卡片里 token 的颜色。
//
// 窗口、卡片、面板、锚点那一整套颜色（--bg / --card / --acc / --call …）不动，
// 因为下面这些主题都是深色系。要上浅色主题得连标题栏、面板、按钮一起换，
// 那是另一件事。
//
// 主题表里每一项的键对应 --hl-* 变量，默认主题 clew 与 index.html 里的
// 兜底值一致，theme.js 没加载成功也不会掉色。
(function clewThemes() {
  const LIST = [
    {
      id: "clew",
      name: "Clew Dark",
      kw: "#ff7b72", str: "#a5d6ff", num: "#79c0ff", fn: "#d2a8ff", cls: "#ffa657",
      builtin: "#ffa657", meta: "#d2a8ff", self: "#ffa657", comment: "#8b949e",
      tmp: "#7d8794", param: "#c9d1d9",
    },
    {
      id: "one-dark",
      name: "One Dark",
      kw: "#c678dd", str: "#98c379", num: "#d19a66", fn: "#61afef", cls: "#e5c07b",
      builtin: "#56b6c2", meta: "#e06c75", self: "#e06c75", comment: "#5c6370",
      tmp: "#7f848e", param: "#abb2bf",
    },
    {
      id: "dracula",
      name: "Dracula",
      kw: "#ff79c6", str: "#f1fa8c", num: "#bd93f9", fn: "#50fa7b", cls: "#8be9fd",
      builtin: "#8be9fd", meta: "#ff79c6", self: "#f8f8f2", comment: "#6272a4",
      tmp: "#6272a4", param: "#f8f8f2",
    },
    {
      id: "monokai",
      name: "Monokai",
      kw: "#f92672", str: "#e6db74", num: "#ae81ff", fn: "#a6e22e", cls: "#66d9ef",
      builtin: "#66d9ef", meta: "#f92672", self: "#fd971f", comment: "#75715e",
      tmp: "#75715e", param: "#f8f8f2",
    },
    {
      id: "nord",
      name: "Nord",
      kw: "#81a1c1", str: "#a3be8c", num: "#b48ead", fn: "#88c0d0", cls: "#8fbcbb",
      builtin: "#88c0d0", meta: "#5e81ac", self: "#d8dee9", comment: "#616e88",
      tmp: "#616e88", param: "#d8dee9",
    },
    {
      id: "gruvbox",
      name: "Gruvbox Dark",
      kw: "#fb4934", str: "#b8bb26", num: "#d3869b", fn: "#fabd2f", cls: "#83a598",
      builtin: "#8ec07c", meta: "#fe8019", self: "#fe8019", comment: "#928374",
      tmp: "#928374", param: "#d5c4a1",
    },
    {
      id: "tokyo-night",
      name: "Tokyo Night",
      kw: "#bb9af7", str: "#9ece6a", num: "#ff9e64", fn: "#7aa2f7", cls: "#2ac3de",
      builtin: "#2ac3de", meta: "#bb9af7", self: "#e0af68", comment: "#565f89",
      tmp: "#565f89", param: "#c0caf5",
    },
    {
      id: "catppuccin",
      name: "Catppuccin Mocha",
      kw: "#cba6f7", str: "#a6e3a1", num: "#fab387", fn: "#89b4fa", cls: "#f9e2af",
      builtin: "#94e2d5", meta: "#f38ba8", self: "#f5e0dc", comment: "#6c7086",
      tmp: "#6c7086", param: "#cdd6f4",
    },
    {
      id: "solarized-dark",
      name: "Solarized Dark",
      kw: "#859900", str: "#2aa198", num: "#d33682", fn: "#268bd2", cls: "#b58900",
      builtin: "#6c71c4", meta: "#cb4b16", self: "#93a1a1", comment: "#586e75",
      tmp: "#586e75", param: "#93a1a1",
    },
    {
      id: "ayu-mirage",
      name: "Ayu Mirage",
      kw: "#ffa759", str: "#aad94c", num: "#d2a6ff", fn: "#ffd580", cls: "#73b8ff",
      builtin: "#95e6cb", meta: "#ff8f40", self: "#d2a6ff", comment: "#6c7a8c",
      tmp: "#6c7a8c", param: "#cccac2",
    },
    {
      id: "night-owl",
      name: "Night Owl",
      kw: "#c792ea", str: "#ecc48d", num: "#f78c6c", fn: "#82aaff", cls: "#ffcb8b",
      builtin: "#7fdbca", meta: "#c792ea", self: "#d6deeb", comment: "#637777",
      tmp: "#637777", param: "#d6deeb",
    },
    {
      id: "palenight",
      name: "Material Palenight",
      kw: "#c792ea", str: "#c3e88d", num: "#f78c6c", fn: "#82aaff", cls: "#ffcb6b",
      builtin: "#89ddff", meta: "#c792ea", self: "#f07178", comment: "#676e95",
      tmp: "#676e95", param: "#babed8",
    },
  ];

  const VAR = {
    kw: "--hl-kw",
    str: "--hl-str",
    num: "--hl-num",
    fn: "--hl-fn",
    cls: "--hl-cls",
    builtin: "--hl-builtin",
    meta: "--hl-meta",
    self: "--hl-self",
    comment: "--hl-comment",
    tmp: "--hl-tmp",
    param: "--hl-param",
  };
  const KEY = "clew.theme";

  function find(id) {
    for (let i = 0; i < LIST.length; i++) if (LIST[i].id === id) return LIST[i];
    return LIST[0];
  }

  function apply(id) {
    const theme = find(id);
    const root = document.documentElement.style;
    Object.keys(VAR).forEach(function (k) {
      if (theme[k]) root.setProperty(VAR[k], theme[k]);
    });
    window.__CLEW_THEME__ = theme.id;
    try {
      localStorage.setItem(KEY, theme.id);
    } catch (err) {
      /* 隐私模式写不进就算了，本次会话照常生效 */
    }
    return theme;
  }

  window.CLEW_THEMES = LIST;
  window.__clewApplyTheme = apply;
  window.__clewThemeId = function () {
    return window.__CLEW_THEME__ || LIST[0].id;
  };

  let saved = null;
  try {
    saved = localStorage.getItem(KEY);
  } catch (err) {
    saved = null;
  }
  apply(saved || LIST[0].id);
})();
