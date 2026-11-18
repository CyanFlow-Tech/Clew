// app/env.js
// 画布的常量与运行环境：不持有状态，只回答「跑在哪、多大」。
export const SUMMARY = 6, CARD_W = 720, COL_GAP = 70, ROW_GAP = 34, HEAD_H = 26;

// 数据按需取：点开一个节点才向后端要它的卡片与出边
export const desktopApi = window.__TAURI__.core;

// 后端的调用偶尔会既不给结果也不报错，那一次点击就永远停在原地，
// 后面每次点又都排在同一批未完成的请求上：整个界面等于废掉。
export const CARD_TIMEOUT = 20000;
