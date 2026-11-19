"""生成 LanPipe 占位图标（M0 用）。用法：python tools/make_icons.py"""

from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent.parent / "apps" / "desktop" / "src-tauri" / "icons"
SIZE = 1024
WHITE = (255, 255, 255, 255)


def lerp(a: int, b: int, t: float) -> int:
    return round(a + (b - a) * t)


def rounded_mask(size: int, radius: int) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1], radius=radius, fill=255)
    return mask


def build_master() -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    top, bottom = (47, 111, 237), (26, 74, 190)
    grad = Image.new("RGB", (SIZE, SIZE))
    gd = ImageDraw.Draw(grad)
    for y in range(SIZE):
        t = y / (SIZE - 1)
        gd.line([(0, y), (SIZE, y)], fill=(lerp(top[0], bottom[0], t), lerp(top[1], bottom[1], t), lerp(top[2], bottom[2], t)))
    img.paste(grad, (0, 0), rounded_mask(SIZE, 220))

    draw = ImageDraw.Draw(img)
    bar = 96  # 箭杆厚度

    # 上箭头：向右
    draw.rounded_rectangle([210, 352 - bar // 2, 620, 352 + bar // 2], radius=bar // 2, fill=WHITE)
    draw.polygon([(600, 352 - 150), (830, 352), (600, 352 + 150)], fill=WHITE)

    # 下箭头：向左
    draw.rounded_rectangle([404, 672 - bar // 2, 814, 672 + bar // 2], radius=bar // 2, fill=WHITE)
    draw.polygon([(424, 672 - 150), (194, 672), (424, 672 + 150)], fill=WHITE)
    return img


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    master = build_master()
    master.save(OUT / "icon.png")
    for name, px in {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon-512.png": 512,
    }.items():
        master.resize((px, px), Image.LANCZOS).save(OUT / name)
    master.resize((256, 256), Image.LANCZOS).save(
        OUT / "icon.ico",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print(f"icons written to {OUT}")


if __name__ == "__main__":
    main()
