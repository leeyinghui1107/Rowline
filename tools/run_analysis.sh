#!/usr/bin/env bash
# Regenerate the full analysis set for WINLINEZ.EXE into analysis/ and assets/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXE="${1:-/Users/thomas/Downloads/WINLINEZ.EXE}"
A="$ROOT/analysis"; AS="$ROOT/assets"
mkdir -p "$A" "$AS/resources" "$AS/png"

echo "### 0. 指纹"
{
  echo "file:    $EXE"
  echo "size:    $(stat -f%z "$EXE") bytes"
  echo "md5:     $(md5 -q "$EXE")"
  echo "sha256:  $(shasum -a 256 "$EXE" | cut -d' ' -f1)"
  echo "sha1:    $(shasum -a 1 "$EXE" | cut -d' ' -f1)"
  echo "date:    $(stat -f%Sm "$EXE")"
} > "$A/hashes.txt"
cat "$A/hashes.txt"

echo "### 1. PE 结构"
python3 "$ROOT/tools/pe_triage.py" "$EXE" > "$A/pe_triage.txt"

echo "### 2. 资源导出"
python3 "$ROOT/tools/res_dump.py" "$EXE" "$AS/resources" > "$A/resources.txt"

echo "### 3. 位图渲染"
python3 "$ROOT/tools/bmp2png.py" > "$A/bmp2png.txt"

echo "### 4. 菜单 / 加速键"
python3 "$ROOT/tools/menu_accel.py" > "$A/menu_accel.txt"

echo "### 5. 精灵索引"
python3 "$ROOT/tools/sprite_table.py" > "$A/sprite_table.txt"

echo "### 6. 交叉引用"
python3 "$ROOT/tools/xrefs.py" > "$A/xrefs.txt"

echo "### 7. 代码段字符串"
python3 - <<'PY' > "$A/strings_code.txt"
import re
d = open("/Users/thomas/Downloads/WINLINEZ.EXE", "rb").read()
code = d[0x400:0x8C00]
print("=== .data / .idata 字符串 (VA) ===")
for m in re.finditer(rb"[\x20-\x7e]{4,}", d[0x8600:0x8C00]):
    print(f"  0x{0x40A000+m.start():06X}  {m.group().decode()}")
print("\n=== 代码段内嵌字符串 (VA) ===")
for m in re.finditer(rb"[\x20-\x7e]{8,}", code):
    s = m.group().decode()
    if " " in s and re.search(r"[A-Za-z]{3}", s):
        print(f"  0x{0x401000+m.start():06X}  {s}")
PY

cp /tmp/wl_png/*.png "$AS/png/" 2>/dev/null || true
cp /tmp/wl_png/sprite_table.json "$AS/" 2>/dev/null || true
cp "$AS/resources/_manifest.json" "$A/resources_manifest.json" 2>/dev/null || true

echo
echo "完成 -> $A, $AS"
ls -la "$A"
