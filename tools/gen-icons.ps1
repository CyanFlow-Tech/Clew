$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$antdSrc = Join-Path $root "target\icons-src"
$matSrc = Join-Path $root "target\icons-material"

$antdMap = [ordered]@{
  min = "MinusOutlined"; max = "BorderOutlined"; restore = "BlockOutlined"; close = "CloseOutlined"
  folder = "FolderOutlined"; chevronDown = "DownOutlined"; chevronRight = "RightOutlined"
  search = "SearchOutlined"; eye = "EyeOutlined"; eyeOff = "EyeInvisibleOutlined"
  trash = "DeleteOutlined"; warn = "WarningOutlined"; gear = "SettingOutlined"; menu = "MenuOutlined"
  starOff = "StarOutlined"; starOn = "StarFilled"; check = "CheckOutlined"; external = "GlobalOutlined"

  enter = "EnterOutlined"
}

$matMap = [ordered]@{
  min = "remove"; max = "crop_square"; restore = "flip_to_front"; close = "close"
  folder = "folder"; chevronDown = "keyboard_arrow_down"; chevronRight = "keyboard_arrow_right"
  search = "search"; eye = "visibility"; eyeOff = "visibility_off"
  trash = "delete"; warn = "warning"; gear = "settings"; menu = "menu"
  starOff = "starOff"; starOn = "starOn"; check = "check"; external = "public"
}

$antdRows = foreach ($k in $antdMap.Keys) {
  $t = Get-Content (Join-Path $antdSrc ($antdMap[$k] + ".js")) -Raw
  $paths = [regex]::Matches($t, '"d"\s*:\s*"([^"]+)"') | ForEach-Object { $_.Groups[1].Value }
  $vb = [regex]::Match($t, '"viewBox"\s*:\s*"([^"]+)"').Groups[1].Value
  if (-not $vb) { $vb = "64 64 896 896" }
  $quoted = ($paths | ForEach-Object { '"' + $_ + '"' }) -join ", "
  "  " + $k + ": { viewBox: `"$vb`", paths: [$quoted] }"
}

$matRows = foreach ($k in $antdMap.Keys) {
  # 下载时按 key 命名（Remove-Item 那批），Material 的原名记在上面 $matMap 里备查
  $t = Get-Content (Join-Path $matSrc ($k + ".svg")) -Raw
  $paths = [regex]::Matches($t, '<path[^>]*\sd="([^"]+)"') | ForEach-Object { $_.Groups[1].Value }
  $vb = [regex]::Match($t, 'viewBox="([^"]+)"').Groups[1].Value
  $quoted = ($paths | ForEach-Object { '"' + $_ + '"' }) -join ", "
  "  " + $k + ": { viewBox: `"$vb`", paths: [$quoted] }"
}

$antdHeader = @"
/*!
 * Ant Design Icons, outlined subset - one of the icon sets.
 * Copyright (c) 2018-present Ant UED
 * MIT License: see antd-icons.LICENSE next to this file.
 * Generated from @ant-design/icons-svg 4.4.2; only the icons Clew uses.
 */
window.CLEW_ICON_SETS = window.CLEW_ICON_SETS || {};
window.CLEW_ICON_SETS.antd = {
"@
$matHeader = @"
/*!
 * Material Symbols, outlined - one of the icon sets.
 * Copyright Google LLC
 * Apache License 2.0: see material-icons.LICENSE next to this file.
 * Downloaded from fonts.gstatic.com (materialsymbolsoutlined); only the icons Clew uses.
 */
window.CLEW_ICON_SETS = window.CLEW_ICON_SETS || {};
window.CLEW_ICON_SETS.material = {
"@

Set-Content -Path (Join-Path $root "src\vendor\icons-antd.js") -Encoding UTF8 -NoNewline `
  -Value ($antdHeader + "`r`n" + ($antdRows -join ",`r`n") + "`r`n};`r`n")
Set-Content -Path (Join-Path $root "src\vendor\icons-material.js") -Encoding UTF8 -NoNewline `
  -Value ($matHeader + "`r`n" + ($matRows -join ",`r`n") + "`r`n};`r`n")

"antd:     " + $antdRows.Count + " icons, " + (Get-Item (Join-Path $root "src\vendor\icons-antd.js")).Length + " bytes"
"material: " + $matRows.Count + " icons, " + (Get-Item (Join-Path $root "src\vendor\icons-material.js")).Length + " bytes"
