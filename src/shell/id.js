// shell/id.js
// 卡片 id（module:qualname）在侧栏与入口面板里的拆法与显示名。
import { showQual } from "../shared/format.js";

export function splitId(id) {
  const i = id.indexOf(":");
  return i < 0
    ? { module: id, qualname: id }
    : { module: id.slice(0, i), qualname: id.slice(i + 1) };
}

export function shortName(id) {
  const parts = splitId(id);
  // 模块级入口就是 if __name__ == "__main__" 那一块，名字叫 __main__，
  // 别再拿文件名冒充函数名
  if (parts.qualname === "<module>") return "__main__";
  return parts.qualname.split(".").pop() || parts.qualname;
}
