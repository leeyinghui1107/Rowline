# Rowline

Rowline 是一款跨平台的连线消除游戏：在 9×9 棋盘上移动彩色球，把同色球连成
五颗或更多，即可消除并得分。

它是 **WinLinez v1.30** 的独立重实现。玩法规则来自对原作可观察行为的整理与对
自有副本的分析；程序、美术和名称均为本项目原创，不包含原作的代码、图像或数据。

## 玩法

- 棋盘为 **9×9**，共有 **7 种颜色**。
- 点击一颗球选中它，再点击一个可达的空格移动过去。球只能沿上下左右四个方向
  经过空格移动，源格随即清空。
- 移动后，如果经过落点的横、竖或两条对角线中有 **5 颗及以上同色球**，这些球
  会同时消除。多条线可以同时成立。
- 消除得分：5 颗 **10 分**、6 颗 **12 分**、7 颗 **18 分**、8 颗 **28 分**、
  9 颗 **42 分**。
- 如果这一步没有消除，系统会从“下一组”中补充 **3 颗球**，并预告新的下一组。
  发生消除的回合不补球。
- 新游戏会在空棋盘上随机放置 **5 颗球**。
- 当棋盘填满、无法继续移动时游戏结束。
- 高分榜保留 **16 行**，首次启动默认有“国王 100”。达到上榜分数时可以输入名字。

棋盘两侧的角色高度表示分数：左侧“国王”代表历史最高纪录，右侧“挑战者”
代表本局当前分数。挑战者超过纪录时会逐渐升高，最高到纪录高度的 150%。

## 操作

| 操作 | 功能 |
|---|---|
| 鼠标左键 | 选中球 / 移动到可达空格 |
| `F4` | 新游戏 |
| `H` | 高分榜 |
| `F5` | 显示统计 |
| `F1` | 帮助 |
| `F10` | 菜单栏 |
| `F11` | 全屏切换 |
| `Alt+F9` | 退出 |

菜单只有两组：

- **文件**：新游戏、高分榜、显示统计、退出
- **帮助**：帮助主题、关于 Rowline

统计、帮助、关于、高分榜和姓名输入都是居中的模态窗口，可用右上角 ×、底部
“关闭/确定”或 Esc 关闭。

## 下载与运行

从 [Releases](https://github.com/leeyinghui1107/Rowline/releases) 下载对应平台：

| 平台 | 文件 | 说明 |
|---|---|---|
| macOS | `rowline-darwin.zip` | 解压后双击 `Rowline.app`，不会打开终端窗口 |
| Windows | `rowline-win32.zip` | 解压后运行 `rowline.exe`，支持 Windows 10/11 |
| Linux | `rowline-linux.zip` | 解压后运行 `rowline`，为 x86_64 可执行文件 |

程序是**单文件**的：精灵图、图标和 UI 字体都编译在可执行文件内部，
不需要额外的资源目录。存档写在平台的应用数据目录中：

- macOS：`~/Library/Application Support/rowline/save.txt`
- Windows：`%APPDATA%\rowline\save.txt`
- Linux：`$XDG_DATA_HOME/rowline/save.txt` 或 `~/.local/share/rowline/save.txt`

macOS 的 `.app` 使用 ad-hoc 签名、未经过公证。如果从网络下载后首次启动被
Gatekeeper 拦截，右键应用选择“打开”，或在“系统设置 → 隐私与安全性”中放行。

## 技术架构

Rowline 使用 Rust 编写，图形层基于 **winit 0.30** 和 **wgpu 30**。

### 分层

```text
rowline-app        事件循环、菜单、HUD、对话框、动画与游戏装配
    ├── rowline-render   wgpu 后端、软件画布、字体与字形图集
    ├── rowline-platform 窗口/时钟/文件系统抽象
    ├── rowline-storage  文本存档与高分榜
    ├── rowline-assets   内嵌精灵图、图标和字体
    ├── rowline-theme    Classic 配色与设计令牌
    ├── rowline-display  逻辑/物理坐标、视口与素材倍率选择
    └── rowline-core     棋盘、规则、随机数、布局、状态与渲染指令
```

`rowline-core` 不依赖任何外部 crate，也不做 I/O；棋盘规则、路径搜索、消除判定、
计分和随机数都可以脱离窗口独立运行。渲染层只接收逻辑坐标下的绘制指令，不参与
游戏判定。

### 渲染与显示

- 棋盘格固定为 36 逻辑像素；窗口大小变化只改变四周留白，不改变玩法几何。
- 逻辑坐标与物理像素分离，按显示器的 DPI 缩放；字体按实际像素密度重新栅格化。
- 精灵图提供 @1x–@4x 四种倍率，渲染时按屏幕 DPR 选择足够清晰的版本。
- 球体使用精灵图；对话框、HUD 和按钮使用 SDF 圆角矩形、描边与文字图元绘制。

### 单文件与跨平台

- 精灵图、图标和中英文字体通过 `include_bytes!` 编入程序。
- UI 字体为 WenQuanYi Micro Hei，覆盖拉丁与 CJK，不依赖系统是否安装中文字体。
- macOS 发布为 arm64 + x86_64 通用 `Rowline.app`；Windows 发布为 x86_64 `.exe`；
  Linux 发布为 x86_64 ELF。
- 存档是简单、可手工编辑的文本格式，便于备份和迁移。

### 运行要求

- macOS：macOS 11 或更高版本，支持 Metal 的 Mac。
- Windows：Windows 10/11 x86_64，支持 DirectX 12 的显卡。
- Linux：x86_64 桌面环境，需要 glibc（发布包基于 Ubuntu 24.04 构建）、支持 X11 或
  Wayland，以及 Vulkan / OpenGL ES 图形驱动。

## 来源与版权

原作 **WinLinez v1.30** © 1998–1999 **Ivan Golubev**，由 GMSoft 发行
（<http://www.golubev.com>）。Rowline 是独立的玩法重实现，与原作作者及发行方
没有关联，也未获得其背书。

本项目不包含原作的任何美术、代码、数据或名称。精灵图、图标和角色造型由
本仓库的工具生成；界面字体 **WenQuanYi Micro Hei 0.2.0-beta** 以
**Apache License 2.0** 授权，许可见 `assets/font/NOTICE.txt` 和
`assets/font/LICENSE-APACHE-2.0.txt`。
