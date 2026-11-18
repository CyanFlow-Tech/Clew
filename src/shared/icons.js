// shared/icons.js
// 全软件唯一一套图标：vendor/icons.js 选好的一套，暴露成图标函数与名字表。
// 拆分前 app.js 与 shell.js 各自从 window.CLEW_ICON 拼一份，收藏星标也存了两份。

export const icon = window.CLEW_ICON || function () { return ""; };

// 外壳按名字取：ICON.gear。画布那边直接 icon("close")，只有一处拼字符串。
export const ICON = {
  min: icon("min"),
  max: icon("max"),
  restore: icon("restore"),
  close: icon("close"),
  chevronDown: icon("chevronDown"),
  chevronRight: icon("chevronRight"),
  folder: icon("folder"),
  search: icon("search"),
  eye: icon("eye"),
  eyeOff: icon("eyeOff"),
  trash: icon("trash"),
  warn: icon("warn"),
  gear: icon("gear"),
  menu: icon("menu"),
};

// 收藏星标：antd 的 StarOutlined / StarFilled 两份都放进去，靠 CSS 切显示
export const STAR_SVG =
  '<span class="star-off">' + icon("starOff") + '</span>' +
  '<span class="star-on">' + icon("starOn") + '</span>';
