"""Genera las imágenes de prueba del chat y el RGBA que devuelve Pillow.

Las imágenes se decodifican en `tests/chat_image_formats.rs` y se comparan con
el `.rgba` de al lado (lo que pinta el Python con ``convert("RGBA")``).

    C:/Users/JDMoreu/Documents/EzAgent/.venv/Scripts/python.exe generate.py
"""

from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
SIZE = (8, 6)


def base() -> Image.Image:
    """Degradado con tres bloques planos: textura suficiente para JPEG."""
    image = Image.new("RGBA", SIZE)
    pixels = []
    for y in range(SIZE[1]):
        for x in range(SIZE[0]):
            if x < 3 and y < 2:
                pixels.append((220, 30, 40, 255))
            elif x >= 5 and y >= 4:
                pixels.append((20, 160, 90, 255))
            else:
                pixels.append((8 * x + 4 * y, 12 * y, 200 - 20 * x, 255))
    image.putdata(pixels)
    return image


def main() -> None:
    image = base()
    image.save(HERE / "gradiente.png")
    image.convert("RGB").save(HERE / "gradiente.jpg", quality=90)
    image.convert("RGB").save(HERE / "perdida.webp", quality=80)
    image.save(HERE / "gradiente.webp", lossless=True)
    image.convert("RGB").save(HERE / "gradiente.bmp")
    # BMP de 32 bits: Pillow lo lee en modo RGB e ignora el canal alfa.
    transparent = base()
    transparent.putpixel((0, 0), (10, 20, 30, 0))
    transparent.save(HERE / "alfa.bmp")

    # Animated files: the chat shows the first frame, like `probe.seek(0)`.
    second = Image.new("RGBA", SIZE, (250, 240, 10, 255))
    frames = [image.convert("RGB").quantize(colors=32), second.convert("RGB").quantize(colors=32)]
    frames[0].save(HERE / "animado.gif", save_all=True, append_images=frames[1:], duration=100)
    image.convert("RGB").save(
        HERE / "animado.webp",
        save_all=True,
        append_images=[second.convert("RGB")],
        lossless=True,
        duration=100,
    )

    gif = image.convert("RGB").quantize(colors=32)
    gif.save(HERE / "gradiente.gif")
    # GIF con transparencia: el índice del primer píxel no se pinta.
    gif_transparent = gif.copy()
    gif_transparent.putpixel((0, 0), 0)
    gif_transparent.save(HERE / "transparente.gif", transparency=0)

    for name in (
        "gradiente.png",
        "gradiente.jpg",
        "perdida.webp",
        "gradiente.webp",
        "gradiente.bmp",
        "alfa.bmp",
        "gradiente.gif",
        "transparente.gif",
        "animado.gif",
        "animado.webp",
    ):
        with Image.open(HERE / name) as probe:
            probe.seek(0)
            rgba = probe.convert("RGBA")
            (HERE / f"{name}.rgba").write_bytes(rgba.tobytes())
            print(name, probe.format, probe.size, "->", rgba.size)


if __name__ == "__main__":
    main()
