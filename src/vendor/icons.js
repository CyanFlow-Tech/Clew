/*!
 * 图标入口：从两套资源里挑一套用。
 *
 * 资源在 icons-antd.js（Ant Design Icons，MIT）与 icons-material.js
 * （Material Symbols，Apache-2.0），两者的键名完全一样，换一套只换这件事。
 * 默认用 material；想临时看另一套，在地址后面加 ?icons=antd。
 */
(function () {
  var DEFAULT = "material";
  var sets = window.CLEW_ICON_SETS || {};
  var asked = null;
  try {
    asked = new URLSearchParams(location.search).get("icons");
  } catch (err) {
    asked = null;
  }
  var name = asked && sets[asked] ? asked : DEFAULT;
  var defs = sets[name] || {};

  window.CLEW_ICON_SET = name;
  window.CLEW_ICONS = defs;

  /// 把一个图标拼成 svg 字符串。网格由资源决定（antd 1024、material 960），
  /// 尺寸交给 CSS。style 里的 position/inset 是必须的：index.html 有一条给
  /// 画布连线用的 `svg{position:absolute;inset:0}` 全局规则，不顶回来图标会飞到视口左上角。
  window.CLEW_ICON = function (iconName) {
    var def = defs[iconName];
    if (!def) {
      // 键名写错时不要静默画空白：这一句能在控制台直接指出是哪个键
      console.warn("[icons] unknown icon key: " + iconName + " (set: " + name + ")");
      return "";
    }
    return (
      '<svg viewBox="' + def.viewBox + '" width="1em" height="1em"' +
      ' fill="currentColor" aria-hidden="true" style="position:static;inset:auto">' +
      def.paths.map(function (d) { return '<path d="' + d + '"/>'; }).join("") +
      "</svg>"
    );
  };
})();
