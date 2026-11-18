// shell/sidebase.js
import { ICON } from "./util.js";


export const side = document.createElement("div");
side.id = "side-panel";
side.hidden = true;
side.innerHTML =
  '<div class="sp-pane" id="sp-pane-tree">' +
    '<div class="sp-head">' +
      '<button type="button" class="sp-view" id="sp-view-btn">' +
        '<span id="sp-view-label"></span>' + ICON.chevronDown +
      "</button>" +
    "</div>" +
    '<div class="sp-list" id="sp-list"></div>' +
  "</div>" +
  '<div class="sp-pane" id="sp-pane-search" hidden>' +
    '<div class="sp-query" id="sp-query">' +
      '<label class="sp-q-row"><span class="sp-q-label">路径</span>' +
        '<input id="sp-q-path" autocomplete="off" spellcheck="false"></label>' +
      '<label class="sp-q-row"><span class="sp-q-label">函数</span>' +
        '<input id="sp-q-name" autocomplete="off" spellcheck="false"></label>' +
    "</div>" +
    '<div class="sp-list" id="sp-list-search"></div>' +
  "</div>";
document.body.appendChild(side);

export const spViewBtn = side.querySelector("#sp-view-btn");
export const spViewLabel = side.querySelector("#sp-view-label");
export const spList = side.querySelector("#sp-list");
export const spQPath = side.querySelector("#sp-q-path");
export const spQName = side.querySelector("#sp-q-name");
export const spListSearch = side.querySelector("#sp-list-search");
export const spPaneTree = side.querySelector("#sp-pane-tree");
export const spPaneSearch = side.querySelector("#sp-pane-search");


