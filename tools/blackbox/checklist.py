#!/usr/bin/env python3
"""
Machine-checkable black-box checklist.

Mirrors docs/01_BLACKBOX_PLAN.md but stores status, evidence and notes in a
JSON file so the progress of the black-box phase can be tracked and audited.

    python3 tools/blackbox/checklist.py summary
    python3 tools/blackbox/checklist.py list --group 规则
    python3 tools/blackbox/checklist.py set BB-G03 --status done \
        --evidence shots/g03_5line.png --note "5 连触发，4 连不触发"
    python3 tools/blackbox/checklist.py check
    python3 tools/blackbox/checklist.py export > checklist.md
"""
import argparse, json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
STORE = os.path.join(ROOT, "blackbox", "checklist.json")

# id, group, title, method, criteria
ITEMS = [
    # ---------------------------------------------------------- 菜单流程
    ("BB-M01", "菜单", "启动后主菜单完整项与层级", "录屏 + 资源导出", "逐项截图；记录 Accelerator 提示文本"),
    ("BB-M02", "菜单", "每个菜单项的 Enabled/Checked 初始态", "winprobe menu.json + 悬停录屏", "记录带勾选的项"),
    ("BB-M03", "菜单", "每个菜单项点击后的行为", "逐项点击录屏", "记录是否弹窗/改棋盘/改配置"),
    ("BB-M04", "菜单", "子菜单展开延迟与层级", "录屏帧率分析", "展开延迟 ms"),
    ("BB-M05", "菜单", "菜单与快捷键的等价性", "鼠标与快捷键各触发一次", "行为必须一致"),
    ("BB-M06", "菜单", "游戏中 vs 结束后菜单可用性差异", "两轮各测一遍", "记录被禁用的项"),
    # ---------------------------------------------------------- 按键映射
    ("BB-K01", "按键", "全部快捷键及其作用", "F1-F12/Alt/Ctrl 逐个按压", "建立按键→功能映射表"),
    ("BB-K02", "按键", "快捷键在不同窗口焦点下是否生效", "主窗口/对话框/统计窗口", "记录差异"),
    ("BB-K03", "按键", "鼠标单击棋盘格的效果", "逐格点击录屏", "记录选中/移动/落子反馈"),
    ("BB-K04", "按键", "双击行为", "同一格快速双击", "与两次单击是否不同"),
    ("BB-K05", "按键", "拖拽行为", "按下-移动-抬起全程录屏", "是否支持拖拽"),
    ("BB-K06", "按键", "鼠标悬停效果", "缓慢划过棋盘", "悬停高亮规则"),
    ("BB-K07", "按键", "右键 / 中键行为", "分别点击", "是否有上下文菜单"),
    ("BB-K08", "按键", "鼠标移出窗口", "移出再移入", "状态复位规则"),
    ("BB-K09", "按键", "窗口缩放中的输入", "拖边框同时点击棋盘", "坐标映射是否随尺寸变化"),
    ("BB-K10", "按键", "键盘方向键 / 回车的棋盘交互", "逐键按压", "是否存在键盘操控"),
    # ---------------------------------------------------------- 游戏规则
    ("BB-G01", "规则", "新局初始棋盘内容", "每局截图 + 逐格记录球色", "初始球数、颜色分布、是否随机"),
    ("BB-G02", "规则", "一次有效操作后棋盘如何变化", "逐次操作录屏", "新球从何处生成、落点规则"),
    ("BB-G03", "规则", "消除触发条件", "构造 3/4/5/6 连", "最小消除数量"),
    ("BB-G04", "规则", "连线是否允许拐弯", "构造 L 形、U 形路径", "拐弯次数上限与折线规则"),
    ("BB-G05", "规则", "连线是否允许穿越已有球", "构造被阻挡路径", "阻挡规则"),
    ("BB-G06", "规则", "消除后棋盘如何重排", "消除前后逐帧比对", "是否下落、是否补充、何时补充"),
    ("BB-G07", "规则", "棋盘满时的行为", "持续操作直到无路可走", "结束条件与提示"),
    ("BB-G08", "规则", "无可消除连线时的棋盘演化", "长时录制", "是否随机补球、何时补"),
    ("BB-G09", "规则", "选中态与路径高亮的视觉表现", "逐帧录屏", "高亮颜色、线宽、是否带箭头"),
    ("BB-G10", "规则", "消除动画时序", "60 fps 逐帧", "动画时长、帧数、是否有停顿"),
    ("BB-G11", "规则", "计分规则", "每次消除记录分数增量", "消除个数/长度 → 分值映射"),
    ("BB-G12", "规则", "分数上限与封顶", "构造高分消除", "是否有单次/单局封顶"),
    ("BB-G13", "规则", "等级/关卡推进", "连续多局", "触发条件与变化"),
    ("BB-G14", "规则", "游戏结束条件与结束画面", "走到结束", "结束判定、弹窗文案、按钮"),
    ("BB-G15", "规则", "新纪录判定", "刻意超过历史最高分", "提示弹窗与按钮行为"),
    # ---------------------------------------------------------- 数值表
    ("BB-N01", "数值", "棋盘维度", "计数可视格数", "9x9"),
    ("BB-N02", "数值", "每格颜色集合", "采样所有出现的颜色", "7 色 + RGB"),
    ("BB-N03", "数值", "初始球数", "新局计数", "是否固定"),
    ("BB-N04", "数值", "新球补充数量", "每次操作后计数", "固定值"),
    ("BB-N05", "数值", "消除阈值", "见 BB-G03", "固定值"),
    ("BB-N06", "数值", "分值表", "见 BB-G11", "完整对照表"),
    ("BB-N07", "数值", "等级表", "见 BB-G13", "完整对照表"),
    ("BB-N08", "数值", "高分榜条目数", "打开统计窗口", "16"),
    ("BB-N09", "数值", "高分榜排序规则", "制造同分/异分", "同分次序"),
    ("BB-N10", "数值", "玩家名输入规则", "触发新纪录", "最大长度、字符集、默认名"),
    ("BB-N11", "数值", "时间/难度参数", "连续操作", "补球频率是否随时间变化"),
    # ---------------------------------------------------------- UI 布局
    ("BB-U01", "UI", "主窗口初始尺寸与位置", "winprobe windows.json + INI", "默认值"),
    ("BB-U02", "UI", "客户区各区域几何", "geometry.py 测量截图", "棋盘区、侧栏、边框的像素矩形"),
    ("BB-U03", "UI", "布局随窗口尺寸的变化", "拉伸到多组尺寸截图", "锚定规则（居中/左对齐）"),
    ("BB-U04", "UI", "棋盘格在客户区的位置", "geometry.py 测量", "棋盘原点相对客户区的偏移"),
    ("BB-U05", "UI", "侧栏内容", "截图", "分数、下一颗预览、统计条、按钮"),
    ("BB-U06", "UI", "下一颗预览球", "截图", "预览位置与数量"),
    ("BB-U07", "UI", "统计条 / 量表控件", "截图", "含义与刷新时机"),
    ("BB-U08", "UI", "对话框（统计/关于/新纪录）", "逐个打开截图", "控件布局、默认按钮、文本"),
    ("BB-U09", "UI", "工具提示", "悬停录屏", "触发区域与文本"),
    ("BB-U10", "UI", "图标与标题", "截图", "任务栏/标题栏图标"),
    ("BB-U11", "UI", "高分榜文字排版", "截图", "`名次. 姓名` 格式与列宽"),
    ("BB-U12", "UI", "系统主题跟随", "切换主题/经典模式", "是否使用 GetSysColor"),
]

STATUS = ["todo", "in-progress", "blocked", "done", "failed"]


def load():
    if os.path.exists(STORE):
        with open(STORE, encoding="utf-8") as fh:
            return json.load(fh)
    return {"version": 1, "items": {
        i: {"status": "todo", "evidence": [], "notes": "", "owner": "",
            "group": g, "title": t, "method": m, "criteria": c}
        for i, g, t, m, c in ITEMS}}


def save(data):
    os.makedirs(os.path.dirname(STORE), exist_ok=True)
    with open(STORE, "w", encoding="utf-8") as fh:
        json.dump(data, fh, ensure_ascii=False, indent=1)


def cmd_list(a):
    d = load()
    for i, g, t, m, c in ITEMS:
        if a.group and a.group not in g:
            continue
        st = d["items"][i]
        print("%-8s [%-11s] %-14s %s" % (i, st["status"], g, t))
        if a.verbose:
            print("          method : %s" % m)
            print("          criteria: %s" % c)
            if st["notes"]:
                print("          notes  : %s" % st["notes"])
            for e in st["evidence"]:
                print("          evidence: %s" % e)


def cmd_set(a):
    d = load()
    st = d["items"].get(a.id)
    if not st:
        print("unknown item: %s" % a.id); return 1
    if a.status:
        if a.status not in STATUS:
            print("invalid status (use one of %s)" % ", ".join(STATUS)); return 1
        st["status"] = a.status
    if a.note:
        st["notes"] = (st["notes"] + "\n" + a.note).strip() if st["notes"] else a.note
    if a.evidence:
        st["evidence"].append(a.evidence)
    save(d)
    print("%-8s -> %-11s  evidence=%d" % (a.id, st["status"], len(st["evidence"])))
    return 0


def cmd_summary(a):
    d = load()
    from collections import Counter
    groups = {}
    for i, g, t, m, c in ITEMS:
        groups.setdefault(g, Counter())[d["items"][i]["status"]] += 1
    print("%-8s %6s %6s %8s %6s %8s %8s" % ("group", "todo", "doing", "blocked", "done", "failed", "total"))
    tot = Counter()
    for g in sorted(groups):
        c = groups[g]
        s = sum(c.values())
        for k in c: tot[k] += c[k]
        print("%-8s %6d %6d %8d %6d %8d %8d" % (g, c["todo"], c["in-progress"],
                                                 c["blocked"], c["done"], c["failed"], s))
    s = sum(tot.values())
    print("%-8s %6d %6d %8d %6d %8d %8d" % ("TOTAL", tot["todo"], tot["in-progress"],
                                              tot["blocked"], tot["done"], tot["failed"], s))
    done = tot["done"]
    print("\nprogress: %d/%d (%.0f%%)" % (done, s, 100.0*done/s if s else 0))
    return 0


def cmd_check(a):
    """Every item marked done must carry evidence."""
    d = load()
    bad = []
    for i, g, t, m, c in ITEMS:
        st = d["items"][i]
        if st["status"] == "done" and not st["evidence"]:
            bad.append((i, "done but no evidence"))
        if st["status"] == "blocked" and not st["notes"]:
            bad.append((i, "blocked but no reason"))
    for i, why in bad:
        print("  PROBLEM %-8s %s" % (i, why))
    if not bad:
        print("  all items consistent")
    return 1 if bad else 0


def cmd_export(a):
    d = load()
    print("# 黑盒采集进度\n")
    from collections import defaultdict
    by = defaultdict(list)
    for i, g, t, m, c in ITEMS:
        by[g].append((i, t, m, c, d["items"][i]))
    for g in sorted(by):
        print("## %s\n" % g)
        print("| ID | 采集项 | 方法 | 通过判据 | 状态 | 证据 |")
        print("|---|---|---|---|---|---|")
        for i, t, m, c, st in by[g]:
            ev = "<br>".join(st["evidence"]) if st["evidence"] else "-"
            print("| %s | %s | %s | %s | %s | %s |" % (i, t, m, c, st["status"], ev))
        print()
    return 0


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("list"); p.add_argument("--group"); p.add_argument("-v", "--verbose", action="store_true"); p.set_defaults(fn=cmd_list)
    p = sub.add_parser("set"); p.add_argument("id"); p.add_argument("--status", choices=STATUS); p.add_argument("--evidence"); p.add_argument("--note"); p.set_defaults(fn=cmd_set)
    sub.add_parser("summary").set_defaults(fn=cmd_summary)
    sub.add_parser("check").set_defaults(fn=cmd_check)
    sub.add_parser("export").set_defaults(fn=cmd_export)
    a = ap.parse_args()
    sys.exit(a.fn(a) or 0)


if __name__ == "__main__":
    main()
