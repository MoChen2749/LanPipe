"""生成 LanPipe UI 线框草图（WinUI 3 / Fluent 2 风格，含分区标注）。"""

from PIL import Image, ImageDraw, ImageFont
import os

OUT = r"F:\Projects\lanpipe\docs\ui"
os.makedirs(OUT, exist_ok=True)

CJK = r"C:\Windows\Fonts\msyh.ttc"
CJK_B = r"C:\Windows\Fonts\msyhbd.ttc"
LAT = r"C:\Windows\Fonts\segoeui.ttf"
LAT_B = r"C:\Windows\Fonts\segoeuib.ttf"


def F(size, bold=False, cjk=True):
    path = (CJK_B if bold else CJK) if cjk else (LAT_B if bold else LAT)
    try:
        return ImageFont.truetype(path, size)
    except Exception:
        return ImageFont.load_default()


ACCENT = (0, 95, 184)
ACCENT_SOFT = (229, 241, 251)
BG = (243, 243, 243)
SURFACE = (255, 255, 255)
BORDER = (225, 225, 225)
TEXT = (26, 26, 26)
MUTED = (96, 96, 96)
SUBTLE = (140, 140, 140)
GREEN = (16, 124, 16)


def rr(d, box, radius=8, fill=SURFACE, outline=None, width=1):
    d.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=width)


def t(d, xy, s, size=13, fill=TEXT, bold=False, cjk=True, anchor="la"):
    d.text(xy, s, font=F(size, bold, cjk), fill=fill, anchor=anchor)


def badge(d, xy, n):
    x, y = xy
    d.ellipse((x, y, x + 22, y + 22), fill=(196, 43, 28))
    t(d, (x + 11, y + 11), str(n), 12, (255, 255, 255), True, False, "mm")


def icon_folder(d, x, y, c=(255, 176, 40)):
    d.rounded_rectangle((x, y + 4, x + 20, y + 18), 2, fill=c)
    d.rounded_rectangle((x + 1, y, x + 9, y + 6), 2, fill=c)


def icon_file(d, x, y, c=(96, 130, 182)):
    d.rounded_rectangle((x + 3, y, x + 17, y + 20), 2, fill=c)
    d.polygon([(x + 9, y), (x + 17, y), (x + 17, y + 8)], fill=(255, 255, 255, 60))


def icon_pc(d, x, y, c=(70, 70, 70)):
    d.rectangle((x, y, x + 20, y + 13), outline=c, width=2)
    d.line((x + 6, y + 19, x + 14, y + 19), fill=c, width=2)
    d.line((x + 10, y + 14, x + 10, y + 18), fill=c, width=2)


def icon_gear(d, x, y, c=(70, 70, 70)):
    d.ellipse((x + 2, y + 2, x + 16, y + 16), outline=c, width=2)
    d.ellipse((x + 7, y + 7, x + 11, y + 11), fill=c)


def icon_arrow_up(d, x, y, c=(0, 95, 184)):
    d.line((x + 10, y + 20, x + 10, y + 4), fill=c, width=2)
    d.polygon([(x + 10, y), (x + 4, y + 7), (x + 16, y + 7)], fill=c)
    d.line((x + 2, y + 24, x + 18, y + 24), fill=c, width=2)


def icon_doc(d, x, y, c=(96, 130, 182)):
    d.rectangle((x + 3, y, x + 17, y + 20), outline=c, width=2)
    for i in range(3):
        d.line((x + 6, y + 7 + i * 4, x + 14, y + 7 + i * 4), fill=c, width=1)


def icon_history(d, x, y, c=(70, 70, 70)):
    d.arc((x + 1, y + 2, x + 19, y + 20), 30, 330, fill=c, width=2)
    d.line((x + 10, y + 6, x + 10, y + 11), fill=c, width=2)
    d.line((x + 10, y + 11, x + 14, y + 13), fill=c, width=2)


def icon_send(d, x, y, c=(70, 70, 70)):
    d.polygon([(x + 2, y + 10), (x + 20, y + 2), (x + 12, y + 20), (x + 9, y + 12)], outline=c, fill=None)


def nav_frame(size, title):
    w, h = size
    img = Image.new("RGB", (w, h), (255, 255, 255))
    d = ImageDraw.Draw(img)
    return img, d


def legend(d, xy, items, width=1120, size=12, cols=2):
    x0, y0 = xy
    per = (len(items) + cols - 1) // cols
    for i, (num, text) in enumerate(items):
        col = i // per
        row = i % per
        x = x0 + col * (width // cols)
        y = y0 + row * 22
        badge(d, (x, y - 1), num)
        t(d, (x + 30, y), text, size, TEXT)


# ---------------------------------------------------------------- A 发送页
def screen_send():
    W, H = 1240, 700
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)

    # 标题栏
    rr(d, (0, 0, W, 48), 0, SURFACE)
    rr(d, (16, 12, 42, 38), 8, ACCENT)
    t(d, (23, 16), "LP", 12, (255, 255, 255), True, False)
    t(d, (50, 15), "LanPipe", 16, TEXT, True)
    t(d, (130, 19), "局域网传输", 11, SUBTLE)
    rr(d, (W - 350, 10, W - 120, 38), 6, (245, 245, 245))
    t(d, (W - 338, 17), "本机 · WIN-2H1782KNJDF", 11, MUTED)
    icon_gear(d, W - 96, 15)
    badge(d, (W - 58, 12), 1)

    # 左侧导航
    rr(d, (0, 48, 76, H), 0, (250, 250, 250))
    nav = [("send", "发送", True), ("recv", "接收", False), ("hist", "历史", False), ("set", "设置", False)]
    for i, (kind, label, sel) in enumerate(nav):
        y = 72 + i * 78
        if sel:
            d.rectangle((0, y - 6, 3, y + 52), fill=ACCENT)
            rr(d, (10, y - 6, 66, y + 52), 6, ACCENT_SOFT)
        x = 26
        (icon_send if kind == "send" else icon_arrow_up if kind == "recv" else icon_history if kind == "hist" else icon_gear)(d, x, y + 6)
        t(d, (38, y + 32), label, 10, ACCENT if sel else MUTED, True, True, "mm")
    badge(d, (58, 60), 2)

    # 主区域
    t(d, (100, 66), "发送", 20, TEXT, True)
    t(d, (100, 94), "拖入内容，选择设备即可发送", 11, MUTED)

    # 拖放区
    x0, y0, x1, y1 = 100, 120, 840, 300
    d.rounded_rectangle((x0, y0, x1, y1), 10, fill=SURFACE, outline=(180, 180, 180), width=2)
    for i in range(x0 + 20, x1 - 20, 24):
        d.line((i, y0, i + 10, y0), fill=(210, 210, 210), width=2)
        d.line((i, y1, i + 10, y1), fill=(210, 210, 210), width=2)
    icon_arrow_up(d, (x0 + x1) // 2 - 10, y0 + 44)
    t(d, ((x0 + x1) // 2, y0 + 100), "拖入文件或文件夹", 16, TEXT, True, True, "mm")
    t(d, ((x0 + x1) // 2, y0 + 126), "支持多选 · 目录原样保留", 11, SUBTLE, False, True, "mm")
    rr(d, ((x0 + x1) // 2 - 170, y0 + 148, (x0 + x1) // 2 - 10, y0 + 182), 6, ACCENT)
    t(d, ((x0 + x1) // 2 - 90, y0 + 158), "选择文件", 12, (255, 255, 255), True, True, "mm")
    rr(d, ((x0 + x1) // 2 + 10, y0 + 148, (x0 + x1) // 2 + 170, y0 + 182), 6, SURFACE, BORDER, 1)
    t(d, ((x0 + x1) // 2 + 90, y0 + 158), "选择文件夹", 12, TEXT, False, True, "mm")
    badge(d, (x1 - 16, y0 + 10), 3)

    # 待发送列表（与右列底部对齐）
    rr(d, (100, 318, 840, 668), 8, SURFACE, BORDER)
    t(d, (116, 330), "待发送 · 3 项 · 4.2 GB", 13, TEXT, True)
    t(d, (820, 332), "清空", 11, ACCENT, False, True, "ra")
    rows = [("Minecraft1.21Fabric", "文件夹 · 2,954 项 · 3.9 GB", "folder"),
            ("mods", "文件夹 · 128 项 · 240 MB", "folder"),
            ("README.txt", "文本 · 2 KB", "file")]
    for i, (name, meta, kind) in enumerate(rows):
        y = 358 + i * 54
        rr(d, (112, y, 828, y + 46), 6, (250, 250, 250))
        (icon_folder if kind == "folder" else icon_doc)(d, 124, y + 13)
        t(d, (152, y + 7), name, 12, TEXT)
        t(d, (152, y + 25), meta, 10, SUBTLE)
        t(d, (806, y + 22), "移除", 10, SUBTLE, False, True, "rm")
    t(d, (116, 536), "拖入区域仍可继续追加内容", 10, SUBTLE, False, True)
    rr(d, (112, 612, 300, 644), 6, ACCENT)
    t(d, (206, 622), "选择设备发送", 12, (255, 255, 255), True, True, "mm")
    t(d, (312, 622), "或点击右侧设备卡片直接发送", 10, SUBTLE, False, True, "lm")
    badge(d, (824, 322), 4)

    # 设备面板
    px0 = 864
    rr(d, (px0, 318, 1216, 560), 8, SURFACE, BORDER)
    t(d, (px0 + 16, 330), "附近设备", 13, TEXT, True)
    rr(d, (px0 + 176, 328, px0 + 240, 354), 5, (246, 246, 246))
    t(d, (px0 + 208, 334), "刷新", 10, MUTED, False, True, "mm")
    badge(d, (1216 - 24, 326), 5)
    devices = [("WIN-2H1782KNJDF", "192.168.100.5:53321", "在线"),
               ("WSL-Ubuntu", "172.18.249.169:53311", "在线"),
               ("DESKTOP-7F2", "192.168.100.23:53311", "空闲")]
    for i, (n, ip, st) in enumerate(devices):
        y = 366 + i * 62
        rr(d, (px0 + 12, y, px0 + 348, y + 52), 6, (250, 250, 250))
        icon_pc(d, px0 + 24, y + 14)
        t(d, (px0 + 56, y + 8), n, 11, TEXT)
        t(d, (px0 + 56, y + 27), ip, 9, SUBTLE, False, False)
        rr(d, (px0 + 236, y + 14, px0 + 292, y + 38), 5, ACCENT_SOFT)
        t(d, (px0 + 264, y + 21), st, 9, ACCENT, False, True, "mm")
        rr(d, (px0 + 300, y + 12, px0 + 344, y + 40), 5, ACCENT)
        t(d, (px0 + 322, y + 20), "发送", 10, (255, 255, 255), False, True, "mm")
    t(d, (px0 + 16, 560 - 26), "附近没有设备时，可用下方 IP 直连", 10, SUBTLE, False, True)

    # IP 直连
    rr(d, (px0, 584, 1216, 668), 8, SURFACE, BORDER)
    t(d, (px0 + 16, 594), "IP 直连", 12, TEXT, True)
    rr(d, (px0 + 16, 614, px0 + 250, 646), 5, (250, 250, 250), BORDER)
    t(d, (px0 + 26, 622), "只输尾号，如 5 / 005", 10, SUBTLE, False, True)
    rr(d, (px0 + 258, 614, px0 + 344, 646), 5, ACCENT)
    t(d, (px0 + 301, 622), "连接", 11, (255, 255, 255), True, True, "mm")
    badge(d, (1216 - 24, 592), 6)

    # 状态栏
    rr(d, (76, H - 28, W, H), 0, (250, 250, 250))
    d.ellipse((90, H - 19, 98, H - 11), fill=GREEN)
    t(d, (104, H - 21), "发现中 · 2 台设备", 10, MUTED, False, True)
    t(d, (W - 16, H - 21), "v0.1.0 · M2", 9, SUBTLE, False, True, "ra")
    badge(d, (90, H - 52), 7)

    # 标注图例
    canvas = Image.new("RGB", (W, H + 150), (255, 255, 255))
    canvas.paste(img, (0, 0))
    dd = ImageDraw.Draw(canvas)
    legend(dd, (24, H + 14), [
        (1, "标题栏：应用标识 + 本机身份胶囊 + 设置入口（不使用 emoji 图标）"),
        (2, "左侧 NavigationView：发送 / 接收 / 历史 / 设置，选中项用 3px 强调条"),
        (3, "拖放区：唯一主视觉；支持拖入/点选两种入口，空态文案 ≤ 2 行"),
        (4, "待发送列表：名称 + 类型/数量/大小；行内移除；底部主按钮（可点设备卡直接发送）"),
        (5, "附近设备：刷新 + 设备卡（名称/IP/状态/发送）；在线状态用文字+颜色双编码"),
        (6, "IP 直连：只输 IPv4 末段 1–3 位（即设备短 ID #NNN），扫描本机全部子网；失败时就地提示"),
        (7, "状态栏：发现状态 + 版本；不放大段说明文字"),
    ], width=1180)
    canvas.save(os.path.join(OUT, "wireframe-send.png"))
    return canvas


# ---------------------------------------------------------------- B 接收页
def screen_receive():
    W, H = 1240, 800
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    rr(d, (0, 0, W, 48), 0, SURFACE)
    rr(d, (16, 12, 42, 38), 8, ACCENT)
    t(d, (23, 16), "LP", 12, (255, 255, 255), True, False)
    t(d, (50, 15), "LanPipe", 16, TEXT, True)
    t(d, (130, 19), "局域网传输", 11, SUBTLE)
    t(d, (W - 120, 19), "接收", 12, MUTED, True, True, "ra")
    # nav
    rr(d, (0, 48, 76, H), 0, (250, 250, 250))
    nav = [("send", "发送", False), ("recv", "接收", True), ("hist", "历史", False), ("set", "设置", False)]
    for i, (kind, label, sel) in enumerate(nav):
        y = 72 + i * 78
        if sel:
            d.rectangle((0, y - 6, 3, y + 52), fill=ACCENT)
            rr(d, (10, y - 6, 66, y + 52), 6, ACCENT_SOFT)
        (icon_send if kind == "send" else icon_arrow_up if kind == "recv" else icon_history if kind == "hist" else icon_gear)(d, 26, y + 6)
        t(d, (38, y + 32), label, 10, ACCENT if sel else MUTED, True, True, "mm")

    t(d, (100, 66), "接收", 20, TEXT, True)
    t(d, (100, 94), "本机设备名 WIN-2H1782KNJDF · 端口 53311", 11, MUTED)

    # 未完成传输横幅
    rr(d, (100, 118, 1216, 196), 8, ACCENT_SOFT, (198, 224, 245))
    icon_pc(d, 116, 136)
    t(d, (148, 128), "未完成传输", 13, ACCENT, True)
    t(d, (148, 150), "来自 WSL-Ubuntu · Minecraft1.21Fabric · 863/2,954 文件（29%）", 11, TEXT)
    t(d, (148, 170), "等待发送方继续，接收端会自动跳过已完成文件", 10, MUTED, False, True)
    rr(d, (1050, 140, 1128, 170), 5, SURFACE, BORDER)
    t(d, (1089, 148), "清除记录", 10, MUTED, False, True, "mm")
    rr(d, (1136, 140, 1212, 170), 5, ACCENT)
    t(d, (1174, 148), "查看位置", 10, (255, 255, 255), False, True, "mm")
    badge(d, (1216 - 22, 122), 1)

    # 当前传输
    rr(d, (100, 212, 1216, 386), 8, SURFACE, BORDER)
    t(d, (116, 224), "接收中", 13, TEXT, True)
    t(d, (172, 226), "来自 WIN-2H1782KNJDF", 10, MUTED, False, True)
    rr(d, (1000, 220, 1090, 248), 5, (246, 246, 246))
    t(d, (1045, 227), "暂停", 10, SUBTLE, False, True, "mm")
    rr(d, (1098, 220, 1210, 248), 5, (246, 246, 246))
    t(d, (1154, 227), "取消", 10, SUBTLE, False, True, "mm")
    rr(d, (116, 268, 1208, 280), 6, (234, 234, 234))
    rr(d, (116, 268, 800, 280), 6, ACCENT)
    t(d, (116, 292), "1,842 / 2,954 文件 · 2.4 GB / 4.2 GB", 11, TEXT)
    t(d, (760, 292), "86 MB/s · 剩余 12 秒", 11, MUTED, False, True)
    t(d, (116, 318), "Minecraft1.21Fabric/mods/sodium-fabric-0.5.8.jar", 10, SUBTLE, False, False)
    rr(d, (116, 340, 320, 368), 5, (246, 246, 246))
    t(d, (218, 347), "打开接收文件夹", 10, MUTED, False, True, "mm")
    badge(d, (1086, 216), 2)

    # 保存位置
    rr(d, (100, 398, 1216, 462), 8, SURFACE, BORDER)
    t(d, (116, 408), "保存到", 11, MUTED, True, True)
    t(d, (172, 408), r"C:\Users\Administrator\Downloads\LanPipe", 11, TEXT, False, False)
    rr(d, (1040, 406, 1108, 434), 5, (246, 246, 246))
    t(d, (1074, 413), "更改", 10, MUTED, False, True, "mm")
    rr(d, (1116, 406, 1208, 434), 5, (246, 246, 246))
    t(d, (1162, 413), "打开", 10, MUTED, False, True, "mm")
    badge(d, (1216 - 22, 402), 3)

    # 最近接收
    rr(d, (100, 474, 1216, 700), 8, SURFACE, BORDER)
    t(d, (116, 486), "最近接收", 13, TEXT, True)
    t(d, (1160, 488), "全部", 10, ACCENT, False, True, "ra")
    items = [("Minecraft1.21Fabric", "完成 · 2,954 文件 · 3.9 GB", "10:42", GREEN),
             ("1.12.2", "已中断 · 488/488", "昨天 22:10", (196, 43, 28)),
             ("server-backup.zip", "完成 · 1 文件 · 2.1 GB", "昨天 21:05", GREEN)]
    for i, (n, meta, tm, col) in enumerate(items):
        y = 512 + i * 58
        rr(d, (112, y, 1212, y + 50), 6, (250, 250, 250))
        icon_folder(d, 124, y + 14)
        t(d, (152, y + 7), n, 11, TEXT)
        t(d, (152, y + 26), meta, 10, SUBTLE, False, True)
        d.ellipse((1050, y + 20, 1058, y + 28), fill=col)
        t(d, (1066, y + 18), tm, 9, SUBTLE, False, True)
    badge(d, (1216 - 22, 470), 4)

    rr(d, (76, H - 28, W, H), 0, (250, 250, 250))
    t(d, (90, H - 21), "已写入 867 个文件 · 校验通过 1,842", 10, MUTED, False, True)
    t(d, (W - 16, H - 21), "接收端自动适配并发", 9, SUBTLE, False, True, "ra")

    canvas = Image.new("RGB", (W, H + 140), (255, 255, 255))
    canvas.paste(img, (0, 0))
    dd = ImageDraw.Draw(canvas)
    legend(dd, (24, H + 14), [
        (1, "未完成传输横幅：只在存在 journal 时出现；说明「自动续传」而不是给按钮"),
        (2, "当前传输卡：进度/百分比/文件数/速度/剩余时间；暂停与取消先占位（M3 实现）"),
        (3, "保存位置：行内只读路径 + 更改/打开；不写解释性文字"),
        (4, "最近接收：状态点 + 文字双编码；中断项可点开清除或等待续传"),
    ], width=1180)
    canvas.save(os.path.join(OUT, "wireframe-receive.png"))
    return canvas


# ---------------------------------------------------------------- C 设置页
def screen_settings():
    W, H = 1240, 900
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    rr(d, (0, 0, W, 48), 0, SURFACE)
    rr(d, (16, 12, 42, 38), 8, ACCENT)
    t(d, (23, 16), "LP", 12, (255, 255, 255), True, False)
    t(d, (50, 15), "LanPipe", 16, TEXT, True)
    t(d, (130, 19), "局域网传输", 11, SUBTLE)
    rr(d, (0, 48, 76, H), 0, (250, 250, 250))
    nav = [("send", "发送", False), ("recv", "接收", False), ("hist", "历史", False), ("set", "设置", True)]
    for i, (kind, label, sel) in enumerate(nav):
        y = 72 + i * 78
        if sel:
            d.rectangle((0, y - 6, 3, y + 52), fill=ACCENT)
            rr(d, (10, y - 6, 66, y + 52), 6, ACCENT_SOFT)
        (icon_send if kind == "send" else icon_arrow_up if kind == "recv" else icon_history if kind == "hist" else icon_gear)(d, 26, y + 6)
        t(d, (38, y + 32), label, 10, ACCENT if sel else MUTED, True, True, "mm")

    t(d, (100, 66), "设置", 20, TEXT, True)
    t(d, (100, 94), "设备 · 传输 · 网络 · 外观", 11, MUTED)

    def section(y, title, rows, tag=None):
        rr(d, (100, y, 1216, y + 34 + len(rows) * 54 + 8), 8, SURFACE, BORDER)
        t(d, (116, y + 10), title, 12, ACCENT, True)
        if tag:
            badge(d, (1216 - 22, y + 6), tag)
        for i, (label, value, kind) in enumerate(rows):
            yy = y + 40 + i * 54
            if i:
                d.line((116, yy - 6, 1208, yy - 6), fill=(240, 240, 240))
            t(d, (116, yy + 6), label, 11, TEXT)
            if kind == "input":
                rr(d, (560, yy, 1000, yy + 32), 5, (250, 250, 250), BORDER)
                t(d, (572, yy + 8), value, 11, TEXT)
            elif kind == "value":
                t(d, (560, yy + 7), value, 11, MUTED)
            elif kind == "button":
                rr(d, (560, yy, 680, yy + 32), 5, (246, 246, 246), BORDER)
                t(d, (620, yy + 8), value, 10, MUTED, False, True, "mm")
            elif kind == "toggle":
                rr(d, (560, yy + 4, 600, yy + 28), 12, ACCENT)
                d.ellipse((578, yy + 6, 598, yy + 26), fill=(255, 255, 255))
                t(d, (612, yy + 7), value, 10, SUBTLE)
            elif kind == "select":
                rr(d, (560, yy, 760, yy + 32), 5, (250, 250, 250), BORDER)
                t(d, (572, yy + 8), value, 11, TEXT)
                t(d, (744, yy + 8), "▾", 10, SUBTLE, False, False, "ra")
            elif kind == "slider":
                d.line((560, yy + 16, 760, yy + 16), fill=(214, 214, 214), width=2)
                d.ellipse((672, yy + 8, 688, yy + 24), fill=ACCENT)
                t(d, (776, yy + 7), value, 11, MUTED)
        return y + 34 + len(rows) * 54 + 8

    y = 120
    y = section(y, "设备", [
        ("设备名称", "WIN-2H1782KNJDF", "input"),
        ("设备 ID", "3d26c122-…-397fae734866  复制 / 重置", "value"),
    ], 1)
    y += 16
    y = section(y, "传输", [
        ("接收目录", r"C:\Users\Administrator\Downloads\LanPipe  更改", "value"),
        ("并发流数", "4", "slider"),
        ("压缩", "关闭（推荐游戏目录）", "select"),
        ("断点续传", "开启 · 保留 7 天", "toggle"),
        ("传输后校验", "快速（xxh64）", "select"),
    ], 2)
    y += 16
    y = section(y, "网络", [
        ("设备发现", "开启", "toggle"),
        ("UDP 发现端口", "53310", "input"),
        ("TCP 传输端口", "53311", "input"),
        ("防火墙", "一键放行（当前专用网络）", "button"),
        ("网络诊断", "打开诊断页", "button"),
    ], 3)
    y += 16
    y = section(y, "外观与语言", [
        ("主题", "跟随系统", "select"),
        ("语言", "中文（简体）", "select"),
    ], 4)
    y += 16
    y = section(y, "关于", [
        ("版本", "0.1.0 (M2)", "value"),
        ("开源许可", "MIT", "value"),
        ("检查更新", "检查", "button"),
    ], 5)

    canvas = Image.new("RGB", (W, H + 120), (255, 255, 255))
    canvas.paste(img, (0, 0))
    dd = ImageDraw.Draw(canvas)
    legend(dd, (24, H + 14), [
        (1, "分组卡片：标题 + 行；每行「标签 + 控件」，控件右对齐，行高 54（8px 网格）"),
        (2, "断点续传开关：默认开启；保留天数在子菜单，不解释原理"),
        (3, "网络：端口可直接编辑；防火墙按钮只对专用网络生效，需管理员时给出一次提示"),
        (4, "语言：当前只有中文（简体）；语言包来自 i18n XML，切换后即时重载界面"),
        (5, "关于：版本/许可/更新；不放大段致谢或说明"),
    ], width=1180)
    canvas.save(os.path.join(OUT, "wireframe-settings.png"))
    return canvas


# ---------------------------------------------------------------- D 移动端
def screen_mobile():
    W, H = 980, 900
    img = Image.new("RGB", (W, H), (255, 255, 255))
    d = ImageDraw.Draw(img)
    d.rectangle((0, 0, W, H), fill=(255, 255, 255))

    def phone(x, y, title):
        w, h = 400, 780
        rr(d, (x, y, x + w, y + h), 22, (250, 250, 250), (210, 210, 210), 2)
        rr(d, (x + 10, y + 12, x + w - 10, y + 66), 10, SURFACE)
        t(d, (x + 24, y + 24), "LanPipe", 14, TEXT, True)
        t(d, (x + 24, y + 44), title, 10, SUBTLE, False, True)
        icon_gear(d, x + w - 44, y + 26)
        # bottom tab bar
        rr(d, (x + 10, y + h - 54, x + w - 10, y + h - 10), 12, SURFACE, BORDER)
        for i, label in enumerate(["发送", "接收", "历史", "设置"]):
            cx = x + 60 + i * 90
            col = ACCENT if (title == "发送" and label == "发送") or (title == "接收" and label == "接收") else MUTED
            d.ellipse((cx - 5, y + h - 46, cx + 5, y + h - 36), outline=col, width=2)
            t(d, (cx, y + h - 28), label, 8, col, False, True, "mm")
        return w, h

    # 左：发送（移动端没有拖入，只有文件选择）
    phone(40, 40, "发送")
    x, y = 40, 40
    rr(d, (x + 24, y + 92, x + 376, y + 208), 12, SURFACE, BORDER)
    t(d, (x + 200, y + 112), "选择要发送的内容", 13, TEXT, True, True, "mm")
    rr(d, (x + 44, y + 132, x + 188, y + 172), 6, ACCENT)
    t(d, (x + 116, y + 143), "选择文件", 11, (255, 255, 255), True, True, "mm")
    rr(d, (x + 212, y + 132, x + 356, y + 172), 6, SURFACE, BORDER)
    t(d, (x + 284, y + 143), "选择文件夹", 11, TEXT, False, True, "mm")
    t(d, (x + 200, y + 188), "支持多选 · 目录保留原结构", 9, SUBTLE, False, True, "mm")
    t(d, (x + 24, y + 228), "待发送 · 3 项 · 4.2 GB", 11, TEXT, True, True)
    rows = [("Minecraft1.21Fabric", "文件夹 · 2,954 项 · 3.9 GB"),
            ("mods", "文件夹 · 128 项 · 240 MB"),
            ("README.txt", "文本 · 2 KB")]
    for i, (n, meta) in enumerate(rows):
        yy = y + 248 + i * 54
        rr(d, (x + 24, yy, x + 376, yy + 46), 8, SURFACE, BORDER)
        icon_folder(d, x + 36, yy + 13)
        t(d, (x + 62, yy + 9), n, 10, TEXT)
        t(d, (x + 62, yy + 26), meta, 8, SUBTLE, False, True)
    t(d, (x + 24, y + 430), "附近设备", 11, TEXT, True, True)
    for i, n in enumerate(["WIN-2H1782KNJDF", "WSL-Ubuntu"]):
        yy = y + 450 + i * 58
        rr(d, (x + 24, yy, x + 376, yy + 50), 8, SURFACE, BORDER)
        icon_pc(d, x + 36, yy + 15)
        t(d, (x + 64, yy + 9), n, 10, TEXT)
        t(d, (x + 64, yy + 26), "#005 · 在线", 8, SUBTLE, False, True)
        rr(d, (x + 300, yy + 11, x + 368, yy + 39), 6, ACCENT)
        t(d, (x + 334, yy + 18), "发送", 9, (255, 255, 255), False, True, "mm")
    rr(d, (x + 24, y + 578, x + 376, y + 640), 10, SURFACE, BORDER)
    t(d, (x + 40, y + 590), "IP 直连", 10, TEXT, True, True)
    t(d, (x + 40, y + 610), "输入设备短 ID（如 005）", 9, SUBTLE, False, True)

    # 右：接收
    phone(520, 40, "接收")
    x, y = 520, 40
    rr(d, (x + 20, y + 84, x + 380, y + 168), 10, ACCENT_SOFT, (198, 224, 245))
    t(d, (x + 34, y + 94), "未完成传输", 11, ACCENT, True, True)
    t(d, (x + 34, y + 114), "863/2,954 文件 · 29%", 10, TEXT, False, True)
    t(d, (x + 34, y + 134), "等待发送方继续", 9, MUTED, False, True)
    rr(d, (x + 20, y + 180, x + 380, y + 330), 10, SURFACE, BORDER)
    t(d, (x + 34, y + 190), "接收中 · 86 MB/s", 11, TEXT, True, True)
    rr(d, (x + 34, y + 216, x + 366, y + 226), 5, (234, 234, 234))
    rr(d, (x + 34, y + 216, x + 240, y + 226), 5, ACCENT)
    t(d, (x + 34, y + 236), "1,842 / 2,954 文件", 9, MUTED, False, True)
    t(d, (x + 34, y + 252), "2.4 / 4.2 GB · 剩余 12 秒", 9, SUBTLE, False, True)
    rr(d, (x + 34, y + 274, x + 200, y + 306), 6, (246, 246, 246))
    t(d, (x + 117, y + 283), "打开接收文件夹", 10, MUTED, False, True, "mm")
    t(d, (x + 24, y + 348), "保存到", 10, MUTED, True, True)
    t(d, (x + 24, y + 368), "Downloads/LanPipe", 9, TEXT, False, False)
    t(d, (x + 24, y + 400), "最近接收", 11, TEXT, True, True)
    for i, (n, st) in enumerate([("Minecraft1.21Fabric", "完成"), ("1.12.2", "已中断")]):
        yy = y + 420 + i * 52
        rr(d, (x + 20, yy, x + 380, yy + 44), 8, SURFACE, BORDER)
        icon_folder(d, x + 32, yy + 12)
        t(d, (x + 58, yy + 8), n, 10, TEXT)
        t(d, (x + 58, yy + 25), st, 8, GREEN if st == "完成" else (196, 43, 28), False, True)

    dd = ImageDraw.Draw(img)
    t(dd, (40, 846), "移动端（<640px）：底部 Tab 替代左侧导航；无拖入，仅「选择文件 / 选择文件夹」；单列布局；触控目标 ≥44px；间距统一 24px", 12, TEXT, False, True)
    img.save(os.path.join(OUT, "wireframe-mobile.png"))
    return img


if __name__ == "__main__":
    screen_send()
    screen_receive()
    screen_settings()
    screen_mobile()
    print("wireframes written to", OUT)
